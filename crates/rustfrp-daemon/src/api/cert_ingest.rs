//! Certificate channel API.
//!
//! One resource for both directions: a channel is a push token, a Certd pull, or
//! a plain URL pull, and it is either attached to a certificate or unbound. The
//! legacy `/cert-ingest-tokens` and `/cert-pull-sources` routes are aliases of
//! this one so existing automation keeps working.
//!
//! Creating a push channel returns its plaintext token exactly once; every later
//! read carries only `has_secret`. A pull channel's `key_secret` is write-only in
//! the same way.

use axum::extract::{Extension, Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use serde::Deserialize;

use rustfrp_client::db::cert_ingest::{CertificateSource, CreatedSource, SourceInput, SourceKind};

use super::auth::AuthIdentity;
use super::response::{api_error, ApiResponse};
use super::state::ApiState;

type ApiErrorResponse = (StatusCode, Json<ApiResponse<()>>);

#[derive(Debug, Deserialize)]
pub struct ListQuery {
    /// Only channels attached to this certificate.
    #[serde(default)]
    pub certificate_id: Option<i64>,
}

pub async fn list(
    State(state): State<ApiState>,
    Extension(identity): Extension<AuthIdentity>,
    Query(query): Query<ListQuery>,
) -> Result<Json<ApiResponse<Vec<CertificateSource>>>, ApiErrorResponse> {
    let sources = state
        .db
        .list_sources(&identity.tenant, query.certificate_id)
        .await
        .map_err(api_error)?;
    let count = sources.len();
    Ok(Json(ApiResponse::ok_list(sources, count)))
}

pub async fn create(
    State(state): State<ApiState>,
    Extension(identity): Extension<AuthIdentity>,
    Json(input): Json<SourceInput>,
) -> Result<(StatusCode, Json<ApiResponse<CreatedSource>>), ApiErrorResponse> {
    let created = state
        .db
        .create_source(&identity.tenant, input)
        .await
        .map_err(api_error)?;
    Ok((StatusCode::CREATED, Json(ApiResponse::ok(created))))
}

pub async fn delete(
    State(state): State<ApiState>,
    Extension(identity): Extension<AuthIdentity>,
    Path(id): Path<i64>,
) -> Result<StatusCode, ApiErrorResponse> {
    state
        .db
        .delete_source(id, &identity.tenant)
        .await
        .map_err(api_error)?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Deserialize)]
pub struct AttachRequest {
    /// `null` detaches the channel without deleting it.
    pub certificate_id: Option<i64>,
}

/// Attach a channel to a certificate, or detach it.
pub async fn attach(
    State(state): State<ApiState>,
    Extension(identity): Extension<AuthIdentity>,
    Path(id): Path<i64>,
    Json(request): Json<AttachRequest>,
) -> Result<Json<ApiResponse<CertificateSource>>, ApiErrorResponse> {
    let source = state
        .db
        .set_source_certificate(id, request.certificate_id, &identity.tenant)
        .await
        .map_err(api_error)?;
    Ok(Json(ApiResponse::ok(source)))
}

/// Make one channel the certificate's preferred one.
pub async fn promote(
    State(state): State<ApiState>,
    Extension(identity): Extension<AuthIdentity>,
    Path(id): Path<i64>,
) -> Result<Json<ApiResponse<CertificateSource>>, ApiErrorResponse> {
    let source = state
        .db
        .set_primary_source(id, &identity.tenant)
        .await
        .map_err(api_error)?;
    Ok(Json(ApiResponse::ok(source)))
}

/// Fetch once from a pull channel and import what comes back.
pub async fn run(
    State(state): State<ApiState>,
    Extension(identity): Extension<AuthIdentity>,
    Path(id): Path<i64>,
) -> Result<Json<ApiResponse<crate::cert_pull::PullReport>>, ApiErrorResponse> {
    let report = crate::cert_pull::pull_once(
        &state.db,
        Some(&state.process_manager),
        id,
        &identity.tenant,
    )
    .await
    .map_err(api_error)?;
    Ok(Json(ApiResponse::ok(report)))
}

/// Supported kinds, for the UI to render the right form.
pub async fn kinds() -> Json<ApiResponse<Vec<&'static str>>> {
    Json(ApiResponse::ok_list(
        vec![
            SourceKind::Push.as_str(),
            SourceKind::Certd.as_str(),
            SourceKind::Url.as_str(),
        ],
        3,
    ))
}
