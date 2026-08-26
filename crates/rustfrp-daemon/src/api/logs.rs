//! Log handlers — read frpc logs for a specific profile
//!
//! Endpoints:
//! - GET /api/v1/logs/{profile_id}         — get combined logs (stdout + stderr)
//! - GET /api/v1/logs/{profile_id}/stdout  — get stdout log only
//! - GET /api/v1/logs/{profile_id}/stderr  — get stderr log only
//!
//! Query parameters:
//! - `lines`: number of lines to read from end (default: 200, max: 1000)
//! - `type`: log type - "combined" (default), "stdout", or "stderr"

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{BufRead, BufReader};

use super::response::{ApiError, ApiResponse};
use super::state::ApiState;

/// Query parameters for log endpoint
#[derive(Debug, Deserialize)]
pub struct LogQuery {
    /// Number of lines to read from end (default: 200, max: 1000)
    #[serde(default = "default_lines")]
    pub lines: u32,
    /// Log type: "combined" (default), "stdout", or "stderr"
    #[serde(default = "default_log_type")]
    pub log_type: String,
}

fn default_lines() -> u32 {
    200
}

fn default_log_type() -> String {
    "combined".to_string()
}

/// Log response
#[derive(Debug, Serialize)]
pub struct LogResponse {
    pub profile_id: i64,
    pub profile_name: String,
    pub log_type: String,
    pub lines: u32,
    pub content: String,
    pub stdout_path: Option<String>,
    pub stderr_path: Option<String>,
    pub exists: bool,
}

/// GET /api/v1/logs/{profile_id} — get logs for a specific profile
pub async fn get_logs(
    State(state): State<ApiState>,
    Path(profile_id): Path<i64>,
    Query(query): Query<LogQuery>,
) -> Result<Json<ApiResponse<LogResponse>>, (StatusCode, Json<ApiResponse<()>>)> {
    // Validate lines parameter
    let lines = query.lines.clamp(1, 1000);

    // Validate log type
    let log_type = match query.log_type.as_str() {
        "combined" | "stdout" | "stderr" => query.log_type.clone(),
        _ => {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(ApiResponse {
                    success: false,
                    data: None,
                    count: None,
                    error: Some(ApiError::generic(
                        "INVALID_LOG_TYPE",
                        "type must be 'combined', 'stdout', or 'stderr'".into(),
                    )),
                }),
            ));
        }
    };

    // Get profile name
    let profile = state.db.get_profile(profile_id).await.map_err(|e| {
        (
            super::response::status_code(&e),
            Json(ApiResponse {
                success: false,
                data: None,
                count: None,
                error: Some(ApiError::from_client_error(&e)),
            }),
        )
    })?;

    // Get log paths
    let stdout_path = state.process_manager.get_stdout_log_path(profile_id).await;
    let stderr_path = state.process_manager.get_stderr_log_path(profile_id).await;

    // Determine which log to read
    let content = match log_type.as_str() {
        "stdout" => {
            if let Some(path) = &stdout_path {
                read_log_tail(path, lines)
            } else {
                String::new()
            }
        }
        "stderr" => {
            if let Some(path) = &stderr_path {
                read_log_tail(path, lines)
            } else {
                String::new()
            }
        }
        "combined" => {
            // Combine stdout and stderr, interleaved by time is complex,
            // so we just concatenate stderr after stdout
            let stdout_content = stdout_path
                .as_ref()
                .map(|p| read_log_tail(p, lines / 2))
                .unwrap_or_default();
            let stderr_content = stderr_path
                .as_ref()
                .map(|p| read_log_tail(p, lines / 2))
                .unwrap_or_default();
            if stdout_content.is_empty() && stderr_content.is_empty() {
                String::new()
            } else if stdout_content.is_empty() {
                format!("--- STDERR ---\n{}", stderr_content)
            } else if stderr_content.is_empty() {
                stdout_content
            } else {
                format!(
                    "--- STDOUT ---\n{}\n--- STDERR ---\n{}",
                    stdout_content, stderr_content
                )
            }
        }
        _ => String::new(),
    };

    // Check if log files exist
    let exists = match log_type.as_str() {
        "stdout" => stdout_path.as_ref().map(|p| p.exists()).unwrap_or(false),
        "stderr" => stderr_path.as_ref().map(|p| p.exists()).unwrap_or(false),
        "combined" => {
            stdout_path.as_ref().map(|p| p.exists()).unwrap_or(false)
                || stderr_path.as_ref().map(|p| p.exists()).unwrap_or(false)
        }
        _ => false,
    };

    Ok(Json(ApiResponse::ok(LogResponse {
        profile_id,
        profile_name: profile.name,
        log_type,
        lines,
        content,
        stdout_path: stdout_path.map(|p| p.to_string_lossy().to_string()),
        stderr_path: stderr_path.map(|p| p.to_string_lossy().to_string()),
        exists,
    })))
}

/// Read the last N lines from a log file
fn read_log_tail(path: &std::path::Path, lines: u32) -> String {
    if !path.exists() {
        return String::new();
    }

    let file = fs::File::open(path);
    if let Err(e) = file {
        tracing::error!(error = %e, path = %path.display(), "Failed to open log file");
        return format!("Error reading log: {e}");
    }

    let file = file.unwrap();
    let reader = BufReader::new(file);

    // Read all lines and take the last N
    let all_lines: Vec<String> = reader.lines().map_while(Result::ok).collect();

    let start = if all_lines.len() > lines as usize {
        all_lines.len() - lines as usize
    } else {
        0
    };

    all_lines[start..].join("\n")
}
