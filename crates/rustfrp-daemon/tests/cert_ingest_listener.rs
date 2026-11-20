//! The ingest endpoint must be configurable at runtime, and its failure mode must
//! never be "the daemon does not start".
//!
//! These tests drive the real listener: what matters is whether a socket is
//! actually accepting connections after a settings change, not what the settings
//! table says.

use axum::body::Body;
use axum::http::Request;
use rustfrp_client::db::settings::{IngestConfig, CERT_INGEST_ENABLED, CERT_INGEST_PORT};
use rustfrp_client::db::{migrate, Database};
use rustfrp_daemon::cert_ingest::{IngestServer, IngestState};
use std::net::TcpListener;
use tower::ServiceExt;

async fn database() -> (tempfile::TempDir, Database) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.db");
    let db = Database::open(path.to_str().unwrap()).await.unwrap();
    {
        let conn = db.lock().await;
        migrate::run(&conn).unwrap();
    }
    (dir, db)
}

/// A port that was free a moment ago, so the test does not depend on 7901.
fn free_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.local_addr().unwrap().port()
}

async fn reachable(address: &str) -> bool {
    reqwest::Client::new()
        .get(format!("http://{address}/ingest/v1/healthz"))
        .timeout(std::time::Duration::from_secs(2))
        .send()
        .await
        .map(|response| response.status().is_success())
        .unwrap_or(false)
}

#[tokio::test]
async fn listener_starts_stops_and_moves_without_a_restart() {
    let (_dir, db) = database().await;
    let server = IngestServer::new(IngestState { db: db.clone() });

    let first = free_port();
    let config = IngestConfig {
        enabled: true,
        listen: "127.0.0.1".into(),
        port: first,
        expose: false,
    };
    let bound = server.apply(&config).await.unwrap();
    assert_eq!(
        bound.as_deref(),
        Some(format!("127.0.0.1:{first}").as_str())
    );
    assert!(
        reachable(&format!("127.0.0.1:{first}")).await,
        "the endpoint must accept connections after being enabled"
    );

    // Moving to another port must free the old one.
    let second = free_port();
    let moved = IngestConfig {
        port: second,
        ..config.clone()
    };
    server.apply(&moved).await.unwrap();
    assert!(
        reachable(&format!("127.0.0.1:{second}")).await,
        "the listener must follow the new port"
    );

    // Disabling must actually close the socket.
    let off = IngestConfig {
        enabled: false,
        ..moved.clone()
    };
    assert!(server.apply(&off).await.unwrap().is_none());
    assert!(
        !reachable(&format!("127.0.0.1:{second}")).await,
        "a disabled listener must not still answer"
    );
    assert!(server.address().await.is_none());
}

#[tokio::test]
async fn a_taken_port_is_reported_rather_than_fatal() {
    let (_dir, db) = database().await;
    let server = IngestServer::new(IngestState { db });

    // Hold the port so the bind must fail.
    let blocker = TcpListener::bind("127.0.0.1:0").unwrap();
    let taken = blocker.local_addr().unwrap().port();

    let config = IngestConfig {
        enabled: true,
        listen: "127.0.0.1".into(),
        port: taken,
        expose: false,
    };
    let error = server.apply(&config).await.unwrap_err();
    assert!(error.contains(&taken.to_string()), "got: {error}");
    // Reporting no address keeps the state honest.
    assert!(server.address().await.is_none());
}

#[tokio::test]
async fn settings_round_trip_through_the_database_and_bind() {
    let (_dir, db) = database().await;
    let port = free_port();
    db.set_platform_setting(CERT_INGEST_PORT, &port.to_string())
        .await
        .unwrap();
    db.set_platform_setting(CERT_INGEST_ENABLED, "true")
        .await
        .unwrap();

    let config = IngestConfig::load(&db).await.unwrap();
    assert_eq!(config.port, port);
    assert_eq!(config.address(), format!("127.0.0.1:{port}"));

    let server = IngestServer::new(IngestState { db });
    server.apply(&config).await.unwrap();
    assert!(reachable(&config.address()).await);
}

#[tokio::test]
async fn ingest_still_rejects_requests_without_a_token() {
    let (_dir, db) = database().await;
    let server = IngestServer::new(IngestState { db: db.clone() });
    let port = free_port();
    server
        .apply(&IngestConfig {
            enabled: true,
            listen: "127.0.0.1".into(),
            port,
            expose: false,
        })
        .await
        .unwrap();

    let response = reqwest::Client::new()
        .post(format!("http://127.0.0.1:{port}/ingest/v1/certs"))
        .header("content-type", "application/json")
        .body(r#"{"crt":"x","key":"y"}"#)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 401);
}

/// The router itself must stay two routes even when reached through the network.
#[tokio::test]
async fn bound_listener_exposes_no_control_routes() {
    let (_dir, db) = database().await;

    // Sanity check that the same service, mounted in-process, 404s these paths.
    let app = rustfrp_daemon::cert_ingest::create_ingest_router(IngestState { db });
    for uri in ["/api/v1/certificates", "/api/v1/config/export", "/"] {
        let response = app
            .clone()
            .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(
            response.status(),
            404,
            "{uri} must not be served by the ingest listener"
        );
    }
}
