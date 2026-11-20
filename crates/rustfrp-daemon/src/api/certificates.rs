//! Certificate management API.
//!
//! Every response carries [`Certificate`] metadata only. The private key and the
//! certificate body never cross this boundary: `CertificateSecrets` has no
//! `Serialize` implementation, so an accidental `Json(secrets)` does not compile.
//!
//! Endpoints:
//! - `GET    /api/v1/certificates`              — list with bindings and expiry
//! - `GET    /api/v1/certificates/:id`          — one certificate
//! - `POST   /api/v1/certificates/pem`          — import a pasted bundle
//! - `POST   /api/v1/certificates/from-path`    — import from files on this host
//! - `POST   /api/v1/certificates/:id/apply`    — write to every bound proxy
//! - `DELETE /api/v1/certificates/:id`          — delete (refused while bound)
//! - `POST   /api/v1/certificate-bindings`      — bind to a proxy
//! - `DELETE /api/v1/certificate-bindings/:id`  — unbind

use axum::extract::{Extension, Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};

use rustfrp_client::cert::parse_bundle;
use rustfrp_client::db::certificate::{
    ImportRequest, SOURCE_LOCAL_PATH, SOURCE_MANUAL, SOURCE_UPLOAD,
};

use super::auth::AuthIdentity;
use super::response::{api_error, ApiResponse};
use super::state::ApiState;

/// Axum error tuple used by every handler in this module.
type ApiErrorResponse = (StatusCode, Json<ApiResponse<()>>);

/// Request body for pasted-PEM import.
#[derive(Debug, Deserialize)]
pub struct PemImportRequest {
    /// Leaf certificate, or a fullchain, PEM or bare base64 DER.
    pub cert: String,
    /// Private key, PEM or bare base64 DER.
    pub key: String,
    #[serde(default)]
    pub note: Option<String>,
}

/// Request body for file import on the daemon host.
#[derive(Debug, Deserialize)]
pub struct PathImportRequest {
    pub crt_path: String,
    pub key_path: String,
    #[serde(default)]
    pub note: Option<String>,
}

/// Request body for upload import (already base64-decoded by the client).
#[derive(Debug, Deserialize)]
pub struct UploadImportRequest {
    /// Certificate text, PEM or base64 DER.
    pub cert: String,
    pub key: String,
    #[serde(default)]
    pub filename: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ListQuery {
    /// Only certificates whose `not_after` is within this many days.
    #[serde(default)]
    pub expiring_within: Option<i64>,
    #[serde(default)]
    pub domain: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct BindRequest {
    pub cert_id: i64,
    pub proxy_id: i64,
}

#[derive(Debug, Deserialize)]
pub struct UnbindQuery {
    /// Also delete the files the last apply wrote. Off by default: leaving them
    /// keeps a live listener serving until the operator changes the proxy.
    #[serde(default)]
    pub remove_files: bool,
}

/// Outcome of an apply run.
#[derive(Debug, Serialize)]
pub struct ApplyReport {
    pub applied: Vec<i64>,
    pub failed: Vec<ApplyFailure>,
}

#[derive(Debug, Serialize)]
pub struct ApplyFailure {
    pub binding_id: i64,
    pub error: String,
}

pub async fn list(
    State(state): State<ApiState>,
    Extension(identity): Extension<AuthIdentity>,
    Query(query): Query<ListQuery>,
) -> Result<Json<ApiResponse<Vec<rustfrp_client::cert::Certificate>>>, ApiErrorResponse> {
    let certificates = state
        .db
        .list_certificates(&identity.tenant)
        .await
        .map_err(api_error)?;

    let domain = query.domain.map(|domain| domain.to_ascii_lowercase());
    let filtered: Vec<_> = certificates
        .into_iter()
        .filter(|certificate| {
            if let Some(domain) = &domain {
                if !certificate.primary_domain.eq_ignore_ascii_case(domain)
                    && !certificate
                        .sans
                        .iter()
                        .any(|san| san.eq_ignore_ascii_case(domain))
                {
                    return false;
                }
            }
            match query.expiring_within {
                // A certificate with no material yet has no expiry; it cannot
                // be "expiring within N days".
                Some(days) => certificate.days_remaining.is_some_and(|left| left <= days),
                None => true,
            }
        })
        .collect();

    let count = filtered.len();
    Ok(Json(ApiResponse::ok_list(filtered, count)))
}

pub async fn get(
    State(state): State<ApiState>,
    Extension(identity): Extension<AuthIdentity>,
    Path(id): Path<i64>,
) -> Result<Json<ApiResponse<rustfrp_client::cert::Certificate>>, ApiErrorResponse> {
    let certificate = state
        .db
        .certificate_in_tenant(id, &identity.tenant)
        .await
        .map_err(api_error)?;
    Ok(Json(ApiResponse::ok(certificate)))
}

pub async fn import_pem(
    State(state): State<ApiState>,
    Extension(identity): Extension<AuthIdentity>,
    Json(request): Json<PemImportRequest>,
) -> Result<
    (
        StatusCode,
        Json<ApiResponse<rustfrp_client::cert::Certificate>>,
    ),
    ApiErrorResponse,
> {
    import_bundle(
        &state,
        &identity.tenant,
        request.cert,
        request.key,
        SOURCE_MANUAL,
        request.note,
    )
    .await
}

pub async fn import_upload(
    State(state): State<ApiState>,
    Extension(identity): Extension<AuthIdentity>,
    Json(request): Json<UploadImportRequest>,
) -> Result<
    (
        StatusCode,
        Json<ApiResponse<rustfrp_client::cert::Certificate>>,
    ),
    ApiErrorResponse,
> {
    import_bundle(
        &state,
        &identity.tenant,
        request.cert,
        request.key,
        SOURCE_UPLOAD,
        request.filename,
    )
    .await
}

async fn import_bundle(
    state: &ApiState,
    tenant: &str,
    cert: String,
    key: String,
    source: &str,
    detail: Option<String>,
) -> Result<
    (
        StatusCode,
        Json<ApiResponse<rustfrp_client::cert::Certificate>>,
    ),
    ApiErrorResponse,
> {
    let parsed = parse_bundle(&cert, &key).map_err(api_error)?;
    let certificate = state
        .db
        .import_certificate(
            tenant,
            ImportRequest {
                parsed,
                source: source.to_string(),
                source_detail: detail.filter(|detail| !detail.trim().is_empty()),
            },
        )
        .await
        .map_err(api_error)?;
    Ok((StatusCode::CREATED, Json(ApiResponse::ok(certificate))))
}

/// Import a bundle already present on this host, without copying it through
/// HTTP. Useful when an existing acme.sh / certbot deployment writes the files.
pub async fn import_from_path(
    State(state): State<ApiState>,
    Extension(identity): Extension<AuthIdentity>,
    Json(request): Json<PathImportRequest>,
) -> Result<
    (
        StatusCode,
        Json<ApiResponse<rustfrp_client::cert::Certificate>>,
    ),
    ApiErrorResponse,
> {
    let cert = tokio::fs::read_to_string(&request.crt_path)
        .await
        .map_err(|error| {
            api_error(rustfrp_client::ClientError::CertificateTarget(format!(
                "cannot read {}: {error}",
                request.crt_path
            )))
        })?;
    let key = tokio::fs::read_to_string(&request.key_path)
        .await
        .map_err(|error| {
            api_error(rustfrp_client::ClientError::CertificateTarget(format!(
                "cannot read {}: {error}",
                request.key_path
            )))
        })?;

    import_bundle(
        &state,
        &identity.tenant,
        cert,
        key,
        SOURCE_LOCAL_PATH,
        Some(format!("{} + {}", request.crt_path, request.key_path)),
    )
    .await
}

pub async fn apply(
    State(state): State<ApiState>,
    Extension(identity): Extension<AuthIdentity>,
    Path(id): Path<i64>,
) -> Result<Json<ApiResponse<ApplyReport>>, ApiErrorResponse> {
    let (applied, failed) = state
        .db
        .apply_certificate(id, &identity.tenant)
        .await
        .map_err(api_error)?;

    // The files are on disk; now make frpc read them. This has to be a restart,
    // not a reload: `frpc reload` re-reads the proxy list but keeps the TLS
    // certificate it loaded at startup, so a reload leaves the old certificate
    // serving traffic — the failure looks like "apply did nothing".
    //
    // A restart drops every connection of that profile for a moment, which is
    // the price of a new certificate; it is reported either way and never
    // retroactively fails the apply, because the material is already in place.
    let mut reload_errors = Vec::new();
    if !applied.is_empty() {
        let profile_ids = state
            .db
            .profiles_for_certificate(id, &identity.tenant)
            .await
            .map_err(api_error)?;
        for profile_id in profile_ids {
            let Some(profile) = state.db.get_profile(profile_id).await.ok() else {
                continue;
            };
            if let Err(error) = state
                .process_manager
                .start(profile_id, &profile.name)
                .await
            {
                reload_errors.push(format!("profile {profile_id}: {error}"));
            }
        }
    }
    for error in &reload_errors {
        tracing::warn!(%error, "frpc restart after certificate apply failed");
    }

    Ok(Json(ApiResponse::ok(ApplyReport {
        applied,
        failed: failed
            .into_iter()
            .map(|(binding_id, error)| ApplyFailure { binding_id, error })
            .collect(),
    })))
}


pub async fn delete(
    State(state): State<ApiState>,
    Extension(identity): Extension<AuthIdentity>,
    Path(id): Path<i64>,
) -> Result<StatusCode, ApiErrorResponse> {
    state
        .db
        .delete_certificate(id, &identity.tenant)
        .await
        .map_err(api_error)?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn bind(
    State(state): State<ApiState>,
    Extension(identity): Extension<AuthIdentity>,
    Json(request): Json<BindRequest>,
) -> Result<
    (
        StatusCode,
        Json<ApiResponse<rustfrp_client::cert::CertificateBinding>>,
    ),
    ApiErrorResponse,
> {
    let binding = state
        .db
        .bind_certificate(request.cert_id, request.proxy_id, &identity.tenant)
        .await
        .map_err(api_error)?;
    Ok((StatusCode::CREATED, Json(ApiResponse::ok(binding))))
}

pub async fn unbind(
    State(state): State<ApiState>,
    Extension(identity): Extension<AuthIdentity>,
    Path(id): Path<i64>,
    Query(query): Query<UnbindQuery>,
) -> Result<StatusCode, ApiErrorResponse> {
    state
        .db
        .unbind_certificate(id, &identity.tenant, query.remove_files)
        .await
        .map_err(api_error)?;
    Ok(StatusCode::NO_CONTENT)
}
