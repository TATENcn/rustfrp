//! Verifies the bind/serve split used by the desktop shell.
//!
//! The desktop shell binds `127.0.0.1:0`, reads the assigned port, and only then
//! points a webview at it. That is only sound if an ephemeral bind really yields
//! a reachable loopback API, which is what these tests exercise over a real
//! TCP socket rather than a `tower::ServiceExt` shortcut.

#![cfg(feature = "http-api")]

use rustfrp_client::db::{migrate, Database};
use rustfrp_client::process::manager::ProcessManager;
use rustfrp_client::ClientState;
use rustfrp_common::signal::SignalHandler;
use rustfrp_daemon::api::state::ApiState;
use rustfrp_daemon::api::{bind_api_listener, create_router, NoAuth};
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::RwLock;

async fn test_state(dir: &std::path::Path) -> ApiState {
    let db = Database::open(dir.join("config.db").to_str().unwrap())
        .await
        .unwrap();
    migrate::run(&*db.lock().await).unwrap();
    let process_manager = ProcessManager::new(
        dir.join("runtime"),
        "/nonexistent/frpc".into(),
        SignalHandler::new(),
    );
    ApiState::new(
        db,
        process_manager,
        dir.join("runtime"),
        Arc::new(RwLock::new(ClientState::Ready)),
    )
}

/// Minimal HTTP/1.1 GET: the response body is all these tests assert on, so a
/// dependency-free request avoids pulling a client stack into the test target.
async fn get(addr: SocketAddr, path: &str) -> String {
    let mut stream = TcpStream::connect(addr).await.expect("connect to API");
    stream
        .write_all(
            format!("GET {path} HTTP/1.1\r\nHost: {addr}\r\nConnection: close\r\n\r\n").as_bytes(),
        )
        .await
        .unwrap();

    let mut response = Vec::new();
    stream.read_to_end(&mut response).await.unwrap();
    String::from_utf8_lossy(&response).into_owned()
}

#[tokio::test]
async fn ephemeral_bind_yields_loopback_port() {
    let listener = bind_api_listener("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    assert_eq!(addr.ip(), IpAddr::from([127, 0, 0, 1]));
    assert_ne!(addr.port(), 0, "the OS must assign a real port");
}

#[tokio::test]
async fn served_listener_answers_health_on_assigned_port() {
    let dir = tempfile::tempdir().unwrap();
    let state = test_state(dir.path()).await;

    let listener = bind_api_listener("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        axum::serve(listener, create_router(state, NoAuth).into_make_service())
            .await
            .unwrap();
    });

    // Give the serve future a chance to start accepting before dialing.
    tokio::time::sleep(Duration::from_millis(50)).await;

    let response = get(addr, "/api/v1/health").await;
    assert!(
        response.starts_with("HTTP/1.1 200"),
        "unexpected response: {response}"
    );
    assert!(response.contains("\"status\":\"ok\""), "{response}");

    server.abort();
}
