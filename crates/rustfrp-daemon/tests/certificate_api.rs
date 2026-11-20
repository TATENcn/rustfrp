//! Certificate API behaviour, including the guarantee that no response carries
//! certificate material.
//!
//! These tests drive the real router: the point is what a caller over HTTP can
//! observe, not what the internal types intend.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use rustfrp_client::db::{migrate, Database};
use rustfrp_client::process::manager::ProcessManager;
use rustfrp_client::ClientState;
use rustfrp_common::signal::SignalHandler;
use rustfrp_daemon::api::{create_router, state::ApiState, NoAuth};
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::RwLock;
use tower::ServiceExt;

const CERT: &str = include_str!("../../client/tests/fixtures/cert_ec.pem");
const KEY: &str = include_str!("../../client/tests/fixtures/cert_ec.key");

struct Harness {
    router: axum::Router,
    _dir: tempfile::TempDir,
}

async fn harness() -> Harness {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.db");
    let db = Database::open(path.to_str().unwrap()).await.unwrap();
    {
        let conn = db.lock().await;
        migrate::run(&conn).unwrap();
    }
    let runtime = dir.path().join("runtime");
    let manager = ProcessManager::new(runtime, "/nonexistent/frpc".into(), SignalHandler::new());
    let state = ApiState::new(
        db,
        manager,
        dir.path().to_path_buf(),
        Arc::new(RwLock::new(ClientState::Ready)),
    );
    Harness {
        router: create_router(state, NoAuth),
        _dir: dir,
    }
}

async fn send(router: &axum::Router, request: Request<Body>) -> (StatusCode, String) {
    let response = router.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 1 << 20)
        .await
        .unwrap();
    (status, String::from_utf8_lossy(&bytes).to_string())
}

async fn post_json(router: &axum::Router, uri: &str, body: Value) -> (StatusCode, String) {
    send(
        router,
        Request::builder()
            .method("POST")
            .uri(uri)
            .header("content-type", "application/json")
            .body(Body::from(body.to_string()))
            .unwrap(),
    )
    .await
}

#[tokio::test]
async fn import_then_read_never_returns_certificate_material() {
    let harness = harness().await;
    let router = &harness.router;

    let (status, body) = post_json(
        router,
        "/api/v1/certificates/pem",
        serde_json::json!({ "cert": CERT, "key": KEY }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "import failed: {body}");

    let imported: Value = serde_json::from_str(&body).unwrap();
    let id = imported["data"]["id"].as_i64().unwrap();
    assert_eq!(imported["data"]["primary_domain"], "test.example.com");

    // The write response, the list, and the detail view must all be material-free.
    for (label, payload) in [
        ("import", body),
        (
            "list",
            send(
                router,
                Request::builder()
                    .uri("/api/v1/certificates")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .1,
        ),
        (
            "detail",
            send(
                router,
                Request::builder()
                    .uri(format!("/api/v1/certificates/{id}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .1,
        ),
    ] {
        for needle in [
            "BEGIN CERTIFICATE",
            "BEGIN PRIVATE KEY",
            "-----BEGIN",
            "cert_pem",
            "key_pem",
            "chain_pem",
            "enc_key",
            "enc_cert",
        ] {
            assert!(
                !payload.contains(needle),
                "{label} response leaked {needle}: {payload}"
            );
        }
    }
}

#[tokio::test]
async fn errors_are_reported_without_echoing_the_submitted_key() {
    let harness = harness().await;
    // A key that does not match the certificate must be rejected with a specific
    // code, and the message must not quote the key back.
    let (status, body) = post_json(
        &harness.router,
        "/api/v1/certificates/pem",
        serde_json::json!({
            "cert": CERT,
            "key": include_str!("../../client/tests/fixtures/cert_rsa.key"),
        }),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert!(body.contains("CERT_003"), "{body}");
    assert!(!body.contains("PRIVATE KEY"), "{body}");
}

#[tokio::test]
async fn duplicate_import_is_idempotent_over_http() {
    let harness = harness().await;
    let body = serde_json::json!({ "cert": CERT, "key": KEY });

    let (_, first) = post_json(&harness.router, "/api/v1/certificates/pem", body.clone()).await;
    let (_, second) = post_json(&harness.router, "/api/v1/certificates/pem", body).await;

    let first: Value = serde_json::from_str(&first).unwrap();
    let second: Value = serde_json::from_str(&second).unwrap();
    assert_eq!(first["data"]["id"], second["data"]["id"]);

    let (_, list) = send(
        &harness.router,
        Request::builder()
            .uri("/api/v1/certificates")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    let list: Value = serde_json::from_str(&list).unwrap();
    assert_eq!(list["count"], 1);
}

#[tokio::test]
async fn certificate_routes_require_write_or_read_scope() {
    use rustfrp_daemon::api::auth::{AuthPolicy, TokenPolicy};
    use rustfrp_daemon::api::ScopedBearerToken;
    use sha2::{Digest, Sha256};

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.db");
    let db = Database::open(path.to_str().unwrap()).await.unwrap();
    {
        let conn = db.lock().await;
        migrate::run(&conn).unwrap();
    }
    let runtime = dir.path().join("runtime");
    let manager = ProcessManager::new(runtime, "/nonexistent/frpc".into(), SignalHandler::new());
    let state = ApiState::new(
        db,
        manager,
        dir.path().to_path_buf(),
        Arc::new(RwLock::new(ClientState::Ready)),
    );

    let reader = "reader-token-value-0123456789abcdef";
    let digest = Sha256::digest(reader.as_bytes())
        .iter()
        .fold(String::new(), |mut out, byte| {
            use std::fmt::Write;
            let _ = write!(out, "{byte:02x}");
            out
        });
    let policy = AuthPolicy {
        tokens: vec![TokenPolicy {
            name: "reader".into(),
            tenant: "default".into(),
            token_sha256: digest,
            scopes: vec!["read".into()],
        }],
    };
    let router = create_router(state, ScopedBearerToken::new(policy));

    let list = send(
        &router,
        Request::builder()
            .uri("/api/v1/certificates")
            .header("authorization", format!("Bearer {reader}"))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(list.0, StatusCode::OK, "read scope must allow listing");

    let write = send(
        &router,
        Request::builder()
            .method("POST")
            .uri("/api/v1/certificates/pem")
            .header("authorization", format!("Bearer {reader}"))
            .header("content-type", "application/json")
            .body(Body::from(
                serde_json::json!({ "cert": CERT, "key": KEY }).to_string(),
            ))
            .unwrap(),
    )
    .await;
    assert_eq!(
        write.0,
        StatusCode::UNAUTHORIZED,
        "a read-only token must not import: {}",
        write.1
    );
}

/// The push route is also mounted on the control API, so a certificate manager
/// can reach it at whatever address the operator already uses for the UI —
/// including a frpc `https` proxy on 443. It must stay token-gated there: the
/// bearer middleware deliberately ignores anything outside `/api/`, so the
/// ingest token is the only thing standing in front of imported material.
#[tokio::test]
async fn the_control_api_serves_the_push_route_and_keeps_its_own_token() {
    let harness = harness().await;
    let router = &harness.router;

    let created = post_json(
        router,
        "/api/v1/cert-ingest-tokens",
        serde_json::json!({
            "kind": "push",
            "name": "certd",
            "domains": ["test.example.com", "alt.example.com"]
        }),
    )
    .await;
    assert_eq!(created.0, StatusCode::CREATED, "token: {}", created.1);
    let token: Value = serde_json::from_str(&created.1).unwrap();
    let plaintext = token["data"]["plaintext"].as_str().unwrap().to_string();

    let without = post_json(
        router,
        "/ingest/v1/certs",
        serde_json::json!({ "crt": CERT, "key": KEY }),
    )
    .await;
    assert_eq!(
        without.0,
        StatusCode::UNAUTHORIZED,
        "the API path must not bypass the ingest token"
    );

    let wrong = send(
        router,
        Request::builder()
            .method("POST")
            .uri("/ingest/v1/certs")
            .header("content-type", "application/json")
            .header("x-cert-ingest-token", "not-a-real-token")
            .body(Body::from(
                serde_json::json!({ "crt": CERT, "key": KEY }).to_string(),
            ))
            .unwrap(),
    )
    .await;
    assert_eq!(wrong.0, StatusCode::UNAUTHORIZED);

    let accepted = send(
        router,
        Request::builder()
            .method("POST")
            .uri("/ingest/v1/certs")
            .header("content-type", "application/json")
            .header("x-cert-ingest-token", &plaintext)
            .body(Body::from(
                serde_json::json!({ "crt": CERT, "key": KEY }).to_string(),
            ))
            .unwrap(),
    )
    .await;
    assert_eq!(accepted.0, StatusCode::OK, "push: {}", accepted.1);
    let pushed: Value = serde_json::from_str(&accepted.1).unwrap();
    assert_eq!(pushed["primary_domain"], "test.example.com");

    let health = send(
        router,
        Request::builder()
            .uri("/ingest/v1/healthz")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(health.0, StatusCode::OK);
}
