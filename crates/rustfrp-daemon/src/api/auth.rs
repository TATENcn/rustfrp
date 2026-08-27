//! Zero-trust API identities backed by SHA256 token digests and explicit scopes.

use axum::http::{Method, Request};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AuthIdentity {
    pub name: String,
    pub tenant: String,
    pub scopes: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TokenPolicy {
    pub name: String,
    pub tenant: String,
    /// Lowercase SHA256 digest of the bearer token. Plaintext tokens are never stored.
    pub token_sha256: String,
    #[serde(default)]
    pub scopes: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthPolicy {
    pub tokens: Vec<TokenPolicy>,
}

impl AuthPolicy {
    pub fn from_file(path: impl AsRef<Path>) -> anyhow::Result<Self> {
        let value: Self = serde_json::from_slice(&std::fs::read(path)?)?;
        anyhow::ensure!(
            !value.tokens.is_empty(),
            "auth policy must contain at least one token"
        );
        for token in &value.tokens {
            anyhow::ensure!(!token.name.trim().is_empty(), "token name cannot be empty");
            anyhow::ensure!(
                !token.tenant.trim().is_empty(),
                "token tenant cannot be empty"
            );
            anyhow::ensure!(
                decode_digest(&token.token_sha256).is_some(),
                "token_sha256 must be 64 lowercase hexadecimal characters"
            );
            anyhow::ensure!(!token.scopes.is_empty(), "token scopes cannot be empty");
        }
        Ok(value)
    }

    pub fn authenticate(
        &self,
        bearer: &str,
        request: &Request<axum::body::Body>,
    ) -> Option<AuthIdentity> {
        if bearer.len() < 32 {
            return None;
        }
        let actual = Sha256::digest(bearer.as_bytes());
        let required = required_scope(request.method(), request.uri().path());
        self.tokens.iter().find_map(|policy| {
            let expected = decode_digest(&policy.token_sha256)?;
            if !constant_time_eq(actual.as_slice(), &expected)
                || !policy
                    .scopes
                    .iter()
                    .any(|scope| scope == "*" || scope == required)
            {
                return None;
            }
            let requested_tenant = request
                .headers()
                .get("x-rustfrp-tenant")
                .and_then(|value| value.to_str().ok());
            if requested_tenant.is_some_and(|tenant| tenant != policy.tenant) {
                return None;
            }
            Some(AuthIdentity {
                name: policy.name.clone(),
                tenant: policy.tenant.clone(),
                scopes: policy.scopes.clone(),
            })
        })
    }
}

pub fn sha256_hex(token: &str) -> String {
    Sha256::digest(token.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn required_scope(method: &Method, path: &str) -> &'static str {
    if path == "/api/v1/metrics/traffic" {
        "telemetry:write"
    } else {
        let tenant_resource = path == "/api/v1/auth/whoami"
            || path.starts_with("/api/v1/profiles")
            || path.starts_with("/api/v1/environments");
        match (
            tenant_resource,
            matches!(*method, Method::GET | Method::HEAD),
        ) {
            (true, true) => "read",
            (true, false) => "write",
            (false, true) => "platform:read",
            (false, false) => "platform:write",
        }
    }
}

fn decode_digest(value: &str) -> Option<[u8; 32]> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return None;
    }
    let mut decoded = [0_u8; 32];
    for (index, output) in decoded.iter_mut().enumerate() {
        *output = u8::from_str_radix(&value[index * 2..index * 2 + 2], 16).ok()?;
    }
    Some(decoded)
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    left.iter()
        .zip(right)
        .fold(0_u8, |diff, (a, b)| diff | (a ^ b))
        == 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;

    const TOKEN: &str = "0123456789abcdef0123456789abcdef";

    fn policy(scopes: &[&str]) -> AuthPolicy {
        AuthPolicy {
            tokens: vec![TokenPolicy {
                name: "automation".into(),
                tenant: "acme".into(),
                token_sha256: sha256_hex(TOKEN),
                scopes: scopes.iter().map(|scope| (*scope).into()).collect(),
            }],
        }
    }

    #[test]
    fn enforces_digest_scope_and_tenant() {
        let read = Request::builder()
            .method(Method::GET)
            .uri("/api/v1/profiles")
            .body(Body::empty())
            .unwrap();
        assert_eq!(
            policy(&["read"]).authenticate(TOKEN, &read).unwrap().tenant,
            "acme"
        );
        assert!(policy(&["read"]).authenticate("wrong", &read).is_none());
        let write = Request::builder()
            .method(Method::POST)
            .uri("/api/v1/profiles")
            .body(Body::empty())
            .unwrap();
        assert!(policy(&["read"]).authenticate(TOKEN, &write).is_none());
        let global = Request::builder()
            .method(Method::GET)
            .uri("/api/v1/status")
            .body(Body::empty())
            .unwrap();
        assert!(policy(&["read"]).authenticate(TOKEN, &global).is_none());
        let foreign = Request::builder()
            .method(Method::GET)
            .uri("/api/v1/profiles")
            .header("x-rustfrp-tenant", "other")
            .body(Body::empty())
            .unwrap();
        assert!(policy(&["*"]).authenticate(TOKEN, &foreign).is_none());
    }
}
