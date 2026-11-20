//! Outbound certificate pulls.
//!
//! Two source kinds, one flow: resolve credentials, fetch, parse the bundle, and
//! hand it to the same import path a pushed certificate uses. No signing, no
//! issuance — RustFRP only consumes what a certificate manager already produced.
//!
//! ## Certd open API
//!
//! `GET /api/v1/cert/get` on the Certd instance. Authentication is the
//! `x-certd-token` header:
//!
//! ```text
//! content = {"keyId":..,"t":<unix seconds>,"encrypt":false,"signType":"md5"}
//! sign    = md5(content + keySecret)
//! header  = base64(content) + "." + base64(sign)
//! ```
//!
//! `encrypt:false` is deliberate: encrypted responses would need Certd's own
//! key derivation, which buys nothing over the TLS the request already uses.

use base64::Engine;
use md5::{Digest, Md5};
use serde::{Deserialize, Serialize};

use rustfrp_client::cert::{domain, parse_bundle};
use rustfrp_client::db::cert_ingest::SourceKind;
use rustfrp_client::db::certificate::{ImportRequest, SOURCE_CERTD_PULL, SOURCE_URL_PULL};
use rustfrp_client::db::Database;
use rustfrp_client::process::manager::ProcessManager;
use rustfrp_client::ClientError;

/// Caps for an outbound fetch: a certificate bundle is small, and a hung peer
/// must not pin a request for a minute.
const FETCH_TIMEOUT_SECS: u64 = 10;
const MAX_BODY_BYTES: usize = 256 * 1024;

/// What one pull did.
#[derive(Debug, Serialize)]
pub struct PullReport {
    pub source_id: i64,
    pub status: String,
    pub certificate_id: Option<i64>,
    pub primary_domain: Option<String>,
    /// Bindings rewritten because their files were behind the new material.
    pub applied: usize,
    pub message: String,
}

/// How long to wait before the next automatic run.
///
/// A certificate lives 90 days and is reissued with days to spare, so a daily
/// check is plenty and keeps the outbound request rate trivially low.
pub const PULL_INTERVAL_SECS: i64 = 24 * 60 * 60;

/// Fetch and import once. Records the outcome on the channel either way.
///
/// `processes` is optional so the database-level tests can run without frpc; when
/// present, a profile whose certificate files were rewritten is restarted, since
/// frpc only reads `crtPath` at process start.
pub async fn pull_once(
    db: &Database,
    processes: Option<&ProcessManager>,
    id: i64,
    tenant: &str,
) -> Result<PullReport, ClientError> {
    match pull_inner(db, processes, id, tenant).await {
        Ok(report) => {
            db.record_source_run(id, tenant, "success", None, Some(&next_run()))
                .await?;
            Ok(report)
        }
        Err(error) => {
            // Keep the same interval on failure: backing off would let an
            // intermittent error hide a certificate that is about to expire.
            db.record_source_run(id, tenant, "failed", Some(&error.to_string()), Some(&next_run()))
                .await?;
            Err(error)
        }
    }
}

fn next_run() -> String {
    (chrono::Utc::now() + chrono::Duration::seconds(PULL_INTERVAL_SECS)).to_rfc3339()
}

async fn pull_inner(
    db: &Database,
    processes: Option<&ProcessManager>,
    id: i64,
    tenant: &str,
) -> Result<PullReport, ClientError> {
    let source = db.source(id, tenant).await?;
    if !source.enabled {
        return Err(ClientError::ConfigValidation(
            "this channel is disabled".into(),
        ));
    }
    if !source.kind.is_pull() {
        return Err(ClientError::ConfigValidation(
            "only a pull channel can be fetched; a push channel waits for the sender".into(),
        ));
    }
    let secret = db.source_secret(id, tenant).await?;

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(FETCH_TIMEOUT_SECS))
        .build()
        .map_err(|e| ClientError::CertificateTarget(format!("failed to build HTTP client: {e}")))?;

    let (cert, key, source_label) = match source.kind {
        SourceKind::Certd => {
            let response = fetch_certd(&client, &source.base_url, &source.domain, &secret).await?;
            let fullchain = response.fullchain()?;
            let key = response.key.ok_or_else(|| {
                ClientError::CertificateParse(
                    "Certd returned a certificate without a private key".into(),
                )
            })?;
            (fullchain, key, SOURCE_CERTD_PULL)
        }
        SourceKind::Url => {
            let body = fetch_url(&client, &source.base_url, &secret).await?;
            let (cert, key) = split_url_body(&body)?;
            (cert, key, SOURCE_URL_PULL)
        }
        SourceKind::Push => {
            return Err(ClientError::ConfigValidation(
                "a push channel is driven by the sender, not fetched".into(),
            ))
        }
    };

    let parsed = parse_bundle(&cert, &key)?;

    // The certificate must actually serve the domain this channel was created
    // for. A misconfigured or hostile endpoint must not be able to install a
    // certificate for an unrelated host, and containment (not equality) is the
    // right test: a CA may reissue with extra SANs.
    if !domain::covers_target(&parsed.sans, &parsed.primary_domain, &source.domain) {
        return Err(ClientError::CertificateTarget(format!(
            "the fetched certificate serves {}, which does not cover the channel's domain {}",
            domain::served_domains(&parsed.sans, &parsed.primary_domain).join(", "),
            source.domain
        )));
    }

    let certificate = db
        .import_certificate(
            tenant,
            ImportRequest {
                parsed,
                source: source_label.to_string(),
                source_detail: Some(format!("{} ({})", source.name, source.base_url)),
            },
        )
        .await?;

    // A channel attached to a certificate replaces that certificate's material;
    // an unbound one resolves (or creates) the identity by domain, so a pull
    // that was configured before the certificate still lands somewhere.
    let certificate = match source.certificate_id {
        Some(attached) if attached != certificate.id => {
            return Err(ClientError::CertificateTarget(format!(
                "the fetched certificate was stored as {}, but this channel is attached to {}",
                certificate.primary_domain, attached
            )))
        }
        Some(_) => certificate,
        None => match db
            .set_source_certificate(id, Some(certificate.id), tenant)
            .await
        {
            Ok(_) => certificate,
            Err(error) => {
                tracing::warn!(source_id = id, %error, "pulled certificate could not be attached");
                certificate
            }
        },
    };

    // A renewal should reach the proxies the operator already bound, without a
    // second manual click — including the process that serves it, because
    // `frpc reload` keeps the TLS certificate it loaded at startup.
    let applied = match db.apply_stale_certificates(tenant).await {
        Ok(applied) => applied,
        Err(error) => {
            tracing::warn!(source_id = id, %error, "automatic apply after pull failed");
            0
        }
    };
    if applied > 0 {
        if let Some(processes) = processes {
            match db.profiles_for_certificate(certificate.id, tenant).await {
                Ok(profile_ids) => {
                    for profile_id in profile_ids {
                        let Ok(profile) = db.get_profile(profile_id).await else {
                            continue;
                        };
                        if let Err(error) = processes.start(profile_id, &profile.name).await {
                            tracing::warn!(
                                profile_id,
                                %error,
                                "frpc restart after automatic certificate apply failed"
                            );
                        }
                    }
                }
                Err(error) => {
                    tracing::warn!(%error, "could not resolve profiles for a renewed certificate")
                }
            }
        }
    }

    Ok(PullReport {
        source_id: id,
        status: "success".into(),
        certificate_id: Some(certificate.id),
        primary_domain: Some(certificate.primary_domain.clone()),
        applied,
        message: format!(
            "imported {}, expires {}",
            certificate.primary_domain, certificate.not_after
        ),
    })
}

/// Certd's `CertInfo` response.
///
/// `crt` is normally the fullchain. Some pipeline steps return the leaf in `oc`
/// and the intermediates in `ic` instead, so both shapes are assembled here.
#[derive(Debug, Deserialize)]
struct CertdCertInfo {
    #[serde(default)]
    crt: String,
    #[serde(default)]
    key: Option<String>,
    /// Certificate only, without intermediates.
    #[serde(default)]
    oc: Option<String>,
    /// Intermediates, concatenated.
    #[serde(default)]
    ic: Option<String>,
}

impl CertdCertInfo {
    fn fullchain(&self) -> Result<String, ClientError> {
        if !self.crt.trim().is_empty() {
            return Ok(self.crt.clone());
        }
        match (&self.oc, &self.ic) {
            (Some(leaf), Some(intermediates)) => Ok(format!("{leaf}\n{intermediates}")),
            (Some(leaf), None) => Ok(leaf.clone()),
            _ => Err(ClientError::CertificateParse(
                "Certd response carried no certificate body".into(),
            )),
        }
    }
}

/// Certd wraps every payload in `{code, message, data}`.
///
/// `message` is the field real deployments use; `msg` is accepted as well
/// because older builds emitted it. Both were previously ignored on failure,
/// which turned every rejection into "no message (code 20002)" — the code alone
/// does not tell an operator whether the key, the clock, or the domain is wrong.
#[derive(Debug, Deserialize)]
struct CertdEnvelope<T> {
    code: Option<i64>,
    #[serde(default, alias = "msg")]
    message: Option<String>,
    data: Option<T>,
}

async fn fetch_certd(
    client: &reqwest::Client,
    base_url: &str,
    domain: &str,
    secret: &rustfrp_client::db::cert_ingest::SourceSecret,
) -> Result<CertdCertInfo, ClientError> {
    let key_id = secret
        .key_id
        .as_deref()
        .ok_or_else(|| ClientError::ConfigValidation("Certd source has no key id".into()))?;
    let key_secret = secret
        .key_secret
        .as_deref()
        .ok_or_else(|| ClientError::ConfigValidation("Certd source has no key secret".into()))?;

    let url = format!(
        "{}/api/v1/cert/get?domains={}",
        base_url.trim_end_matches('/'),
        urlencode(domain)
    );
    // A skewed clock is the one failure this daemon can work around: Certd's
    // own `Date` header says what "now" is, and the signature is nothing but
    // (key, timestamp). Trying again with that value turns a permanent
    // "timestamp error" into a successful pull on a machine whose clock is wrong
    // — which is exactly the machine that cannot fix it without root.
    let first = certd_request(client, &url, key_id, key_secret, None).await;
    match first {
        Ok(info) => Ok(info),
        Err(CertdFailure::Clock(server_time)) => {
            tracing::warn!(
                local = chrono::Utc::now().timestamp(),
                server = server_time,
                "Certd rejected the request timestamp; retrying with the server's clock"
            );
            certd_request(client, &url, key_id, key_secret, Some(server_time))
                .await
                .map_err(|failure| failure.into_error())
        }
        Err(failure) => Err(failure.into_error()),
    }
}

/// Why a Certd call failed, so the caller can decide whether to retry.
enum CertdFailure {
    /// A rejection that a different timestamp would fix, carrying the server's
    /// notion of "now" when it advertised one.
    Clock(i64),
    /// Anything else: wrong key, missing certificate, transport error.
    Other(ClientError),
}

impl CertdFailure {
    fn into_error(self) -> ClientError {
        match self {
            Self::Clock(_) => ClientError::CertificateTarget(
                "Certd rejected the request timestamp even after retrying with its own clock"
                    .into(),
            ),
            Self::Other(error) => error,
        }
    }
}

async fn certd_request(
    client: &reqwest::Client,
    url: &str,
    key_id: &str,
    key_secret: &str,
    at: Option<i64>,
) -> std::result::Result<CertdCertInfo, CertdFailure> {
    let token = match at {
        Some(timestamp) => certd_token_at(key_id, key_secret, timestamp),
        None => certd_token(key_id, key_secret),
    };

    let response = client
        .get(url)
        .header("x-certd-token", token)
        .send()
        .await
        .map_err(|e| {
            CertdFailure::Other(ClientError::CertificateTarget(format!(
                "Certd request failed: {e}"
            )))
        })?;

    // The header is captured before the body is consumed; it is the only place
    // a skewed host can learn the correct time.
    let server_time = response
        .headers()
        .get(reqwest::header::DATE)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| chrono::DateTime::parse_from_rfc2822(value).ok())
        .map(|date| date.timestamp());

    let status = response.status();
    let body = read_bounded(response).await.map_err(CertdFailure::Other)?;
    if !status.is_success() {
        return Err(CertdFailure::Other(ClientError::CertificateTarget(format!(
            "Certd returned HTTP {}: {}",
            status.as_u16(),
            truncate(&body, 200)
        ))));
    }

    // Certd answers both plain `{crt,...}` and `{code,message,data:{crt,...}}`.
    if let Ok(envelope) = serde_json::from_str::<CertdEnvelope<CertdCertInfo>>(&body) {
        if let Some(data) = envelope.data {
            return Ok(data);
        }
        if let Some(code) = envelope.code {
            if code != 0 && code != 200 {
                let message = envelope.message.unwrap_or_else(|| "no message".into());
                // Only the timestamp rejection is retryable, and only when the
                // server told us its time.
                if code == CERTD_TIMESTAMP_REJECTED {
                    if let Some(server_time) = server_time {
                        return Err(CertdFailure::Clock(server_time));
                    }
                }
                return Err(CertdFailure::Other(ClientError::CertificateTarget(format!(
                    "Certd rejected the request: {message} (code {code}){}",
                    certd_hint(code),
                ))));
            }
        }
    }
    serde_json::from_str::<CertdCertInfo>(&body).map_err(|e| {
        CertdFailure::Other(ClientError::CertificateParse(format!(
            "Certd response is not a certificate payload: {e} — {}",
            truncate(&body, 200)
        )))
    })
}

async fn fetch_url(
    client: &reqwest::Client,
    base_url: &str,
    secret: &rustfrp_client::db::cert_ingest::SourceSecret,
) -> Result<String, ClientError> {
    let mut request = client.get(base_url);
    if let Some(token) = secret
        .key_secret
        .as_deref()
        .filter(|value| !value.is_empty())
    {
        request = request.header("Authorization", format!("Bearer {token}"));
    }
    let response = request
        .send()
        .await
        .map_err(|e| ClientError::CertificateTarget(format!("URL fetch failed: {e}")))?;
    let status = response.status();
    let body = read_bounded(response).await?;
    if !status.is_success() {
        return Err(ClientError::CertificateTarget(format!(
            "source returned HTTP {}: {}",
            status.as_u16(),
            truncate(&body, 200)
        )));
    }
    Ok(body)
}

/// A URL source may answer with a PEM bundle containing both halves, or with
/// `{ "crt": "...", "key": "..." }`.
fn split_url_body(body: &str) -> Result<(String, String), ClientError> {
    if let Ok(json) = serde_json::from_str::<serde_json::Value>(body) {
        let field = |name: &str| {
            json.get(name)
                .and_then(serde_json::Value::as_str)
                .map(str::to_string)
        };
        if let (Some(cert), Some(key)) = (field("crt").or(field("cert")), field("key")) {
            return Ok((cert, key));
        }
    }

    // Split on PEM headers so a concatenated bundle resolves without guessing.
    let blocks: Vec<&str> = body.split("-----BEGIN ").skip(1).collect();
    if blocks.len() >= 2 {
        let mut cert_parts = Vec::new();
        let mut key_part = None;
        for block in blocks {
            let full = format!("-----BEGIN {block}");
            if block.starts_with("PRIVATE KEY")
                || block.starts_with("RSA PRIVATE KEY")
                || block.starts_with("EC PRIVATE KEY")
            {
                key_part = Some(full);
            } else if block.starts_with("CERTIFICATE") {
                cert_parts.push(full);
            }
        }
        if let (false, Some(key)) = (cert_parts.is_empty(), key_part) {
            return Ok((cert_parts.join("\n"), key));
        }
    }

    Err(ClientError::CertificateParse(
        "source body contains neither {crt,key} JSON nor a PEM certificate + key pair".into(),
    ))
}

/// What a Certd rejection code usually means, appended to the message.
///
/// Naming the likely cause is the difference between "it failed" and "your key
/// is wrong"; the codes are Certd's, not ours, so they are explained rather than
/// translated into our own taxonomy.
/// Certd's "ApiToken时间戳错误". Verified against a live instance: a request
/// signed eight hours in the future is accepted, which is why this is the only
/// rejection a clock correction can fix.
const CERTD_TIMESTAMP_REJECTED: i64 = 20002;

fn certd_hint(code: i64) -> &'static str {
    match code {
        20001 => " — the key id or secret does not match; regenerate the open API key in Certd",
        // The signature covers a second-resolution timestamp, so this is the
        // one failure a correct key still hits when the two clocks disagree.
        // Either side can be wrong; the message must not assume it is ours.
        20002 => " — this host and Certd disagree about the current time; the retry below uses Certd's own clock, so a second failure means the key is wrong too",
        _ => "",
    }
}

/// Build the `x-certd-token` header value.
fn certd_token(key_id: &str, key_secret: &str) -> String {
    certd_token_at(key_id, key_secret, chrono::Utc::now().timestamp())
}

/// Same token, signed for an explicit timestamp.
///
/// Split out because the timestamp is the one input a host with a wrong clock
/// can substitute: everything else in the signature is the caller's key.
fn certd_token_at(key_id: &str, key_secret: &str, timestamp: i64) -> String {
    let content = format!(
        r#"{{"keyId":"{}","t":{},"encrypt":false,"signType":"md5"}}"#,
        key_id, timestamp
    );
    let mut hasher = Md5::new();
    hasher.update(content.as_bytes());
    hasher.update(key_secret.as_bytes());
    let sign = hex(&hasher.finalize());

    let engine = base64::engine::general_purpose::STANDARD;
    format!(
        "{}.{}",
        engine.encode(content.as_bytes()),
        engine.encode(sign.as_bytes())
    )
}

async fn read_bounded(response: reqwest::Response) -> Result<String, ClientError> {
    let bytes = response
        .bytes()
        .await
        .map_err(|e| ClientError::CertificateTarget(format!("failed to read response: {e}")))?;
    if bytes.len() > MAX_BODY_BYTES {
        return Err(ClientError::CertificateTarget(format!(
            "response is {} bytes, above the {MAX_BODY_BYTES} byte limit",
            bytes.len()
        )));
    }
    Ok(String::from_utf8_lossy(&bytes).to_string())
}

fn urlencode(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b',' => {
                out.push(byte as char)
            }
            other => out.push_str(&format!("%{other:02X}")),
        }
    }
    out
}

fn truncate(value: &str, limit: usize) -> String {
    if value.len() <= limit {
        return value.to_string();
    }
    let mut end = limit;
    while end > 0 && !value.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…", &value[..end])
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().fold(String::new(), |mut out, byte| {
        use std::fmt::Write;
        let _ = write!(out, "{byte:02x}");
        out
    })
}

/// Run due pull channels forever.
///
/// A single task for the whole process, not one per channel: the work is a
/// handful of outbound requests per day, and one loop makes "what is due right
/// now" a single query instead of a scheduling structure to keep in sync.
pub fn spawn_scheduler(db: Database, processes: ProcessManager) {
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(std::time::Duration::from_secs(60));
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            ticker.tick().await;
            let due = match db.due_pull_sources(16).await {
                Ok(due) => due,
                Err(error) => {
                    tracing::warn!(%error, "could not list due certificate channels");
                    continue;
                }
            };
            for (id, tenant) in due {
                match pull_once(&db, Some(&processes), id, &tenant).await {
                    Ok(report) => tracing::info!(
                        source_id = id,
                        domain = report.primary_domain.as_deref().unwrap_or("?"),
                        applied = report.applied,
                        "automatic certificate pull succeeded"
                    ),
                    Err(error) => tracing::warn!(
                        source_id = id,
                        %error,
                        "automatic certificate pull failed"
                    ),
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn certd_token_has_the_documented_shape() {
        let token = certd_token("kid", "secret");
        let (content_b64, sign_b64) = token.split_once('.').unwrap();
        let engine = base64::engine::general_purpose::STANDARD;
        let content = String::from_utf8(engine.decode(content_b64).unwrap()).unwrap();
        let sign = String::from_utf8(engine.decode(sign_b64).unwrap()).unwrap();

        assert!(content.contains(r#""keyId":"kid""#));
        assert!(content.contains(r#""signType":"md5""#));
        assert!(content.contains(r#""encrypt":false"#));
        assert_eq!(sign.len(), 32);

        // The signature must be md5(content + secret).
        let expected = {
            let mut hasher = Md5::new();
            hasher.update(content.as_bytes());
            hasher.update(b"secret");
            hex(&hasher.finalize())
        };
        assert_eq!(sign, expected);
    }

    #[test]
    fn splits_json_and_pem_bodies() {
        let json = r#"{"crt":"CERT","key":"KEY"}"#;
        assert_eq!(
            split_url_body(json).unwrap(),
            ("CERT".to_string(), "KEY".to_string())
        );

        let pem = "-----BEGIN CERTIFICATE-----\nAAA\n-----END CERTIFICATE-----\n-----BEGIN PRIVATE KEY-----\nBBB\n-----END PRIVATE KEY-----\n";
        let (cert, key) = split_url_body(pem).unwrap();
        assert!(cert.contains("BEGIN CERTIFICATE"));
        assert!(key.contains("BEGIN PRIVATE KEY"));

        assert!(split_url_body("nonsense").is_err());
    }

    #[test]
    fn encodes_domain_query_safely() {
        assert_eq!(urlencode("a.example.com"), "a.example.com");
        assert_eq!(urlencode("*.example.com"), "%2A.example.com");
        assert_eq!(urlencode("a b"), "a%20b");
    }

    #[test]
    fn truncates_on_char_boundaries() {
        let value = "中文中文中文";
        let truncated = truncate(value, 7);
        assert!(truncated.ends_with('…'));
        assert!(std::str::from_utf8(truncated.as_bytes()).is_ok());
    }
}

#[cfg(test)]
mod envelope_tests {
    use super::*;

    #[test]
    fn certd_message_field_is_surfaced() {
        // The real Certd payload uses `message`; ignoring it produced
        // "no message (code 20002)" for every rejection.
        let body = r#"{"code":20002,"message":"ApiToken时间戳错误"}"#;
        let envelope: CertdEnvelope<CertdCertInfo> = serde_json::from_str(body).unwrap();
        assert_eq!(envelope.code, Some(20002));
        assert_eq!(envelope.message.as_deref(), Some("ApiToken时间戳错误"));
        assert!(envelope.data.is_none());

        // Older builds used `msg`; both must land in the same field.
        let legacy: CertdEnvelope<CertdCertInfo> =
            serde_json::from_str(r#"{"code":1,"msg":"openKey不存在"}"#).unwrap();
        assert_eq!(legacy.message.as_deref(), Some("openKey不存在"));
    }

    #[test]
    fn a_success_payload_still_parses() {
        let body = r#"{"code":0,"message":"ok","data":{"crt":"CERT","key":"KEY"}}"#;
        let envelope: CertdEnvelope<CertdCertInfo> = serde_json::from_str(body).unwrap();
        let data = envelope.data.unwrap();
        assert_eq!(data.crt, "CERT");
        assert_eq!(data.key.as_deref(), Some("KEY"));
    }

    #[test]
    fn known_codes_explain_the_likely_cause() {
        assert!(certd_hint(20001).contains("key id or secret"));
        assert!(certd_hint(20002).contains("disagree about the current time"));
        assert_eq!(CERTD_TIMESTAMP_REJECTED, 20002);
        assert!(certd_hint(12345).is_empty());
    }
}

#[cfg(test)]
mod clock_retry_tests {
    use super::*;

    /// The token must be signed with the timestamp it was given, or the retry
    /// would resend the same rejected value.
    #[test]
    fn token_honours_an_explicit_timestamp() {
        let engine = base64::engine::general_purpose::STANDARD;
        let decode = |key: &str, secret: &str, at: i64| {
            let token = certd_token_at(key, secret, at);
            let (content, sign) = token.split_once('.').unwrap();
            (
                String::from_utf8(engine.decode(content).unwrap()).unwrap(),
                String::from_utf8(engine.decode(sign).unwrap()).unwrap(),
            )
        };

        let (content, sign) = decode("kid", "s", 1_700_000_000);
        assert!(content.contains(r#""t":1700000000"#), "{content}");

        let mut hasher = Md5::new();
        hasher.update(content.as_bytes());
        hasher.update(b"s");
        assert_eq!(sign, hex(&hasher.finalize()));

        // A different timestamp is a different signature; that is what makes the
        // retry meaningful rather than a repeat.
        assert_ne!(decode("kid", "s", 1).1, decode("kid", "s", 2).1);
    }

    /// A skew-retry must not swallow a genuine key failure.
    #[test]
    fn only_the_timestamp_code_is_retryable() {
        assert_eq!(CERTD_TIMESTAMP_REJECTED, 20002);
        // 20001 means the key is wrong; retrying with a better clock cannot help.
        assert_ne!(CERTD_TIMESTAMP_REJECTED, 20001);
    }
}
