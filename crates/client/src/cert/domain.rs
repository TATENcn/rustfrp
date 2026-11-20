//! Which domains a certificate is allowed to serve, and which certificate a
//! channel is allowed to install.
//!
//! Two opposite directions live here because they are easy to confuse and
//! getting either backwards is a security bug:
//!
//! * **Push**: every SAN of the submitted certificate must be covered by the
//!   token's allowlist. Otherwise a leak of one token could overwrite the
//!   certificate for an unrelated domain.
//! * **Pull**: the channel's target domain must be covered by the returned
//!   certificate. Otherwise a misconfigured or hostile endpoint could install a
//!   certificate for a domain the operator never pointed it at.
//!
//! Both use one matching rule: exact, or a single-label wildcard. That mirrors
//! how TLS wildcards actually work, so `*.example.com` covers `a.example.com`
//! and not `a.b.example.com`.

/// True when `candidate` is served by `pattern`.
///
/// An exact match always counts. A pattern of the form `*.suffix` covers
/// exactly one extra label.
pub fn covers(pattern: &str, candidate: &str) -> bool {
    let pattern = pattern.trim().to_ascii_lowercase();
    let candidate = candidate.trim().to_ascii_lowercase();
    if pattern.is_empty() || candidate.is_empty() {
        return false;
    }
    if candidate == pattern {
        return true;
    }
    if let Some(suffix) = pattern.strip_prefix("*.") {
        if let Some((label, rest)) = candidate.split_once('.') {
            return !label.is_empty() && rest == suffix;
        }
    }
    false
}

/// Domains the certificate claims to serve.
///
/// Falls back to the primary domain when a bundle somehow carries no SANs, so a
/// single-SAN certificate is never treated as "serves nothing".
pub fn served_domains(sans: &[String], primary: &str) -> Vec<String> {
    if !sans.is_empty() {
        return sans.to_vec();
    }
    if primary.trim().is_empty() {
        Vec::new()
    } else {
        vec![primary.trim().to_ascii_lowercase()]
    }
}

/// Push direction: every domain the certificate serves must be allowed.
///
/// Returns the domains that were not allowed, so the caller can name them in the
/// error instead of saying only "forbidden".
pub fn disallowed(sans: &[String], primary: &str, allowed: &[String]) -> Vec<String> {
    served_domains(sans, primary)
        .into_iter()
        .filter(|domain| !allowed.iter().any(|pattern| covers(pattern, domain)))
        .collect()
}

/// Pull direction: the certificate must actually cover what the channel asked
/// for.
///
/// A server may return a wider certificate (an extra SAN is normal when a CA
/// reissues), so this checks containment rather than equality.
pub fn covers_target(sans: &[String], primary: &str, target: &str) -> bool {
    let served = served_domains(sans, primary);
    // The target counts as covered when the certificate serves it directly, and
    // also when the certificate serves a wildcard that includes it.
    served.iter().any(|domain| covers(domain, target))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sans(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_string()).collect()
    }

    #[test]
    fn exact_and_single_label_wildcards_match() {
        assert!(covers("a.example.com", "a.example.com"));
        assert!(covers("A.Example.com", "a.example.com"));
        assert!(covers("*.example.com", "a.example.com"));
        assert!(!covers("*.example.com", "a.b.example.com"));
        assert!(!covers("*.example.com", "example.com"));
        assert!(!covers("a.example.com", "b.example.com"));
        assert!(!covers("", "a.example.com"));
    }

    #[test]
    fn push_requires_every_san_to_be_allowed() {
        let allowed = sans(&["example.com", "*.example.com"]);
        assert!(disallowed(&sans(&["a.example.com"]), "a.example.com", &allowed).is_empty());

        // One uncovered SAN is enough to reject the whole bundle.
        let rejected = disallowed(
            &sans(&["a.example.com", "other.test"]),
            "a.example.com",
            &allowed,
        );
        assert_eq!(rejected, vec!["other.test".to_string()]);
    }

    #[test]
    fn push_does_not_require_the_allowlist_to_be_exhausted() {
        // Allowing more domains than a certificate covers is normal: the token
        // is used for several certificates over its lifetime.
        let allowed = sans(&["*.example.com", "*.other.test"]);
        assert!(disallowed(&sans(&["a.example.com"]), "a.example.com", &allowed).is_empty());
    }

    #[test]
    fn pull_accepts_a_certificate_that_covers_the_target() {
        assert!(covers_target(&sans(&["a.example.com"]), "a.example.com", "a.example.com"));
        assert!(covers_target(&sans(&["*.example.com"]), "*.example.com", "a.example.com"));
        // A wider certificate is fine: CAs reissue with extra SANs.
        assert!(covers_target(
            &sans(&["a.example.com", "b.example.com"]),
            "a.example.com",
            "a.example.com"
        ));
    }

    #[test]
    fn pull_rejects_a_certificate_for_the_wrong_domain() {
        assert!(!covers_target(&sans(&["evil.test"]), "evil.test", "a.example.com"));
        // A wildcard for a parent domain does not cover a subdomain's own name.
        assert!(!covers_target(&sans(&["*.other.test"]), "*.other.test", "a.example.com"));
        // And a deeper wildcard must not be treated as a parent match.
        assert!(!covers_target(&sans(&["*.b.example.com"]), "*.b.example.com", "a.example.com"));
    }

    #[test]
    fn pull_falls_back_to_the_primary_domain_without_sans() {
        assert!(covers_target(&[], "a.example.com", "a.example.com"));
        assert!(!covers_target(&[], "a.example.com", "b.example.com"));
        assert!(!covers_target(&[], "", "a.example.com"));
    }
}
