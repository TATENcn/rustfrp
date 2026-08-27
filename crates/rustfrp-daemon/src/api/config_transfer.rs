//! Explicit configuration migration and backup endpoints.

use axum::body::Body;
use axum::extract::State;
use axum::http::{header, Response, StatusCode};
use axum::Json;
use serde::Deserialize;

use super::response::{ApiError, ApiResponse};
use super::state::ApiState;

const MAX_TOML_BYTES: usize = 2 * 1024 * 1024;

#[derive(Debug, Deserialize)]
pub struct ImportRequest {
    pub profile_name: String,
    pub toml: String,
}

/// POST /api/v1/config/import — one-shot migration from modern frpc TOML.
pub async fn import(
    State(state): State<ApiState>,
    Json(request): Json<ImportRequest>,
) -> Result<
    (
        StatusCode,
        Json<ApiResponse<rustfrp_client::config::import::ImportSummary>>,
    ),
    (StatusCode, Json<ApiResponse<()>>),
> {
    if request.toml.len() > MAX_TOML_BYTES {
        return Err((
            StatusCode::PAYLOAD_TOO_LARGE,
            Json(ApiResponse {
                success: false,
                data: None,
                count: None,
                error: Some(ApiError::generic(
                    "CFG_IMPORT_TOO_LARGE",
                    format!("TOML input exceeds {MAX_TOML_BYTES} bytes"),
                )),
            }),
        ));
    }

    let summary = state
        .db
        .import_frpc_toml(&request.profile_name, &request.toml)
        .await
        .map_err(|error| {
            (
                super::response::status_code(&error),
                Json(ApiResponse {
                    success: false,
                    data: None,
                    count: None,
                    error: Some(ApiError::from_client_error(&error)),
                }),
            )
        })?;

    Ok((StatusCode::CREATED, Json(ApiResponse::ok(summary))))
}

/// GET /api/v1/config/export — download a consistent SQLite backup.
pub async fn export(State(state): State<ApiState>) -> Response<Body> {
    let bytes = match state.db.export_backup().await {
        Ok(bytes) => bytes,
        Err(error) => {
            let body = serde_json::to_vec(&ApiResponse::<()> {
                success: false,
                data: None,
                count: None,
                error: Some(ApiError::from_client_error(&error)),
            })
            .unwrap_or_default();
            return Response::builder()
                .status(super::response::status_code(&error))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(body))
                .expect("static response headers are valid");
        }
    };

    let filename = format!(
        "rustfrp-backup-{}.sqlite",
        chrono::Utc::now().format("%Y%m%d-%H%M%S")
    );
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/vnd.sqlite3")
        .header(
            header::CONTENT_DISPOSITION,
            format!("attachment; filename=\"{filename}\""),
        )
        .body(Body::from(bytes))
        .expect("static response headers are valid")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn import_request_deserializes() {
        let value: ImportRequest = serde_json::from_value(serde_json::json!({
            "profile_name": "legacy",
            "toml": "serverAddr = \"example.com\""
        }))
        .unwrap();
        assert_eq!(value.profile_name, "legacy");
    }
}
