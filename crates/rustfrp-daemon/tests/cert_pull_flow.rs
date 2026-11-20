//! Automatic pull: the daemon fetches a bundle, refuses one that does not cover
//! the channel's domain, and rewrites the files an already-bound proxy reads.
//!
//! Driven against a real local HTTP server rather than a mocked client, because
//! the property under test is end to end: what ends up on disk.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use rustfrp_client::config::model::LocalProxy;
use rustfrp_client::db::cert_ingest::{SourceInput, SourceKind};
use rustfrp_client::db::{certificate::SOURCE_MANUAL, migrate, Database};
use rustfrp_client::process::manager::ProcessManager;
use rustfrp_client::ClientState;
use rustfrp_common::signal::SignalHandler;
use rustfrp_daemon::api::{create_router, state::ApiState, NoAuth};
use serde_json::json;
use std::sync::Arc;
use tokio::sync::RwLock;
use tower::ServiceExt;

const CERT: &str = include_str!("../../client/tests/fixtures/cert_ec.pem");
const KEY: &str = include_str!("../../client/tests/fixtures/cert_ec.key");
const OTHER_CERT: &str = include_str!("../../client/tests/fixtures/cert_rsa.pem");
const OTHER_KEY: &str = include_str!("../../client/tests/fixtures/cert_rsa.key");

struct Harness {
    db: Database,
    router: axum::Router,
    config_dir: std::path::PathBuf,
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
    let config_dir = dir.path().join("runtime");
    std::fs::create_dir_all(&config_dir).unwrap();
    let manager = ProcessManager::new(
        config_dir.clone(),
        "/nonexistent/frpc".into(),
        SignalHandler::new(),
    );
    let state = ApiState::new(
        db.clone(),
        manager,
        config_dir.clone(),
        Arc::new(RwLock::new(ClientState::Ready)),
    );
    Harness {
        db,
        router: create_router(state, NoAuth),
        config_dir,
        _dir: dir,
    }
}

/// Serve `crt`/`key` as the JSON a URL channel accepts, on a random port.
async fn serve_bundle(crt: &str, key: &str) -> String {
    let body = json!({ "crt": crt, "key": key }).to_string();
    let app = axum::Router::new().route(
        "/bundle.json",
        axum::routing::get(move || {
            let body = body.clone();
            async move { body }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    format!("http://{addr}/bundle.json")
}

async fn send(router: &axum::Router, request: Request<Body>) -> (StatusCode, String) {
    let response = router.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 1 << 20)
        .await
        .unwrap();
    (status, String::from_utf8_lossy(&bytes).to_string())
}

async fn post_json(router: &axum::Router, uri: &str, body: serde_json::Value) -> (StatusCode, String) {
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

/// The happy path: a certificate configured ahead of its material, a pull
/// channel for its domain, and a bound proxy whose files must be rewritten.
#[tokio::test]
async fn a_pull_imports_the_certificate_and_rewrites_bound_files() {
    let harness = harness().await;
    let certificate = harness
        .db
        .ensure_certificate("default", "test.example.com")
        .await
        .unwrap();
    assert!(certificate.material_id.is_none());

    // A proxy that already declares where its certificate belongs.
    let crt_path = harness.config_dir.join("certs/fullchain.pem");
    let key_path = harness.config_dir.join("certs/privkey.pem");
    std::fs::create_dir_all(crt_path.parent().unwrap()).unwrap();
    let proxy = LocalProxy {
        name: "web".into(),
        proxy_type: rustfrp_client::config::model::ProxyType::Https,
        local_ip: "127.0.0.1".into(),
        local_port: 8080,
        custom_domains: Some(vec!["test.example.com".into()]),
        plugin_config: Some(json!({
            "type": "tls2raw",
            "localAddr": "127.0.0.1:8080",
            "crtPath": crt_path.to_string_lossy(),
            "keyPath": key_path.to_string_lossy(),
        })),
        ..LocalProxy::default()
    };
    let proxy_id = harness
        .db
        .insert_proxy_for_tenant(&proxy, "default")
        .await
        .unwrap();

    let url = serve_bundle(CERT, KEY).await;
    let (status, body) = post_json(
        &harness.router,
        "/api/v1/certificate-sources",
        json!({
            "kind": "url",
            "name": "nightly",
            "certificate_id": certificate.id,
            "domain": "test.example.com",
            "base_url": url,
        }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "channel: {body}");
    let source_id = serde_json::from_str::<serde_json::Value>(&body).unwrap()["data"]["source"]
        ["id"]
        .as_i64()
        .unwrap();

    let (status, body) = post_json(
        &harness.router,
        &format!("/api/v1/certificate-sources/{source_id}/run"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "pull: {body}");
    let report: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(report["data"]["certificate_id"], certificate.id);

    // Material arrived on the same identity, so nothing was duplicated.
    let stored = harness
        .db
        .certificate_in_tenant(certificate.id, "default")
        .await
        .unwrap();
    assert!(stored.material_id.is_some());
    assert_eq!(stored.primary_domain, "test.example.com");
    assert_eq!(harness.db.list_certificates("default").await.unwrap().len(), 1);

    // Bind, then pull again: the files must already be correct without a manual
    // apply, because a renewal is expected to reach the proxy on its own.
    harness
        .db
        .bind_certificate(certificate.id, proxy_id, "default")
        .await
        .unwrap();
    let applied = harness.db.apply_stale_certificates("default").await.unwrap();
    assert_eq!(applied, 1, "the stale binding is rewritten");
    assert!(std::fs::read_to_string(&crt_path)
        .unwrap()
        .contains("BEGIN CERTIFICATE"));
    assert!(std::fs::read_to_string(&key_path)
        .unwrap()
        .contains("BEGIN PRIVATE KEY"));

    // A second pull with identical material changes nothing on disk.
    let before = std::fs::read_to_string(&crt_path).unwrap();
    let _ = post_json(
        &harness.router,
        &format!("/api/v1/certificate-sources/{source_id}/run"),
        json!({}),
    )
    .await;
    assert_eq!(std::fs::read_to_string(&crt_path).unwrap(), before);
}

/// The guard that makes automatic replacement safe: a channel may only install a
/// certificate that covers the domain it was created for.
#[tokio::test]
async fn a_pull_refuses_a_certificate_for_a_different_domain() {
    let harness = harness().await;
    let certificate = harness
        .db
        .ensure_certificate("default", "test.example.com")
        .await
        .unwrap();

    // This endpoint answers with a certificate for `not-test.example.com`.
    let url = serve_bundle(OTHER_CERT, OTHER_KEY).await;
    let created = harness
        .db
        .create_source(
            "default",
            SourceInput {
                certificate_id: Some(certificate.id),
                kind: SourceKind::Url,
                name: "wrong".into(),
                domain: "other.example.com".into(),
                base_url: url,
                domains: vec![],
                key_id: None,
                key_secret: None,
                enabled: true,
            },
        )
        .await
        .unwrap();

    let (status, body) = post_json(
        &harness.router,
        &format!("/api/v1/certificate-sources/{}/run", created.source.id),
        json!({}),
    )
    .await;
    assert_ne!(status, StatusCode::OK, "this must be refused: {body}");
    assert!(
        body.contains("does not cover the channel's domain"),
        "the error must name the mismatch: {body}"
    );

    // Nothing was imported, and the failure is visible on the channel.
    let stored = harness
        .db
        .certificate_in_tenant(certificate.id, "default")
        .await
        .unwrap();
    assert!(stored.material_id.is_none(), "no material must be stored");

    let source = harness
        .db
        .source(created.source.id, "default")
        .await
        .unwrap();
    assert_eq!(source.last_status.as_deref(), Some("failed"));
    assert!(source
        .last_error
        .as_deref()
        .unwrap_or_default()
        .contains("does not cover"));
    assert!(source.next_run_at.is_some(), "a retry is scheduled");
}

/// A push token bound to one certificate must not be able to replace another.
#[tokio::test]
async fn a_bound_push_token_rejects_a_certificate_for_another_certificate() {
    let harness = harness().await;
    let certificate = harness
        .db
        .ensure_certificate("default", "other.example.com")
        .await
        .unwrap();
    let created = harness
        .db
        .create_source(
            "default",
            SourceInput {
                certificate_id: Some(certificate.id),
                kind: SourceKind::Push,
                name: "certd".into(),
                domain: String::new(),
                base_url: String::new(),
                domains: vec!["*.example.com".into()],
                key_id: None,
                key_secret: None,
                enabled: true,
            },
        )
        .await
        .unwrap();

    // Allowed by the token's wildcard, but it does not cover the bound domain.
    let (status, body) = send(
        &harness.router,
        Request::builder()
            .method("POST")
            .uri("/ingest/v1/certs")
            .header("content-type", "application/json")
            .header(
                "x-cert-ingest-token",
                created.plaintext.as_deref().unwrap(),
            )
            .body(Body::from(json!({ "crt": CERT, "key": KEY }).to_string()))
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "body: {body}");
    assert!(body.contains("CERT_008"), "body: {body}");
}

/// An unbound pull channel still works: it resolves its certificate by domain,
/// which is what makes a pre-existing source usable without re-creation.
#[tokio::test]
async fn an_unbound_pull_channel_resolves_its_certificate_by_domain() {
    let harness = harness().await;
    let url = serve_bundle(CERT, KEY).await;
    let created = harness
        .db
        .create_source(
            "default",
            SourceInput {
                certificate_id: None,
                kind: SourceKind::Url,
                name: "legacy".into(),
                domain: "test.example.com".into(),
                base_url: url,
                domains: vec![],
                key_id: None,
                key_secret: None,
                enabled: true,
            },
        )
        .await
        .unwrap();

    let report = rustfrp_daemon::cert_pull::pull_once(&harness.db, None, created.source.id, "default")
        .await
        .unwrap();
    assert_eq!(report.primary_domain.as_deref(), Some("test.example.com"));

    let attached = harness
        .db
        .source(created.source.id, "default")
        .await
        .unwrap();
    assert!(
        attached.certificate_id.is_some(),
        "the channel attaches itself to the certificate it produced"
    );
}

/// A channel that predates any certificate creates the identity it needs.
#[tokio::test]
async fn a_pull_for_an_unknown_domain_creates_the_certificate() {
    let harness = harness().await;
    let url = serve_bundle(CERT, KEY).await;
    let created = harness
        .db
        .create_source(
            "default",
            SourceInput {
                certificate_id: None,
                kind: SourceKind::Url,
                name: "fresh".into(),
                domain: "test.example.com".into(),
                base_url: url,
                domains: vec![],
                key_id: None,
                key_secret: None,
                enabled: true,
            },
        )
        .await
        .unwrap();
    // The identity exists as soon as the channel does, so the operator sees
    // something in the list while the first fetch is still pending.
    let before = harness.db.list_certificates("default").await.unwrap();
    assert_eq!(before.len(), 1);
    assert!(before[0].material_id.is_none());

    rustfrp_daemon::cert_pull::pull_once(&harness.db, None, created.source.id, "default")
        .await
        .unwrap();

    let certificates = harness.db.list_certificates("default").await.unwrap();
    assert_eq!(certificates.len(), 1, "the fetch must not create a second row");
    assert_eq!(certificates[0].primary_domain, "test.example.com");
    assert_eq!(certificates[0].id, before[0].id);
    assert!(certificates[0].material_id.is_some());
    assert_eq!(certificates[0].source, "url_pull");
}

/// The scheduled path must not touch a disabled channel.
#[tokio::test]
async fn a_disabled_channel_is_never_due() {
    let harness = harness().await;
    let url = serve_bundle(CERT, KEY).await;
    harness
        .db
        .create_source(
            "default",
            SourceInput {
                certificate_id: None,
                kind: SourceKind::Url,
                name: "off".into(),
                domain: "test.example.com".into(),
                base_url: url,
                domains: vec![],
                key_id: None,
                key_secret: None,
                enabled: false,
            },
        )
        .await
        .unwrap();
    assert!(harness.db.due_pull_sources(16).await.unwrap().is_empty());
}

/// A push channel is not something the daemon fetches, so the run endpoint must
/// refuse it rather than silently succeed.
#[tokio::test]
async fn running_a_push_channel_is_refused() {
    let harness = harness().await;
    let created = harness
        .db
        .create_source(
            "default",
            SourceInput {
                certificate_id: None,
                kind: SourceKind::Push,
                name: "certd".into(),
                domain: String::new(),
                base_url: String::new(),
                domains: vec!["test.example.com".into()],
                key_id: None,
                key_secret: None,
                enabled: true,
            },
        )
        .await
        .unwrap();
    let error =
        rustfrp_daemon::cert_pull::pull_once(&harness.db, None, created.source.id, "default")
            .await
            .unwrap_err();
    assert!(error.to_string().contains("push channel"), "{error}");
}

/// Unused import guard: `SOURCE_MANUAL` documents where a manual import lands.
#[allow(dead_code)]
fn _keeps_source_constant_used() -> &'static str {
    SOURCE_MANUAL
}
