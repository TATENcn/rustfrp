//! Certificate storage: identity, material, and the channels that update it.
//!
//! Three shapes are stored apart on purpose:
//!
//! * `certificate` — identity. One row per `(tenant, primary_domain)`, so a
//!   renewal updates the certificate instead of appending a near-duplicate.
//! * `certificate_material` — one row per issued bundle: fingerprint, validity,
//!   SANs, and the encrypted bodies.
//! * `certificate_source` — how a new bundle arrives: a push token, a Certd
//!   pull, a plain URL. A certificate can exist with no material and no
//!   channel, which is what "configure it now, fill it later" needs.
//!
//! A certificate's identity is deliberately *not* its fingerprint: every
//! reissue has a new fingerprint, and treating that as a new certificate is
//! exactly the bug this split exists to prevent.
//!
//! [`Certificate`](crate::cert::Certificate) is the only shape that may be
//! serialized; [`CertificateSecrets`](crate::cert::CertificateSecrets) has no
//! `Serialize` implementation, so material cannot leak into a response by
//! accident.

use rusqlite::{params, OptionalExtension};

use crate::cert::{
    apply::{target_from_plugin_config, write_material},
    Certificate, CertificateBinding, CertificateSecrets, ParsedCertificate,
};
use crate::db::Database;
use crate::error::{ClientError, Result};

/// Encrypted blobs as stored: certificate, chain, key.
type SecretsRow = (Vec<u8>, Option<Vec<u8>>, Vec<u8>);

/// Where a certificate came from. Stored verbatim and shown in the UI.
pub const SOURCE_MANUAL: &str = "manual";
pub const SOURCE_UPLOAD: &str = "upload";
pub const SOURCE_LOCAL_PATH: &str = "local_path";
pub const SOURCE_WEBHOOK: &str = "webhook";
pub const SOURCE_CERTD_PULL: &str = "certd_pull";
pub const SOURCE_URL_PULL: &str = "url_pull";

/// Import request with its provenance attached.
pub struct ImportRequest {
    pub parsed: ParsedCertificate,
    pub source: String,
    pub source_detail: Option<String>,
}

/// One certificate, identity plus its newest material.
///
/// A bare identity (no material yet) still produces one of these: the fields
/// that only material can supply are empty and `days_remaining` is `None`, so
/// the UI can show "waiting for the first fetch" instead of a fake expiry.
struct CertificateRow {
    id: i64,
    tenant_id: String,
    primary_domain: String,
    created_at: String,
    updated_at: String,
    material_id: Option<i64>,
    fingerprint_sha256: Option<String>,
    sans_json: Option<String>,
    issuer: Option<String>,
    serial: Option<String>,
    key_algo: Option<String>,
    not_before: Option<String>,
    not_after: Option<String>,
    source: Option<String>,
    source_detail: Option<String>,
}

impl CertificateRow {
    /// Column order shared by every query that reads a certificate.
    ///
    /// `certificate_material` is joined with `LEFT JOIN`, so a certificate with
    /// no material yet is still returned rather than silently disappearing.
    /// "Newest" is the most recently *stored* material, not the one expiring
    /// last: a reissue can legitimately have an earlier `not_after` (a shorter
    /// validity), and trusting the date would silently keep serving the old
    /// bundle. Explain the choice where it is made, once.
    const SELECT: &'static str = "SELECT c.id, c.tenant_id, c.primary_domain,
                c.created_at, c.updated_at,
                m.id, m.fingerprint_sha256, m.sans_json, m.issuer, m.serial,
                m.key_algo, m.not_before, m.not_after, m.source, m.source_detail
           FROM certificate c
      LEFT JOIN certificate_material m ON m.id = (
                SELECT id FROM certificate_material
                 WHERE certificate_id = c.id
                 ORDER BY id DESC LIMIT 1)";

    fn from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get(0)?,
            tenant_id: row.get(1)?,
            primary_domain: row.get(2)?,
            created_at: row.get(3)?,
            updated_at: row.get(4)?,
            material_id: row.get(5)?,
            fingerprint_sha256: row.get(6)?,
            sans_json: row.get(7)?,
            issuer: row.get(8)?,
            serial: row.get(9)?,
            key_algo: row.get(10)?,
            not_before: row.get(11)?,
            not_after: row.get(12)?,
            source: row.get(13)?,
            source_detail: row.get(14)?,
        })
    }

    fn into_certificate(self) -> Certificate {
        let sans: Vec<String> = self
            .sans_json
            .as_deref()
            .map(|json| serde_json::from_str(json).unwrap_or_default())
            .unwrap_or_default();
        let not_after = self.not_after.clone().unwrap_or_default();
        let days_remaining = if not_after.is_empty() {
            None
        } else {
            Some(crate::cert::days_remaining(&not_after))
        };
        let (status, status_reason) =
            crate::cert::classify(self.not_before.as_deref(), self.not_after.as_deref());
        // `warnings` predates `status`; keep it as the non-blocking findings list
        // so callers that only read warnings keep working unchanged.
        let warnings = status_reason
            .iter()
            .filter(|_| !status.usable())
            .cloned()
            .collect();
        Certificate {
            id: self.id,
            tenant_id: self.tenant_id,
            primary_domain: self.primary_domain,
            sans,
            issuer: self.issuer.unwrap_or_default(),
            serial: self.serial.unwrap_or_default(),
            fingerprint_sha256: self.fingerprint_sha256.unwrap_or_default(),
            key_algo: self.key_algo.unwrap_or_default(),
            not_before: self.not_before.unwrap_or_default(),
            not_after,
            days_remaining,
            material_id: self.material_id,
            status,
            status_reason,
            usable: status.usable(),
            channel_error: None,
            source: self.source.unwrap_or_default(),
            source_detail: self.source_detail,
            created_at: self.created_at,
            updated_at: self.updated_at,
            bindings: Vec::new(),
            warnings,
        }
    }
}

impl Database {
    /// Store a parsed bundle, keyed by its primary domain.
    ///
    /// Re-importing the same bundle is a no-op on the material but still
    /// refreshes the recorded source; a renewal replaces the newest material
    /// rather than adding a certificate.
    pub async fn import_certificate(
        &self,
        tenant: &str,
        request: ImportRequest,
    ) -> Result<Certificate> {
        let cipher = self.cert_cipher()?;
        let ImportRequest {
            parsed,
            source,
            source_detail,
        } = request;

        let enc_cert = cipher.encrypt(&parsed.cert_der)?;
        let enc_chain = if parsed.chain_der.is_empty() {
            None
        } else {
            Some(cipher.encrypt(&parsed.chain_der)?)
        };
        let enc_key = cipher.encrypt(&parsed.key_der)?;
        let sans_json = serde_json::to_string(&parsed.sans)
            .map_err(|e| ClientError::CertificateParse(e.to_string()))?;
        let now = chrono::Utc::now().to_rfc3339();

        {
            let conn = self.lock().await;
            // Identity first: a certificate may already exist because its
            // channel was configured before any material arrived.
            conn.execute(
                "INSERT INTO certificate
                    (tenant_id, primary_domain, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?3)
                 ON CONFLICT(tenant_id, primary_domain) DO UPDATE SET updated_at = excluded.updated_at",
                params![tenant, parsed.primary_domain, now],
            )?;
            let certificate_id: i64 = conn.query_row(
                "SELECT id FROM certificate WHERE tenant_id = ?1 AND primary_domain = ?2",
                params![tenant, parsed.primary_domain],
                |row| row.get(0),
            )?;

            // The same bundle arriving twice must not create a second version.
            let existing: Option<i64> = conn
                .query_row(
                    "SELECT id FROM certificate_material
                      WHERE certificate_id = ?1 AND fingerprint_sha256 = ?2",
                    params![certificate_id, parsed.fingerprint_sha256],
                    |row| row.get(0),
                )
                .optional()?;

            match existing {
                Some(material_id) => {
                    conn.execute(
                        "UPDATE certificate_material
                            SET sans_json = ?1, issuer = ?2, serial = ?3, key_algo = ?4,
                                not_before = ?5, not_after = ?6, source = ?7, source_detail = ?8,
                                enc_cert_der = ?9, enc_chain_der = ?10, enc_key_der = ?11,
                                updated_at = ?12
                          WHERE id = ?13",
                        params![
                            sans_json,
                            parsed.issuer,
                            parsed.serial,
                            parsed.key_algo,
                            parsed.not_before,
                            parsed.not_after,
                            source,
                            source_detail,
                            enc_cert,
                            enc_chain,
                            enc_key,
                            now,
                            material_id,
                        ],
                    )?;
                }
                None => {
                    conn.execute(
                        "INSERT INTO certificate_material
                            (tenant_id, certificate_id, fingerprint_sha256, sans_json, issuer,
                             serial, key_algo, not_before, not_after, source, source_detail,
                             enc_cert_der, enc_chain_der, enc_key_der, created_at, updated_at)
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?15)",
                        params![
                            tenant,
                            certificate_id,
                            parsed.fingerprint_sha256,
                            sans_json,
                            parsed.issuer,
                            parsed.serial,
                            parsed.key_algo,
                            parsed.not_before,
                            parsed.not_after,
                            source,
                            source_detail,
                            enc_cert,
                            enc_chain,
                            enc_key,
                            now,
                        ],
                    )?;
                }
            }

            tracing::info!(
                certificate_id,
                domain = %parsed.primary_domain,
                fingerprint = %parsed.fingerprint_sha256,
                source,
                "Certificate material imported"
            );
        }

        self.certificate_for_domain(tenant, &parsed.primary_domain)
            .await
    }

    /// All certificates for a tenant, expiring soonest first.
    ///
    /// Certificates with no material sort last: they have no expiry to sort by
    /// and are not yet at risk.
    pub async fn list_certificates(&self, tenant: &str) -> Result<Vec<Certificate>> {
        let rows = {
            let conn = self.lock().await;
            let mut stmt = conn.prepare(&format!(
                "{} WHERE c.tenant_id = ?1
                 ORDER BY m.not_after IS NULL, m.not_after, c.primary_domain",
                CertificateRow::SELECT
            ))?;
            let rows = stmt
                .query_map(params![tenant], CertificateRow::from_row)?
                .collect::<std::result::Result<Vec<_>, _>>()?;
            rows
        };

        let bindings = self.list_certificate_bindings(tenant, None).await?;
        let channel_errors = self.latest_source_errors(tenant).await?;
        Ok(rows
            .into_iter()
            .map(|row| {
                let mut certificate = row.into_certificate();
                certificate.bindings = bindings
                    .iter()
                    .filter(|binding| binding.cert_id == certificate.id)
                    .cloned()
                    .collect();
                certificate.channel_error = channel_errors.get(&certificate.id).cloned();
                certificate
            })
            .collect())
    }

    /// Last failure per certificate, from any attached channel.
    ///
    /// A `Pending` certificate is not broken, it is waiting — and the reason it
    /// is still waiting is the failure of a channel. Keeping the two together in
    /// one response is what lets the UI say why instead of showing an empty row.
    async fn latest_source_errors(&self, tenant: &str) -> Result<std::collections::HashMap<i64, String>> {
        let conn = self.lock().await;
        let mut stmt = conn.prepare(
            "SELECT certificate_id, last_error
               FROM certificate_source
              WHERE tenant_id = ?1
                AND certificate_id IS NOT NULL
                AND last_status = 'failed'
                AND last_error IS NOT NULL
              ORDER BY COALESCE(last_run_at, ''), id",
        )?;
        let mut map = std::collections::HashMap::new();
        for row in stmt.query_map(params![tenant], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
        })? {
            let (certificate_id, error) = row?;
            // Last writer wins by ordering, so a newer attempt replaces an older.
            map.insert(certificate_id, error);
        }
        Ok(map)
    }

    /// One certificate, scoped to a tenant.
    pub async fn certificate_in_tenant(&self, id: i64, tenant: &str) -> Result<Certificate> {
        let row = {
            let conn = self.lock().await;
            conn.query_row(
                &format!("{} WHERE c.id = ?1 AND c.tenant_id = ?2", CertificateRow::SELECT),
                params![id, tenant],
                CertificateRow::from_row,
            )
            .optional()?
        }
        .ok_or(ClientError::RecordNotFound {
            table: "certificate".into(),
            id,
        })?;

        let mut certificate = row.into_certificate();
        certificate.bindings = self.list_certificate_bindings(tenant, Some(id)).await?;
        certificate.channel_error = self
            .latest_source_errors(tenant)
            .await?
            .get(&id)
            .cloned();
        Ok(certificate)
    }

    /// One certificate by domain, scoped to a tenant.
    ///
    /// The pull path uses this to find (or create) the identity a channel
    /// belongs to before it has fetched anything.
    pub async fn certificate_for_domain(
        &self,
        tenant: &str,
        domain: &str,
    ) -> Result<Certificate> {
        let row = {
            let conn = self.lock().await;
            conn.query_row(
                &format!(
                    "{} WHERE c.tenant_id = ?1 AND c.primary_domain = ?2",
                    CertificateRow::SELECT
                ),
                params![tenant, domain],
                CertificateRow::from_row,
            )
            .optional()?
        }
        .ok_or(ClientError::RecordNotFound {
            table: "certificate".into(),
            id: 0,
        })?;

        let mut certificate = row.into_certificate();
        certificate.bindings = self
            .list_certificate_bindings(tenant, Some(certificate.id))
            .await?;
        Ok(certificate)
    }

    /// Create a certificate identity with no material, returning its row.
    ///
    /// Idempotent: an existing identity with the same domain is returned as-is.
    pub async fn ensure_certificate(&self, tenant: &str, domain: &str) -> Result<Certificate> {
        let domain = domain.trim().to_ascii_lowercase();
        if domain.is_empty() {
            return Err(ClientError::ConfigValidation(
                "certificate domain cannot be empty".into(),
            ));
        }
        let now = chrono::Utc::now().to_rfc3339();
        {
            let conn = self.lock().await;
            conn.execute(
                "INSERT INTO certificate
                    (tenant_id, primary_domain, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?3)
                 ON CONFLICT(tenant_id, primary_domain) DO NOTHING",
                params![tenant, domain, now],
            )?;
        }
        self.certificate_for_domain(tenant, &domain).await
    }

    /// Decrypt the newest material for `id`. Never exposed through an API
    /// response.
    pub async fn certificate_secrets(&self, id: i64, tenant: &str) -> Result<CertificateSecrets> {
        let cipher = self.cert_cipher()?;
        let (cert, chain, key) = {
            let conn = self.lock().await;
            let row: Option<SecretsRow> = conn
                .query_row(
                    "SELECT m.enc_cert_der, m.enc_chain_der, m.enc_key_der
                       FROM certificate_material m
                       JOIN certificate c ON c.id = m.certificate_id
                      WHERE m.certificate_id = ?1 AND c.tenant_id = ?2
                      ORDER BY m.id DESC
                      LIMIT 1",
                    params![id, tenant],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                )
                .optional()?;
            match row {
                Some(row) => row,
                // Distinguish "no such certificate" from "no material yet":
                // only the first is a 404, the second is a configuration state.
                None => {
                    let exists: i64 = conn.query_row(
                        "SELECT COUNT(*) FROM certificate WHERE id = ?1 AND tenant_id = ?2",
                        params![id, tenant],
                        |row| row.get(0),
                    )?;
                    if exists == 0 {
                        return Err(ClientError::RecordNotFound {
                            table: "certificate".into(),
                            id,
                        });
                    }
                    return Err(ClientError::CertificateTarget(format!(
                        "certificate {id} has no material yet; import one or wait for its channel to fetch it"
                    )));
                }
            }
        };

        Ok(CertificateSecrets {
            cert_der: cipher.decrypt(&cert)?,
            chain_der: match chain {
                Some(blob) => cipher.decrypt(&blob)?,
                None => Vec::new(),
            },
            key_label: "PRIVATE KEY".into(),
            key_der: cipher.decrypt(&key)?,
        })
    }

    /// Delete a certificate, its material, and its channels.
    ///
    /// Refused while bindings still reference it: an apply would otherwise point
    /// at a certificate that no longer exists.
    pub async fn delete_certificate(&self, id: i64, tenant: &str) -> Result<()> {
        let conn = self.lock().await;
        let bindings: i64 = conn.query_row(
            "SELECT COUNT(*) FROM certificate_binding WHERE cert_id = ?1 AND tenant_id = ?2",
            params![id, tenant],
            |row| row.get(0),
        )?;
        if bindings > 0 {
            return Err(ClientError::CertificateStillBound {
                count: bindings as usize,
            });
        }

        // Material and channels cascade; the binding check above is the only
        // thing FK RESTRICT would have stopped us on.
        let deleted = conn.execute(
            "DELETE FROM certificate WHERE id = ?1 AND tenant_id = ?2",
            params![id, tenant],
        )?;
        if deleted == 0 {
            return Err(ClientError::RecordNotFound {
                table: "certificate".into(),
                id,
            });
        }
        Ok(())
    }

    /// Bind a certificate to a proxy. One proxy holds at most one certificate.
    pub async fn bind_certificate(
        &self,
        cert_id: i64,
        proxy_id: i64,
        tenant: &str,
    ) -> Result<CertificateBinding> {
        // Resolve the target before inserting so a mismatched plugin is reported
        // as a certificate error rather than as a database constraint failure.
        let proxy = self.get_proxy_in_tenant(proxy_id, tenant).await?;
        let plugin = proxy.plugin_config.as_ref().ok_or_else(|| {
            ClientError::CertificateTarget(
                "the proxy has no FRP plugin configuration; set type = https2http and the certificate paths first"
                    .into(),
            )
        })?;
        target_from_plugin_config(plugin)?;

        let now = chrono::Utc::now().to_rfc3339();
        let id = {
            let conn = self.lock().await;
            let exists: i64 = conn.query_row(
                "SELECT COUNT(*) FROM certificate WHERE id = ?1 AND tenant_id = ?2",
                params![cert_id, tenant],
                |row| row.get(0),
            )?;
            if exists == 0 {
                return Err(ClientError::RecordNotFound {
                    table: "certificate".into(),
                    id: cert_id,
                });
            }

            conn.execute(
                "INSERT INTO certificate_binding (tenant_id, cert_id, proxy_id, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?4)
                 ON CONFLICT(proxy_id) DO UPDATE SET
                    cert_id = excluded.cert_id,
                    applied_fingerprint = NULL,
                    applied_at = NULL,
                    last_error = NULL,
                    updated_at = excluded.updated_at",
                params![tenant, cert_id, proxy_id, now],
            )?;
            conn.query_row(
                "SELECT id FROM certificate_binding WHERE proxy_id = ?1",
                params![proxy_id],
                |row| row.get(0),
            )?
        };

        self.certificate_binding(id, tenant).await
    }

    /// Bindings for one certificate, or for the whole tenant when `cert_id` is None.
    pub async fn list_certificate_bindings(
        &self,
        tenant: &str,
        cert_id: Option<i64>,
    ) -> Result<Vec<CertificateBinding>> {
        let conn = self.lock().await;
        let sql = "SELECT b.id, b.tenant_id, b.cert_id, b.proxy_id,
                          COALESCE(p.name, ''), b.applied_fingerprint, b.applied_at,
                          b.last_error, b.created_at, b.updated_at
                     FROM certificate_binding b
                LEFT JOIN local_proxy p ON p.id = b.proxy_id
                    WHERE b.tenant_id = ?1 AND (?2 IS NULL OR b.cert_id = ?2)
                 ORDER BY b.id";
        let mut stmt = conn.prepare(sql)?;
        let bindings = stmt
            .query_map(params![tenant, cert_id], |row| {
                Ok(CertificateBinding {
                    id: row.get(0)?,
                    tenant_id: row.get(1)?,
                    cert_id: row.get(2)?,
                    proxy_id: row.get(3)?,
                    proxy_name: row.get(4)?,
                    applied_fingerprint: row.get(5)?,
                    applied_at: row.get(6)?,
                    last_error: row.get(7)?,
                    created_at: row.get(8)?,
                    updated_at: row.get(9)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(bindings)
    }

    pub async fn certificate_binding(&self, id: i64, tenant: &str) -> Result<CertificateBinding> {
        let conn = self.lock().await;
        conn.query_row(
            "SELECT b.id, b.tenant_id, b.cert_id, b.proxy_id,
                    COALESCE(p.name, ''), b.applied_fingerprint, b.applied_at,
                    b.last_error, b.created_at, b.updated_at
               FROM certificate_binding b
          LEFT JOIN local_proxy p ON p.id = b.proxy_id
              WHERE b.id = ?1 AND b.tenant_id = ?2",
            params![id, tenant],
            |row| {
                Ok(CertificateBinding {
                    id: row.get(0)?,
                    tenant_id: row.get(1)?,
                    cert_id: row.get(2)?,
                    proxy_id: row.get(3)?,
                    proxy_name: row.get(4)?,
                    applied_fingerprint: row.get(5)?,
                    applied_at: row.get(6)?,
                    last_error: row.get(7)?,
                    created_at: row.get(8)?,
                    updated_at: row.get(9)?,
                })
            },
        )
        .optional()?
        .ok_or(ClientError::RecordNotFound {
            table: "certificate_binding".into(),
            id,
        })
    }

    /// Remove a binding. `remove_files` deletes what apply wrote, and only when
    /// it still carries the fingerprint we wrote (never a user's file).
    pub async fn unbind_certificate(
        &self,
        id: i64,
        tenant: &str,
        remove_files: bool,
    ) -> Result<()> {
        let binding = self.certificate_binding(id, tenant).await?;
        let proxy = self
            .get_proxy_in_tenant(binding.proxy_id, tenant)
            .await
            .ok();
        let certificate = self
            .certificate_in_tenant(binding.cert_id, tenant)
            .await
            .ok();

        if remove_files {
            if let (Some(proxy), Some(certificate)) = (proxy.as_ref(), certificate.as_ref()) {
                if let Some(plugin) = proxy.plugin_config.as_ref() {
                    if let Ok(target) = target_from_plugin_config(plugin) {
                        if binding.applied_fingerprint.as_deref()
                            == Some(certificate.fingerprint_sha256.as_str())
                        {
                            for path in [&target.crt_path, &target.key_path] {
                                if let Err(error) = std::fs::remove_file(path) {
                                    if error.kind() != std::io::ErrorKind::NotFound {
                                        tracing::warn!(path = %path.display(), %error, "failed to remove certificate file");
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        let conn = self.lock().await;
        let deleted = conn.execute(
            "DELETE FROM certificate_binding WHERE id = ?1 AND tenant_id = ?2",
            params![id, tenant],
        )?;
        if deleted == 0 {
            return Err(ClientError::RecordNotFound {
                table: "certificate_binding".into(),
                id,
            });
        }
        Ok(())
    }

    /// Write this certificate to every bound proxy's declared paths.
    ///
    /// Each binding succeeds or fails on its own: one unwritable target must not
    /// undo a target that is already correct.
    pub async fn apply_certificate(
        &self,
        id: i64,
        tenant: &str,
    ) -> Result<(Vec<i64>, Vec<(i64, String)>)> {
        let certificate = self.certificate_in_tenant(id, tenant).await?;

        // Writing an expired (or not-yet-valid) bundle would replace a working
        // file with one every client rejects. Refuse here, once, so both the API
        // and the automatic renewal path get the same answer.
        if !certificate.usable {
            let reason = certificate
                .status_reason
                .clone()
                .unwrap_or_else(|| format!("certificate is {}", certificate.status.as_str()));
            return Err(ClientError::CertificateTarget(format!(
                "refusing to write {} to disk: {reason}",
                certificate.primary_domain
            )));
        }

        let secrets = self.certificate_secrets(id, tenant).await?;
        let bindings = certificate.bindings.clone();

        let mut applied = Vec::new();
        let mut failed = Vec::new();

        for binding in bindings {
            let result = self
                .apply_one_binding(&binding, &secrets, &certificate.fingerprint_sha256)
                .await;
            match result {
                Ok(()) => applied.push(binding.id),
                Err(error) => {
                    tracing::warn!(
                        binding_id = binding.id,
                        proxy_id = binding.proxy_id,
                        %error,
                        "Certificate apply failed"
                    );
                    self.record_binding_result(binding.id, None, Some(&error.to_string()))
                        .await?;
                    failed.push((binding.id, error.to_string()));
                }
            }
        }

        Ok((applied, failed))
    }

    /// Apply every certificate whose material changed after its files were
    /// written.
    ///
    /// Called right after a channel imports new material: the operator already
    /// bound the certificate, so a renewal should reach the proxies without a
    /// second manual click. Bindings that already carry the current fingerprint
    /// are skipped, which makes this idempotent.
    pub async fn apply_stale_certificates(&self, tenant: &str) -> Result<usize> {
        let stale: Vec<i64> = {
            let conn = self.lock().await;
            let mut stmt = conn.prepare(
                "SELECT DISTINCT c.id
                   FROM certificate c
                   JOIN certificate_material m ON m.certificate_id = c.id
                   JOIN certificate_binding b ON b.cert_id = c.id
                  WHERE c.tenant_id = ?1
                    AND (b.applied_fingerprint IS NULL OR b.applied_fingerprint != m.fingerprint_sha256)
                    AND m.id = (SELECT id FROM certificate_material
                                 WHERE certificate_id = c.id
                                 ORDER BY id DESC LIMIT 1)",
            )?;
            let rows = stmt
                .query_map(params![tenant], |row| row.get(0))?
                .collect::<std::result::Result<Vec<i64>, _>>()?;
            rows
        };

        let mut applied = 0;
        for id in stale {
            match self.apply_certificate(id, tenant).await {
                Ok((ok, _failed)) => applied += ok.len(),
                Err(error) => {
                    tracing::warn!(certificate_id = id, %error, "automatic apply failed")
                }
            }
        }
        Ok(applied)
    }

    async fn apply_one_binding(
        &self,
        binding: &CertificateBinding,
        secrets: &CertificateSecrets,
        fingerprint: &str,
    ) -> Result<()> {
        let proxy = self
            .get_proxy_in_tenant(binding.proxy_id, binding.tenant_id.as_str())
            .await?;
        let plugin = proxy.plugin_config.as_ref().ok_or_else(|| {
            ClientError::CertificateTarget(format!(
                "proxy '{}' has no plugin configuration",
                proxy.name
            ))
        })?;
        let target = target_from_plugin_config(plugin)?;

        let files = write_material(&target, secrets)?;
        files.commit();

        self.record_binding_result(binding.id, Some(fingerprint), None)
            .await
    }

    async fn record_binding_result(
        &self,
        binding_id: i64,
        fingerprint: Option<&str>,
        error: Option<&str>,
    ) -> Result<()> {
        let now = chrono::Utc::now().to_rfc3339();
        let conn = self.lock().await;
        conn.execute(
            "UPDATE certificate_binding
                SET applied_fingerprint = COALESCE(?1, applied_fingerprint),
                    applied_at = CASE WHEN ?1 IS NULL THEN applied_at ELSE ?2 END,
                    last_error = ?3,
                    updated_at = ?2
              WHERE id = ?4",
            params![fingerprint, now, error, binding_id],
        )?;
        Ok(())
    }
}

impl Database {
    /// Profiles whose enabled bindings include a proxy bound to this certificate.
    ///
    /// The apply path needs it to restart exactly the processes that serve the
    /// rewritten files, and the pull path needs the same answer as the API
    /// handler — so it lives with the data rather than in either caller.
    pub async fn profiles_for_certificate(
        &self,
        certificate_id: i64,
        tenant: &str,
    ) -> Result<Vec<i64>> {
        let conn = self.lock().await;
        let mut stmt = conn.prepare(
            "SELECT DISTINCT r.profile_id
               FROM certificate_binding b
               JOIN binding_rule r ON r.proxy_id = b.proxy_id
              WHERE b.cert_id = ?1 AND b.tenant_id = ?2 AND r.enabled = 1
              ORDER BY r.profile_id",
        )?;
        let ids = stmt
            .query_map(params![certificate_id, tenant], |row| row.get(0))?
            .collect::<std::result::Result<Vec<i64>, _>>()?;
        Ok(ids)
    }
}
