//! Structured classification of common frpc failures.

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FailureReason {
    AuthenticationFailed,
    NetworkUnreachable,
    ConfigurationInvalid,
    AddressInUse,
    TlsError,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProcessFailure {
    pub reason: FailureReason,
    pub summary: String,
    pub exit_code: i32,
    /// Number of adjacent crashes grouped into this notification.
    pub occurrences: u32,
}

pub fn classify_failure(stderr: &str, exit_code: i32) -> ProcessFailure {
    let lower = stderr.to_ascii_lowercase();
    let (reason, summary) = if contains_any(
        &lower,
        &[
            "authentication failed",
            "token in login doesn't match",
            "authorization failed",
        ],
    ) {
        (
            FailureReason::AuthenticationFailed,
            "FRP authentication failed",
        )
    } else if contains_any(
        &lower,
        &[
            "network is unreachable",
            "no route to host",
            "connection refused",
            "i/o timeout",
            "connection timed out",
            "temporary failure in name resolution",
            "no such host",
        ],
    ) {
        (
            FailureReason::NetworkUnreachable,
            "FRP server is unreachable",
        )
    } else if contains_any(
        &lower,
        &[
            "address already in use",
            "bind: only one usage",
            "cannot assign requested address",
        ],
    ) {
        (
            FailureReason::AddressInUse,
            "A configured address or port is unavailable",
        )
    } else if contains_any(
        &lower,
        &[
            "tls:",
            "x509:",
            "certificate signed by unknown authority",
            "certificate has expired",
        ],
    ) {
        (FailureReason::TlsError, "FRP TLS validation failed")
    } else if contains_any(
        &lower,
        &[
            "parse config error",
            "unmarshal",
            "invalid configuration",
            "field not found",
            "json: cannot unmarshal",
        ],
    ) {
        (
            FailureReason::ConfigurationInvalid,
            "FRP configuration is invalid",
        )
    } else {
        (FailureReason::Unknown, "frpc exited unexpectedly")
    };

    ProcessFailure {
        reason,
        summary: summary.into(),
        exit_code,
        occurrences: 1,
    }
}

fn contains_any(haystack: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| haystack.contains(needle))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_common_frpc_failures() {
        let cases = [
            (
                "token in login doesn't match token from configuration",
                FailureReason::AuthenticationFailed,
            ),
            (
                "dial tcp 10.0.0.1:7000: connect: network is unreachable",
                FailureReason::NetworkUnreachable,
            ),
            (
                "parse config error: field not found",
                FailureReason::ConfigurationInvalid,
            ),
            (
                "listen tcp :7000: bind: address already in use",
                FailureReason::AddressInUse,
            ),
            (
                "tls: failed to verify certificate: x509: certificate has expired",
                FailureReason::TlsError,
            ),
        ];
        for (message, expected) in cases {
            assert_eq!(classify_failure(message, 1).reason, expected);
        }
        assert_eq!(
            classify_failure("unexpected EOF", 2).reason,
            FailureReason::Unknown
        );
    }
}
