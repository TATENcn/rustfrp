//! Certificate channels: everything that can bring a bundle in.
//!
//! A channel is either a **push** (a token a certificate manager presents) or a
//! **pull** (an outbound fetch from Certd or a plain HTTPS endpoint). They share
//! one table because they answer the same question — "where does the next bundle
//! for this certificate come from?" — and because an operator should be able to
//! switch a certificate between them without deleting and re-creating rows.
//!
//! A channel may be *unbound* (`certificate_id = NULL`): the legacy shape where
//! a token or source existed on its own. Binding it to a certificate is what
//! makes the pair meaningful, and is what the UI now does by default.
//!
//! Neither kind stores plaintext credentials: tokens keep only a SHA-256 digest,
//! and a pull secret is encrypted with the same master key as certificate
//! material.

use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::db::Database;
use crate::error::{ClientError, Result};

/// How a channel brings material in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceKind {
    /// Inbound: a certificate manager POSTs a bundle with a bearer token.
    Push,
    /// Outbound: Certd's open API, signed with keyId/keySecret.
    Certd,
    /// Outbound: any HTTPS endpoint returning a PEM bundle or `{crt, key}` JSON.
    Url,
}

impl SourceKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Push => "push",
            Self::Certd => "certd",
            Self::Url => "url",
        }
    }

    pub fn parse(value: &str) -> Result<Self> {
        match value {
            "push" => Ok(Self::Push),
            "certd" => Ok(Self::Certd),
            "url" => Ok(Self::Url),
            other => Err(ClientError::ConfigValidation(format!(
                "unknown certificate source kind '{other}'"
            ))),
        }
    }

    /// True when the daemon fetches on its own schedule.
    pub fn is_pull(&self) -> bool {
        matches!(self, Self::Certd | Self::Url)
    }
}

/// A channel, as the UI sees it. Never carries a secret.
#[derive(Debug, Clone, Serialize)]
pub struct CertificateSource {
    pub id: i64,
    pub tenant_id: String,
    /// `None` for a channel not yet attached to a certificate.
    pub certificate_id: Option<i64>,
    pub kind: SourceKind,
    pub name: String,
    /// The domain this channel is expected to serve. For a push token this is
    /// empty; the allowlist in `domains` governs it instead.
    pub domain: String,
    pub base_url: String,
    /// Allowed domains for a push token. Empty for pull channels.
    pub domains: Vec<String>,
    /// Whether any credential is stored, without revealing it.
    pub has_secret: bool,
    /// The one channel a certificate prefers when several exist.
    pub is_primary: bool,
    pub enabled: bool,
    pub expires_at: Option<String>,
    pub revoked_at: Option<String>,
    pub last_used_at: Option<String>,
    pub use_count: i64,
    pub last_run_at: Option<String>,
    pub last_status: Option<String>,
    pub last_error: Option<String>,
    pub next_run_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// Result of creating a channel: the row plus the one-time token for a push.
#[derive(Debug, Serialize)]
pub struct CreatedSource {
    pub source: CertificateSource,
    /// Shown once, for `push` only. Null on every subsequent read.
    pub plaintext: Option<String>,
}

/// Fields accepted when creating a channel.
#[derive(Debug, Deserialize)]
pub struct SourceInput {
    #[serde(default)]
    pub certificate_id: Option<i64>,
    pub kind: SourceKind,
    pub name: String,
    #[serde(default)]
    pub domain: String,
    #[serde(default)]
    pub base_url: String,
    /// Push only: the domains a presented certificate may cover.
    #[serde(default)]
    pub domains: Vec<String>,
    /// Certd only.
    #[serde(default)]
    pub key_id: Option<String>,
    #[serde(default)]
    pub key_secret: Option<String>,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_true() -> bool {
    true
}

/// Stored secret payload. No `Serialize`, so it cannot reach a response.
#[derive(Debug, Deserialize, Default)]
pub struct SourceSecret {
    #[serde(default)]
    pub key_id: Option<String>,
    #[serde(default)]
    pub key_secret: Option<String>,
}

/// SHA-256 of a token, lowercase hex.
pub fn token_digest(token: &str) -> String {
    Sha256::digest(token.as_bytes())
        .iter()
        .fold(String::new(), |mut out, byte| {
            use std::fmt::Write;
            let _ = write!(out, "{byte:02x}");
            out
        })
}

/// Columns in [`row_to_source`] order.
const SOURCE_COLUMNS: &str = "id, tenant_id, certificate_id, kind, name, domain, base_url,
     domains_json, enc_secret, token_sha256, is_primary, enabled, expires_at, revoked_at,
     last_used_at, use_count, last_run_at, last_status, last_error, next_run_at,
     created_at, updated_at";

/// Digest lookup row: id, tenant, allowed domains, certificate, name, expiry,
/// revocation.
type TokenRow = (i64, String, String, Option<i64>, String, Option<String>, Option<String>);

/// What a presented token resolved to.
pub struct TokenGrant {
    pub tenant: String,
    pub allowed_domains: Vec<String>,
    /// Set when the token is attached to exactly one certificate, so the push
    /// path can refuse a bundle meant for a different certificate.
    pub certificate_id: Option<i64>,
    pub source_id: i64,
}

impl Database {
    /// Create a channel. A push channel returns its plaintext token exactly once.
    pub async fn create_source(
        &self,
        tenant: &str,
        input: SourceInput,
    ) -> Result<CreatedSource> {
        validate_source_input(&input)?;

        let cipher = self.cert_cipher()?;
        let secret = SourceSecret {
            key_id: input.key_id.clone(),
            key_secret: input.key_secret.clone(),
        };
        let enc_secret = if secret.key_id.is_none() && secret.key_secret.is_none() {
            None
        } else {
            // Built as an explicit map so `SourceSecret` never needs `Serialize`:
            // the only shape that can reach a response carries `has_secret`.
            let mut payload = serde_json::Map::new();
            if let Some(key_id) = &secret.key_id {
                payload.insert("key_id".into(), key_id.clone().into());
            }
            if let Some(key_secret) = &secret.key_secret {
                payload.insert("key_secret".into(), key_secret.clone().into());
            }
            let json =
                serde_json::to_vec(&payload).map_err(|e| ClientError::ConfigValidation(e.to_string()))?;
            Some(cipher.encrypt(&json)?)
        };

        // A push channel's allowlist is required and normalised; a pull channel
        // has no allowlist at all, so it stores an empty list rather than
        // tripping the push validation.
        let domains_json = if input.kind == SourceKind::Push {
            serde_json::to_string(&normalize_domains(input.domains.clone())?)
                .map_err(|e| ClientError::ConfigValidation(e.to_string()))?
        } else {
            "[]".to_string()
        };

        // Only a push channel needs a token; generating one for a pull would
        // leave an unused credential in the database.
        let plaintext = if input.kind == SourceKind::Push {
            Some(generate_token())
        } else {
            None
        };
        let token_sha256 = plaintext.as_ref().map(|token| token_digest(token));

        let domain = input.domain.trim().to_ascii_lowercase();
        let now = chrono::Utc::now().to_rfc3339();

        // A pull channel always belongs to a certificate: it names the domain it
        // will keep up to date, and that domain *is* the certificate's identity.
        // Creating the certificate here is what makes it visible in the list
        // while its first fetch is still pending — otherwise a channel whose
        // first pull failed leaves the operator staring at an empty page with no
        // hint that anything was configured.
        let certificate_id = match (input.certificate_id, input.kind.is_pull()) {
            (Some(id), _) => Some(id),
            (None, true) if !domain.is_empty() => Some(
                self.ensure_certificate(tenant, &domain)
                    .await?
                    .id,
            ),
            (None, _) => None,
        };

        // A pull channel's target domain decides which certificates it accepts
        // and, once attached, which certificate it may replace. Checking it here
        // turns "the first fetch fails with a target error" into "this channel
        // cannot be created", which names the mistake at the point of making it.
        if input.kind.is_pull() {
            if let Some(certificate_id) = certificate_id {
                let certificate = self.certificate_in_tenant(certificate_id, tenant).await?;
                if certificate.material_id.is_some() {
                    let served = crate::cert::domain::served_domains(
                        &certificate.sans,
                        &certificate.primary_domain,
                    );
                    if !served
                        .iter()
                        .any(|san| crate::cert::domain::covers(san, &domain))
                    {
                        return Err(ClientError::ConfigValidation(format!(
                            "the certificate serves {}, which does not cover the channel's domain {}",
                            served.join(", "),
                            domain
                        )));
                    }
                }
            }
        }

        let id = {
            let conn = self.lock().await;
            if let Some(certificate_id) = certificate_id {
                let exists: i64 = conn.query_row(
                    "SELECT COUNT(*) FROM certificate WHERE id = ?1 AND tenant_id = ?2",
                    params![certificate_id, tenant],
                    |row| row.get(0),
                )?;
                if exists == 0 {
                    return Err(ClientError::RecordNotFound {
                        table: "certificate".into(),
                        id: certificate_id,
                    });
                }
            }
            conn.execute(
                "INSERT INTO certificate_source
                    (tenant_id, certificate_id, kind, name, domain, base_url, enc_secret,
                     domains_json, token_sha256, is_primary, enabled, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 0, ?10, ?11, ?11)",
                params![
                    tenant,
                    certificate_id,
                    input.kind.as_str(),
                    input.name.trim(),
                    domain,
                    input.base_url.trim(),
                    enc_secret,
                    domains_json,
                    token_sha256,
                    input.enabled as i32,
                    now,
                ],
            )?;
            conn.last_insert_rowid()
        };

        // A certificate's first channel becomes its primary: otherwise the UI
        // would render a certificate whose only channel is not the one used.
        if certificate_id.is_some() {
            self.promote_if_primary_missing(id, tenant).await?;
        }

        Ok(CreatedSource {
            source: self.source(id, tenant).await?,
            plaintext,
        })
    }

    /// All channels for a tenant, optionally filtered to one certificate.
    pub async fn list_sources(
        &self,
        tenant: &str,
        certificate_id: Option<i64>,
    ) -> Result<Vec<CertificateSource>> {
        let conn = self.lock().await;
        let mut stmt = conn.prepare(&format!(
            "SELECT {SOURCE_COLUMNS} FROM certificate_source
              WHERE tenant_id = ?1 AND (?2 IS NULL OR certificate_id = ?2)
              ORDER BY is_primary DESC, id"
        ))?;
        let sources = stmt
            .query_map(params![tenant, certificate_id], row_to_source)?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(sources)
    }

    pub async fn source(&self, id: i64, tenant: &str) -> Result<CertificateSource> {
        let conn = self.lock().await;
        conn.query_row(
            &format!(
                "SELECT {SOURCE_COLUMNS} FROM certificate_source
                  WHERE id = ?1 AND tenant_id = ?2"
            ),
            params![id, tenant],
            row_to_source,
        )
        .optional()?
        .ok_or(ClientError::RecordNotFound {
            table: "certificate_source".into(),
            id,
        })
    }

    /// Every enabled pull channel that is due, oldest due first.
    ///
    /// Unbound channels are included: a pull source that predates the
    /// certificate-first shape still runs, it just has no certificate to attach
    /// its result to until one is created for the same domain.
    pub async fn due_pull_sources(&self, limit: usize) -> Result<Vec<(i64, String)>> {
        let conn = self.lock().await;
        let mut stmt = conn.prepare(
            "SELECT id, tenant_id FROM certificate_source
              WHERE enabled = 1 AND kind IN ('certd', 'url')
                AND (next_run_at IS NULL OR next_run_at <= ?1)
              ORDER BY COALESCE(next_run_at, ''), id
              LIMIT ?2",
        )?;
        let rows = stmt
            .query_map(params![chrono::Utc::now().to_rfc3339(), limit as i64], |row| {
                Ok((row.get(0)?, row.get(1)?))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Decrypt a channel's credentials for an outbound request.
    pub async fn source_secret(&self, id: i64, tenant: &str) -> Result<SourceSecret> {
        let cipher = self.cert_cipher()?;
        // The column is nullable: a source without credentials (a plain URL, an
        // anonymous fetch) stores NULL, so the row must be read as
        // `Option<Option<Vec<u8>>>` — the outer `None` is "no such source", the
        // inner one is "no secret".
        let blob: Option<Option<Vec<u8>>> = {
            let conn = self.lock().await;
            conn.query_row(
                "SELECT enc_secret FROM certificate_source WHERE id = ?1 AND tenant_id = ?2",
                params![id, tenant],
                |row| row.get(0),
            )
            .optional()?
        };
        match blob {
            Some(Some(blob)) => {
                let json = cipher.decrypt(&blob)?;
                serde_json::from_slice(&json).map_err(|e| {
                    ClientError::CertificateSecrets(format!("source secret is unreadable: {e}"))
                })
            }
            Some(None) => Ok(SourceSecret::default()),
            None => Err(ClientError::RecordNotFound {
                table: "certificate_source".into(),
                id,
            }),
        }
    }

    /// Delete a channel. Pushes using its token start failing immediately.
    pub async fn delete_source(&self, id: i64, tenant: &str) -> Result<()> {
        let conn = self.lock().await;
        let deleted = conn.execute(
            "DELETE FROM certificate_source WHERE id = ?1 AND tenant_id = ?2",
            params![id, tenant],
        )?;
        if deleted == 0 {
            return Err(ClientError::RecordNotFound {
                table: "certificate_source".into(),
                id,
            });
        }
        Ok(())
    }

    /// Attach a channel to a certificate, or detach it with `None`.
    ///
    /// This is what makes a channel switchable: the same token can move from a
    /// certificate to a different one, or become unbound, without being
    /// re-issued.
    pub async fn set_source_certificate(
        &self,
        id: i64,
        certificate_id: Option<i64>,
        tenant: &str,
    ) -> Result<CertificateSource> {
        {
            let conn = self.lock().await;
            if let Some(certificate_id) = certificate_id {
                let exists: i64 = conn.query_row(
                    "SELECT COUNT(*) FROM certificate WHERE id = ?1 AND tenant_id = ?2",
                    params![certificate_id, tenant],
                    |row| row.get(0),
                )?;
                if exists == 0 {
                    return Err(ClientError::RecordNotFound {
                        table: "certificate".into(),
                        id: certificate_id,
                    });
                }
            }
            let updated = conn.execute(
                "UPDATE certificate_source
                    SET certificate_id = ?1, updated_at = ?2
                  WHERE id = ?3 AND tenant_id = ?4",
                params![certificate_id, chrono::Utc::now().to_rfc3339(), id, tenant],
            )?;
            if updated == 0 {
                return Err(ClientError::RecordNotFound {
                    table: "certificate_source".into(),
                    id,
                });
            }
        }
        if certificate_id.is_some() {
            self.promote_if_primary_missing(id, tenant).await?;
        }
        self.source(id, tenant).await
    }

    /// Make `id` the channel a certificate prefers, demoting its siblings.
    ///
    /// The partial unique index allows one primary per certificate, so the
    /// demotion has to happen first or the update would violate it.
    pub async fn set_primary_source(&self, id: i64, tenant: &str) -> Result<CertificateSource> {
        let source = self.source(id, tenant).await?;
        let Some(certificate_id) = source.certificate_id else {
            return Err(ClientError::ConfigValidation(
                "a channel must be attached to a certificate before it can be primary".into(),
            ));
        };

        let conn = self.lock().await;
        let now = chrono::Utc::now().to_rfc3339();
        conn.execute(
            "UPDATE certificate_source SET is_primary = 0, updated_at = ?1
              WHERE tenant_id = ?2 AND certificate_id = ?3",
            params![now, tenant, certificate_id],
        )?;
        conn.execute(
            "UPDATE certificate_source SET is_primary = 1, updated_at = ?1
              WHERE id = ?2 AND tenant_id = ?3",
            params![now, id, tenant],
        )?;
        drop(conn);
        self.source(id, tenant).await
    }

    /// Give a channel primary status when its certificate has none.
    async fn promote_if_primary_missing(&self, id: i64, tenant: &str) -> Result<()> {
        let conn = self.lock().await;
        conn.execute(
            "UPDATE certificate_source SET is_primary = 1
              WHERE id = ?1 AND tenant_id = ?2
                AND certificate_id IS NOT NULL
                AND NOT EXISTS (
                    SELECT 1 FROM certificate_source other
                     WHERE other.certificate_id = certificate_source.certificate_id
                       AND other.is_primary = 1)",
            params![id, tenant],
        )?;
        Ok(())
    }

    /// Resolve a presented push token.
    ///
    /// Returns the tenant, the domains the token may serve, and the certificate
    /// it is attached to (if any). Revoked and expired tokens resolve to `None`,
    /// as does one with an empty allowlist: an unrestricted token would let a
    /// leak overwrite any bound domain.
    pub async fn authenticate_source_token(&self, presented: &str) -> Result<Option<TokenGrant>> {
        if presented.len() < 24 {
            return Ok(None);
        }
        let digest = token_digest(presented);
        let row: Option<TokenRow> = {
            let conn = self.lock().await;
            conn.query_row(
                "SELECT id, tenant_id, domains_json, certificate_id, name, expires_at, revoked_at
                   FROM certificate_source
                  WHERE token_sha256 = ?1 AND kind = 'push'",
                params![digest],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                        row.get(5)?,
                        row.get(6)?,
                    ))
                },
            )
            .optional()?
        };

        let Some((id, tenant, domains_json, certificate_id, name, expires_at, revoked_at)) = row
        else {
            return Ok(None);
        };
        if revoked_at.is_some() {
            return Ok(None);
        }
        if let Some(expires_at) = expires_at {
            let expired = chrono::DateTime::parse_from_rfc3339(&expires_at)
                .map(|at| at < chrono::Utc::now())
                .unwrap_or(true);
            if expired {
                return Ok(None);
            }
        }

        let allowed_domains: Vec<String> = serde_json::from_str(&domains_json).unwrap_or_default();
        if allowed_domains.is_empty() {
            tracing::warn!(
                source_id = id,
                token_name = %name,
                "Push token has no allowed domains; refusing the push"
            );
            return Ok(None);
        }

        {
            let conn = self.lock().await;
            conn.execute(
                "UPDATE certificate_source
                    SET last_used_at = ?1, use_count = use_count + 1
                  WHERE id = ?2",
                params![chrono::Utc::now().to_rfc3339(), id],
            )?;
        }
        tracing::info!(source_id = id, token_name = %name, "Certificate push token used");

        Ok(Some(TokenGrant {
            tenant,
            allowed_domains,
            certificate_id,
            source_id: id,
        }))
    }

    /// Record the outcome of one run, and schedule the next one.
    ///
    /// A failure keeps the same interval: backing off would let a transient
    /// error hide a certificate that is about to expire.
    pub async fn record_source_run(
        &self,
        id: i64,
        tenant: &str,
        status: &str,
        error: Option<&str>,
        next_run_at: Option<&str>,
    ) -> Result<()> {
        let conn = self.lock().await;
        conn.execute(
            "UPDATE certificate_source
                SET last_run_at = ?1, last_status = ?2, last_error = ?3, next_run_at = ?4
              WHERE id = ?5 AND tenant_id = ?6",
            params![
                chrono::Utc::now().to_rfc3339(),
                status,
                error,
                next_run_at,
                id,
                tenant
            ],
        )?;
        Ok(())
    }
}

fn row_to_source(row: &rusqlite::Row<'_>) -> rusqlite::Result<CertificateSource> {
    let kind: String = row.get(3)?;
    let domains_json: String = row.get(7)?;
    let enc_secret: Option<Vec<u8>> = row.get(8)?;
    Ok(CertificateSource {
        id: row.get(0)?,
        tenant_id: row.get(1)?,
        certificate_id: row.get(2)?,
        // An unknown kind can only come from a hand-edited row; treating it as a
        // push keeps it inert instead of silently turning it into a fetch.
        kind: SourceKind::parse(&kind).unwrap_or(SourceKind::Push),
        name: row.get(4)?,
        domain: row.get(5)?,
        base_url: row.get(6)?,
        domains: serde_json::from_str(&domains_json).unwrap_or_default(),
        has_secret: enc_secret.is_some(),
        is_primary: row.get::<_, i64>(10)? != 0,
        enabled: row.get::<_, i64>(11)? != 0,
        expires_at: row.get(12)?,
        revoked_at: row.get(13)?,
        last_used_at: row.get(14)?,
        use_count: row.get(15)?,
        last_run_at: row.get(16)?,
        last_status: row.get(17)?,
        last_error: row.get(18)?,
        next_run_at: row.get(19)?,
        created_at: row.get(20)?,
        updated_at: row.get(21)?,
    })
}

fn validate_source_input(input: &SourceInput) -> Result<()> {
    if input.name.trim().is_empty() {
        return Err(ClientError::ConfigValidation(
            "certificate source name cannot be empty".into(),
        ));
    }
    match input.kind {
        SourceKind::Push => {
            normalize_domains(input.domains.clone())?;
        }
        SourceKind::Certd | SourceKind::Url => {
            if input.domain.trim().is_empty() {
                return Err(ClientError::ConfigValidation(
                    "a pull source needs the domain it serves".into(),
                ));
            }
            let url = input.base_url.trim();
            if !url.starts_with("http://") && !url.starts_with("https://") {
                return Err(ClientError::ConfigValidation(
                    "pull source base URL must start with http:// or https://".into(),
                ));
            }
            if input.kind == SourceKind::Certd
                && (input.key_id.as_deref().unwrap_or("").is_empty()
                    || input.key_secret.as_deref().unwrap_or("").is_empty())
            {
                return Err(ClientError::ConfigValidation(
                    "a Certd source requires key_id and key_secret".into(),
                ));
            }
        }
    }
    Ok(())
}

fn normalize_domains(domains: Vec<String>) -> Result<Vec<String>> {
    let mut out: Vec<String> = domains
        .into_iter()
        .map(|domain| domain.trim().to_ascii_lowercase())
        .filter(|domain| !domain.is_empty())
        .collect();
    out.sort();
    out.dedup();
    if out.is_empty() {
        return Err(ClientError::ConfigValidation(
            "a push token needs at least one allowed domain".into(),
        ));
    }
    Ok(out)
}

/// 32 bytes of randomness, hex encoded. Long enough that the digest cannot be
/// brute forced, and safe in a header value.
fn generate_token() -> String {
    use chacha20poly1305::aead::rand_core::RngCore;
    use chacha20poly1305::aead::OsRng;
    let mut bytes = [0_u8; 32];
    OsRng.fill_bytes(&mut bytes);
    bytes.iter().fold(String::new(), |mut out, byte| {
        use std::fmt::Write;
        let _ = write!(out, "{byte:02x}");
        out
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn digest_is_stable_and_hex() {
        let digest = token_digest("abc");
        assert_eq!(digest.len(), 64);
        assert!(digest.chars().all(|c| c.is_ascii_hexdigit()));
        assert_eq!(digest, token_digest("abc"));
    }

    #[test]
    fn generated_tokens_differ() {
        assert_ne!(generate_token(), generate_token());
        assert_eq!(generate_token().len(), 64);
    }

    #[test]
    fn empty_domain_list_is_rejected() {
        assert!(normalize_domains(vec!["  ".into()]).is_err());
        assert_eq!(
            normalize_domains(vec!["A.example.com".into(), "a.example.com".into()]).unwrap(),
            vec!["a.example.com".to_string()]
        );
    }

    #[test]
    fn only_pull_kinds_are_fetched() {
        assert!(SourceKind::Certd.is_pull() && SourceKind::Url.is_pull());
        assert!(!SourceKind::Push.is_pull());
    }
}
