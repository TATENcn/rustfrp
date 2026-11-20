//! Certificate management for the frpc side.
//!
//! FRP's `https` / `tcp` vhost does not terminate TLS on the server: the
//! `https2http` / `https2https` proxies and the `tls2raw` plugin hold the domain
//! certificate as a **local file** (`crtPath` / `keyPath`). This module owns the
//! platform side of that: parse an imported bundle, keep the material encrypted
//! at rest, and write it to the paths a `[[proxies]]` entry already declares.
//!
//! Secrets live in [`CertificateSecrets`], which deliberately implements neither
//! `Serialize` nor `Clone`: only the apply engine can read them back, and no API
//! response can accidentally carry a private key.

pub mod apply;
pub mod crypto;
pub mod domain;
pub mod parse;

pub use crypto::CertCipher;
pub use parse::{parse_bundle, ParsedCertificate};

use serde::Serialize;

/// Whether a certificate can be written to frpc and expected to work.
///
/// This is derived, never stored: a stored flag would drift from the material it
/// describes the moment a clock ticks or a channel fails. The point of naming it
/// is that "no material yet", "expired" and "fine" are different answers to the
/// same question, and treating them as one (`days_remaining >= 0`) hides the
/// case that actually hurts: a configured certificate that never arrived.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CertificateStatus {
    /// Material present and inside its validity window.
    Active,
    /// Material present, usable, but expiring within [`EXPIRING_SOON_DAYS`].
    ExpiringSoon,
    /// Material present but past `not_after`. Not written to disk.
    Expired,
    /// Material present but `not_before` is still in the future.
    NotYetValid,
    /// No material at all: the certificate is configured, nothing has arrived.
    Pending,
}

/// How close to expiry counts as "soon". Chosen to match the default renewal
/// window of every common issuer, so the warning appears while a fix still works.
pub const EXPIRING_SOON_DAYS: i64 = 30;

impl CertificateStatus {
    /// True only for states where writing the files is a good idea.
    pub fn usable(&self) -> bool {
        matches!(self, Self::Active | Self::ExpiringSoon)
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::ExpiringSoon => "expiring_soon",
            Self::Expired => "expired",
            Self::NotYetValid => "not_yet_valid",
            Self::Pending => "pending",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "active" => Some(Self::Active),
            "expiring_soon" => Some(Self::ExpiringSoon),
            "expired" => Some(Self::Expired),
            "not_yet_valid" => Some(Self::NotYetValid),
            "pending" => Some(Self::Pending),
            _ => None,
        }
    }
}

/// Certificate metadata — the only certificate shape that may leave the daemon.
///
/// Every field here is non-secret. The private key and the certificate body are
/// absent by construction, so "the API must not expose certificates" is enforced
/// by the type rather than by remembering to skip a field.
///
/// A certificate exists before it has material: it is configurable (channels,
/// bindings) while its first bundle is still pending. That state is visible as
/// `status: Pending`, `material_id: None` and `days_remaining: None`, not as a
/// fake expiry.
#[derive(Debug, Clone, Serialize)]
pub struct Certificate {
    pub id: i64,
    pub tenant_id: String,
    pub primary_domain: String,
    pub sans: Vec<String>,
    pub issuer: String,
    pub serial: String,
    /// Fingerprint of the newest material, empty until the first import.
    pub fingerprint_sha256: String,
    pub key_algo: String,
    pub not_before: String,
    pub not_after: String,
    /// The material row these fields came from, `None` before the first import.
    pub material_id: Option<i64>,
    /// Whole days until `not_after`; negative once expired.
    pub days_remaining: Option<i64>,
    /// Derived state. `usable` is exactly `status.usable()`.
    pub status: CertificateStatus,
    /// Why the certificate cannot be used, when it cannot. Empty otherwise.
    pub status_reason: Option<String>,
    pub usable: bool,
    /// The most recent failure recorded on any of its channels, so a `Pending`
    /// certificate can explain why nothing arrived.
    pub channel_error: Option<String>,
    pub source: String,
    pub source_detail: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub bindings: Vec<CertificateBinding>,
    /// Non-fatal findings recorded at import time (incomplete chain, ...).
    pub warnings: Vec<String>,
}

/// One certificate ↔ proxy link. The file paths are *not* stored here: they live
/// in the proxy's FRP-native `plugin_config.crtPath` / `keyPath`, so there is a
/// single source of truth for where material is written.
#[derive(Debug, Clone, Serialize)]
pub struct CertificateBinding {
    pub id: i64,
    pub tenant_id: String,
    pub cert_id: i64,
    pub proxy_id: i64,
    pub proxy_name: String,
    /// Fingerprint written by the last successful apply, if any.
    pub applied_fingerprint: Option<String>,
    pub applied_at: Option<String>,
    pub last_error: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// Decrypted certificate material. Never serializable, never `Clone`.
pub struct CertificateSecrets {
    pub cert_der: Vec<u8>,
    pub chain_der: Vec<u8>,
    pub key_label: String,
    pub key_der: Vec<u8>,
}

impl std::fmt::Debug for CertificateSecrets {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CertificateSecrets")
            .field("cert_der", &format_args!("<{} bytes>", self.cert_der.len()))
            .field(
                "chain_der",
                &format_args!("<{} bytes>", self.chain_der.len()),
            )
            .field("key_label", &self.key_label)
            .field("key_der", &format_args!("<{} bytes>", self.key_der.len()))
            .finish()
    }
}

impl CertificateSecrets {
    /// Full-chain PEM: leaf first, then every intermediate, as frpc expects.
    pub fn fullchain_pem(&self) -> String {
        let mut out = parse::encode_pem("CERTIFICATE", &self.cert_der);
        if !self.chain_der.is_empty() {
            for der in parse::split_der_sequence(&self.chain_der) {
                out.push_str(&parse::encode_pem("CERTIFICATE", der));
            }
        }
        out
    }

    pub fn key_pem(&self) -> String {
        parse::encode_pem(&self.key_label, &self.key_der)
    }
}

/// Derive the state from the material and the clock.
///
/// `None` for both bounds means "no material"; an empty string is treated the
/// same way so a row written before the split cannot look `Active`.
pub fn classify(
    not_before: Option<&str>,
    not_after: Option<&str>,
) -> (CertificateStatus, Option<String>) {
    let Some(not_after) = not_after.filter(|value| !value.trim().is_empty()) else {
        return (
            CertificateStatus::Pending,
            Some("no certificate has been received yet".into()),
        );
    };

    let days = days_remaining(not_after);
    if days < 0 {
        return (
            CertificateStatus::Expired,
            Some(format!(
                "expired {} days ago ({})",
                -days,
                &not_after[..not_after.len().min(10)]
            )),
        );
    }

    // A bundle whose validity starts in the future is rejected by every client
    // that checks dates, so it is reported rather than silently written.
    if let Some(not_before) = not_before.filter(|value| !value.trim().is_empty()) {
        if let Ok(start) = chrono::DateTime::parse_from_rfc3339(not_before) {
            if start > chrono::Utc::now() {
                return (
                    CertificateStatus::NotYetValid,
                    Some(format!("validity starts at {}", &not_before[..not_before.len().min(19)])),
                );
            }
        }
    }

    if days <= EXPIRING_SOON_DAYS {
        return (
            CertificateStatus::ExpiringSoon,
            Some(format!("expires in {days} days")),
        );
    }

    (CertificateStatus::Active, None)
}

/// Whole days from `now` until `not_after` (RFC 3339). Negative when expired.
pub fn days_remaining(not_after: &str) -> i64 {
    let Ok(parsed) = chrono::DateTime::parse_from_rfc3339(not_after) else {
        return 0;
    };
    (parsed.with_timezone(&chrono::Utc) - chrono::Utc::now()).num_days()
}

#[cfg(test)]
mod status_tests {
    use super::*;

    fn at(days: i64) -> String {
        (chrono::Utc::now() + chrono::Duration::days(days)).to_rfc3339()
    }

    #[test]
    fn no_material_is_pending_not_expired() {
        let (status, reason) = classify(None, None);
        assert_eq!(status, CertificateStatus::Pending);
        assert!(!status.usable());
        assert!(reason.unwrap().contains("no certificate"));

        // An empty string is the shape a pre-split row has; it must not read as
        // "active until 1970".
        assert_eq!(classify(None, Some("")).0, CertificateStatus::Pending);
    }

    #[test]
    fn expiry_boundaries_are_ordered() {
        assert_eq!(classify(Some(&at(-10)), Some(&at(-1))).0, CertificateStatus::Expired);
        assert_eq!(classify(Some(&at(-10)), Some(&at(1))).0, CertificateStatus::ExpiringSoon);
        assert_eq!(
            classify(Some(&at(-10)), Some(&at(EXPIRING_SOON_DAYS))).0,
            CertificateStatus::ExpiringSoon
        );
        // `days_remaining` truncates, so one day past the boundary is still
        // reported as the boundary value; step two days to test the gap.
        assert_eq!(
            classify(Some(&at(-10)), Some(&at(EXPIRING_SOON_DAYS + 2))).0,
            CertificateStatus::Active
        );
    }

    #[test]
    fn a_future_not_before_is_reported() {
        let (status, reason) = classify(Some(&at(5)), Some(&at(400)));
        assert_eq!(status, CertificateStatus::NotYetValid);
        assert!(!status.usable());
        assert!(reason.unwrap().contains("validity starts"));
    }

    #[test]
    fn only_active_and_expiring_soon_are_usable() {
        for (status, usable) in [
            (CertificateStatus::Active, true),
            (CertificateStatus::ExpiringSoon, true),
            (CertificateStatus::Expired, false),
            (CertificateStatus::NotYetValid, false),
            (CertificateStatus::Pending, false),
        ] {
            assert_eq!(status.usable(), usable, "{status:?}");
        }
    }

    #[test]
    fn status_strings_round_trip() {
        for status in [
            CertificateStatus::Active,
            CertificateStatus::ExpiringSoon,
            CertificateStatus::Expired,
            CertificateStatus::NotYetValid,
            CertificateStatus::Pending,
        ] {
            assert_eq!(CertificateStatus::parse(status.as_str()), Some(status));
        }
    }
}
