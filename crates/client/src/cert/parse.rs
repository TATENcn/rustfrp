//! Certificate bundle parsing and validation.
//!
//! Accepts PEM or base64 DER for both halves of a bundle, splits a fullchain
//! into leaf + intermediates, and proves the private key belongs to the leaf.
//! Nothing here trusts the caller's claim about which key goes with which
//! certificate: a mismatch is rejected before anything is written to disk.

use base64::Engine;
use sha2::{Digest, Sha256};
use x509_parser::extensions::GeneralName;
use x509_parser::prelude::*;

use crate::error::{ClientError, Result};

/// OIDs we compare against, spelled out as strings so the crate does not depend
/// on generated constant names.
const OID_RSA: &str = "1.2.840.113549.1.1.1";
const OID_EC: &str = "1.2.840.10045.2.1";
const OID_ED25519: &str = "1.3.101.112";

/// A bundle that passed every structural check.
#[derive(Debug)]
pub struct ParsedCertificate {
    pub primary_domain: String,
    pub sans: Vec<String>,
    pub issuer: String,
    pub serial: String,
    pub fingerprint_sha256: String,
    pub key_algo: String,
    /// RFC 3339, UTC.
    pub not_before: String,
    pub not_after: String,
    pub cert_der: Vec<u8>,
    /// Concatenated DER of every intermediate, leaf excluded. May be empty.
    pub chain_der: Vec<u8>,
    /// PEM label of the key, e.g. `PRIVATE KEY` or `RSA PRIVATE KEY`.
    pub key_label: String,
    pub key_der: Vec<u8>,
    /// Non-fatal findings (incomplete chain, short validity, ...).
    pub warnings: Vec<String>,
}

/// Parse and cross-check a certificate + private key bundle.
///
/// `cert_pem` may be a fullchain; `key_pem` may be PKCS#8 or PKCS#1, PEM or
/// bare base64 DER.
pub fn parse_bundle(cert_pem: &str, key_pem: &str) -> Result<ParsedCertificate> {
    let cert_blocks = decode_pem_blocks(cert_pem, "CERTIFICATE")?;
    if cert_blocks.is_empty() {
        return Err(ClientError::CertificateParse(
            "no CERTIFICATE block found in the certificate input".into(),
        ));
    }

    let leaf_der = cert_blocks[0].clone();
    let mut chain_der = Vec::new();
    for block in &cert_blocks[1..] {
        chain_der.extend_from_slice(block);
    }

    let (_, leaf) = parse_x509_certificate(&leaf_der)
        .map_err(|e| ClientError::CertificateParse(format!("leaf certificate is invalid: {e}")))?;

    let mut warnings = Vec::new();
    let mut sans = Vec::new();
    match leaf.subject_alternative_name() {
        Ok(Some(extension)) => {
            for name in &extension.value.general_names {
                match name {
                    GeneralName::DNSName(dns) => sans.push((*dns).to_ascii_lowercase()),
                    GeneralName::IPAddress(raw) => sans.push(format_ip(raw)),
                    _ => {}
                }
            }
        }
        Ok(None) => {}
        Err(e) => warnings.push(format!("subjectAltName could not be read: {e}")),
    }
    // The first SAN in the extension is the ordering the issuer intended; sort
    // only the display copy, so the "primary" domain is not chosen by alphabet.
    let first_san = sans.first().cloned();
    sans.sort();
    sans.dedup();

    // Prefer the common name: it is what the operator typed into the ACME
    // client, whereas the SAN list may begin with a www alias.
    let common_name = leaf
        .subject()
        .iter_common_name()
        .next()
        .and_then(|cn| cn.as_str().ok())
        .map(|cn| cn.to_ascii_lowercase())
        .filter(|cn| !cn.is_empty());

    let primary_domain = common_name.or(first_san).ok_or_else(|| {
        ClientError::CertificateParse(
            "certificate has neither subjectAltName nor a common name".into(),
        )
    })?;

    // Collect the subjects present in the chain so we can tell whether the leaf
    // was shipped with its issuer.
    let mut chain_subjects = Vec::new();
    for der in &cert_blocks[1..] {
        if let Ok((_, intermediate)) = parse_x509_certificate(der) {
            chain_subjects.push(intermediate.subject().as_raw().to_vec());
        }
    }
    if chain_subjects.is_empty() {
        warnings
            .push("no intermediate certificate was supplied; clients may reject the chain".into());
    } else if !chain_subjects
        .iter()
        .any(|subject| subject.as_slice() == leaf.issuer().as_raw())
    {
        warnings.push("the supplied intermediates do not include the certificate's issuer".into());
    }

    let key_algo = describe_key(leaf.public_key());
    let (key_label, key_der) = decode_private_key(key_pem)?;
    verify_key_matches(&leaf, &key_der)?;

    let not_before = leaf.validity().not_before.timestamp();
    let not_after = leaf.validity().not_after.timestamp();
    if not_after <= chrono::Utc::now().timestamp() {
        warnings.push("the certificate is already expired".into());
    }

    let fingerprint_sha256 = hex(&Sha256::digest(&leaf_der));

    Ok(ParsedCertificate {
        primary_domain,
        sans,
        issuer: leaf.issuer().to_string(),
        serial: hex(leaf.raw_serial()),
        fingerprint_sha256,
        key_algo,
        not_before: rfc3339(not_before),
        not_after: rfc3339(not_after),
        cert_der: leaf_der,
        chain_der,
        key_label,
        key_der,
        warnings,
    })
}

/// Split a concatenation of DER objects into its members.
///
/// Chains are stored as concatenated DER rather than PEM so the database holds
/// one canonical form; this recovers the individual certificates when writing
/// the PEM file frpc reads.
pub fn split_der_sequence(der: &[u8]) -> Vec<&[u8]> {
    let mut out = Vec::new();
    let mut rest = der;
    while !rest.is_empty() {
        match der_object_len(rest) {
            Some(len) if len <= rest.len() => {
                out.push(&rest[..len]);
                rest = &rest[len..];
            }
            _ => break,
        }
    }
    out
}

/// Wrap DER bytes in a PEM block.
pub fn encode_pem(label: &str, der: &[u8]) -> String {
    let base64 = base64::engine::general_purpose::STANDARD.encode(der);
    let mut out = format!("-----BEGIN {label}-----\n");
    for chunk in base64.as_bytes().chunks(64) {
        out.push_str(&String::from_utf8_lossy(chunk));
        out.push('\n');
    }
    out.push_str(&format!("-----END {label}-----\n"));
    out
}

/// Length of the DER object starting at `bytes`, header included.
fn der_object_len(bytes: &[u8]) -> Option<usize> {
    let first = *bytes.first()?;
    if first != 0x30 {
        return None;
    }
    let length = *bytes.get(1)?;
    if length < 0x80 {
        return Some(2 + usize::from(length));
    }
    let count = usize::from(length & 0x7f);
    if count == 0 || count > 4 {
        return None;
    }
    let mut value = 0_usize;
    for byte in bytes.get(2..2 + count)? {
        value = (value << 8) | usize::from(*byte);
    }
    Some(2 + count + value)
}

/// Decode every PEM block with the given label, or treat the input as bare
/// base64 DER when no block is present.
fn decode_pem_blocks(input: &str, label: &str) -> Result<Vec<Vec<u8>>> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err(ClientError::CertificateParse(format!(
            "no {label} input supplied"
        )));
    }

    let mut blocks = Vec::new();
    let mut rest: &str = trimmed;
    let begin = format!("-----BEGIN {label}-----");
    let end = format!("-----END {label}-----");
    while let Some(start) = rest.find(&begin) {
        let after = &rest[start + begin.len()..];
        let Some(stop) = after.find(&end) else {
            return Err(ClientError::CertificateParse(format!(
                "{label} block is not terminated"
            )));
        };
        let body: String = after[..stop]
            .chars()
            .filter(|c| !c.is_whitespace())
            .collect();
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(body)
            .map_err(|e| {
                ClientError::CertificateParse(format!("{label} block is not valid base64: {e}"))
            })?;
        blocks.push(decoded);
        rest = &after[stop + end.len()..];
    }

    if !blocks.is_empty() {
        return Ok(blocks);
    }

    // No PEM header: accept a bare base64 body, which is what clients that
    // strip headers before POSTing produce.
    let compact: String = trimmed.chars().filter(|c| !c.is_whitespace()).collect();
    if compact.contains("-----") {
        return Ok(blocks);
    }
    match base64::engine::general_purpose::STANDARD.decode(compact) {
        Ok(decoded) if !decoded.is_empty() => Ok(vec![decoded]),
        _ => Err(ClientError::CertificateParse(format!(
            "input is neither a {label} PEM block nor base64 DER"
        ))),
    }
}

/// Decode a private key and normalise it to `(PEM label, PKCS#8 DER)`.
///
/// PKCS#1 (`RSA PRIVATE KEY`) inputs are wrapped so the stored form is always
/// PKCS#8 — a single representation means the PEM we write back is always
/// consistent with the label we write it under.
fn decode_private_key(input: &str) -> Result<(String, Vec<u8>)> {
    for label in ["PRIVATE KEY", "RSA PRIVATE KEY", "EC PRIVATE KEY"] {
        let blocks = decode_pem_blocks(input, label)?;
        if let Some(block) = blocks.into_iter().next() {
            return normalize_key(block);
        }
    }
    Err(ClientError::CertificateParse(
        "no private key found in the key input".into(),
    ))
}

/// Return the key as PKCS#8, wrapping PKCS#1 RSA when needed.
fn normalize_key(der: Vec<u8>) -> Result<(String, Vec<u8>)> {
    if key_parses(&der) {
        return Ok(("PRIVATE KEY".to_string(), der));
    }
    let wrapped = wrap_pkcs1_rsa(&der)?;
    if key_parses(&wrapped) {
        return Ok(("PRIVATE KEY".to_string(), wrapped));
    }
    Err(ClientError::CertificateParse(
        "the private key is neither PKCS#8 nor PKCS#1 RSA".into(),
    ))
}

/// True when a crypto backend can load the key as PKCS#8.
fn key_parses(der: &[u8]) -> bool {
    if ring::signature::RsaKeyPair::from_pkcs8(der).is_ok() {
        return true;
    }
    let rng = ring::rand::SystemRandom::new();
    if ring::signature::EcdsaKeyPair::from_pkcs8(
        &ring::signature::ECDSA_P256_SHA256_ASN1_SIGNING,
        der,
        &rng,
    )
    .is_ok()
    {
        return true;
    }
    if ring::signature::EcdsaKeyPair::from_pkcs8(
        &ring::signature::ECDSA_P384_SHA384_ASN1_SIGNING,
        der,
        &rng,
    )
    .is_ok()
    {
        return true;
    }
    ring::signature::Ed25519KeyPair::from_pkcs8_maybe_unchecked(der).is_ok()
}

/// Wrap a PKCS#1 RSA key in a PKCS#8 envelope.
fn wrap_pkcs1_rsa(pkcs1: &[u8]) -> Result<Vec<u8>> {
    const RSA_ALGORITHM_ID: &[u8] = &[
        0x30, 0x0d, 0x06, 0x09, 0x2a, 0x86, 0x48, 0x86, 0xf7, 0x0d, 0x01, 0x01, 0x01, 0x05, 0x00,
    ];
    let version = [0x02, 0x01, 0x00];

    let mut inner = Vec::new();
    inner.extend_from_slice(&version);
    inner.extend_from_slice(RSA_ALGORITHM_ID);

    let mut octet = Vec::new();
    octet.push(0x04);
    push_der_len(&mut octet, pkcs1.len());
    octet.extend_from_slice(pkcs1);
    inner.extend_from_slice(&octet);

    let mut out = Vec::new();
    out.push(0x30);
    push_der_len(&mut out, inner.len());
    out.extend_from_slice(&inner);
    Ok(out)
}

fn push_der_len(out: &mut Vec<u8>, len: usize) {
    if len < 0x80 {
        out.push(len as u8);
    } else if len <= 0xff {
        out.push(0x81);
        out.push(len as u8);
    } else {
        out.push(0x82);
        out.push((len >> 8) as u8);
        out.push((len & 0xff) as u8);
    }
}

/// Reject a bundle whose key does not belong to the leaf certificate.
fn verify_key_matches(leaf: &X509Certificate<'_>, key_der: &[u8]) -> Result<()> {
    let mismatched = || {
        ClientError::CertificateKeyMismatch(
            "the private key does not match the certificate's public key".into(),
        )
    };

    // The BIT STRING payload is exactly the encoding the crypto backend derives
    // from a private key: `SEQUENCE{modulus, exponent}` for RSA, and the
    // uncompressed point for ECDSA. So one comparison covers both families.
    let certificate_public = leaf.public_key().subject_public_key.data.to_vec();
    if certificate_public.is_empty() {
        return Err(ClientError::CertificateParse(
            "certificate has an empty public key".into(),
        ));
    }

    if let Ok(pair) = ring::signature::RsaKeyPair::from_pkcs8(key_der) {
        return if pair.public().as_ref() == certificate_public.as_slice() {
            Ok(())
        } else {
            Err(mismatched())
        };
    }

    use ring::signature::KeyPair;
    let rng = ring::rand::SystemRandom::new();
    for algorithm in [
        &ring::signature::ECDSA_P256_SHA256_ASN1_SIGNING,
        &ring::signature::ECDSA_P384_SHA384_ASN1_SIGNING,
    ] {
        if let Ok(pair) = ring::signature::EcdsaKeyPair::from_pkcs8(algorithm, key_der, &rng) {
            return if pair.public_key().as_ref() == certificate_public.as_slice() {
                Ok(())
            } else {
                Err(mismatched())
            };
        }
    }

    Err(mismatched())
}

fn describe_key(spki: &SubjectPublicKeyInfo<'_>) -> String {
    let oid = spki.algorithm.algorithm.to_id_string();
    let bits = spki.parsed().map(|key| key.key_size()).unwrap_or_default();
    match oid.as_str() {
        OID_RSA => format!("rsa-{bits}"),
        // The named curve lives in the algorithm parameters; the key size is
        // enough to label it and avoids re-parsing the parameter OID.
        OID_EC => match bits {
            256 => "ecdsa-p256".into(),
            384 => "ecdsa-p384".into(),
            521 => "ecdsa-p521".into(),
            other => format!("ecdsa-{other}"),
        },
        OID_ED25519 => "ed25519".into(),
        other => format!("oid-{other}"),
    }
}

fn format_ip(raw: &[u8]) -> String {
    match raw.len() {
        4 => std::net::Ipv4Addr::new(raw[0], raw[1], raw[2], raw[3]).to_string(),
        16 => {
            let mut octets = [0_u8; 16];
            octets.copy_from_slice(raw);
            std::net::Ipv6Addr::from(octets).to_string()
        }
        _ => raw.iter().fold(String::new(), |mut out, byte| {
            if !out.is_empty() {
                out.push('.');
            }
            out.push_str(&byte.to_string());
            out
        }),
    }
}

fn rfc3339(timestamp: i64) -> String {
    chrono::DateTime::from_timestamp(timestamp, 0)
        .unwrap_or_else(|| chrono::DateTime::from_timestamp(0, 0).expect("epoch is valid"))
        .to_rfc3339()
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().fold(String::new(), |mut out, byte| {
        use std::fmt::Write;
        let _ = write!(out, "{byte:02x}");
        out
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const CERT: &str = include_str!("../../tests/fixtures/cert_ec.pem");
    const KEY: &str = include_str!("../../tests/fixtures/cert_ec.key");
    const RSA_KEY: &str = include_str!("../../tests/fixtures/cert_rsa.key");

    #[test]
    fn parses_ecdsa_bundle() {
        let parsed = parse_bundle(CERT, KEY).unwrap();
        assert_eq!(parsed.primary_domain, "test.example.com");
        assert!(parsed.sans.contains(&"test.example.com".to_string()));
        assert_eq!(parsed.key_algo, "ecdsa-p256");
        assert!(parsed.chain_der.is_empty());
        assert_eq!(parsed.fingerprint_sha256.len(), 64);
        // No intermediates supplied on purpose: the warning must say so.
        assert!(parsed.warnings.iter().any(|w| w.contains("intermediate")));
    }

    #[test]
    fn rejects_mismatched_key() {
        let error = parse_bundle(CERT, RSA_KEY).unwrap_err();
        assert!(matches!(error, ClientError::CertificateKeyMismatch(_)));
    }

    #[test]
    fn accepts_bare_base64_certificate() {
        let der = decode_pem_blocks(CERT, "CERTIFICATE").unwrap().remove(0);
        let bare = base64::engine::general_purpose::STANDARD.encode(der);
        let parsed = parse_bundle(&bare, KEY).unwrap();
        assert_eq!(parsed.primary_domain, "test.example.com");
    }

    #[test]
    fn fingerprint_is_stable() {
        let first = parse_bundle(CERT, KEY).unwrap();
        let second = parse_bundle(CERT, KEY).unwrap();
        assert_eq!(first.fingerprint_sha256, second.fingerprint_sha256);
    }

    #[test]
    fn splits_concatenated_der() {
        let der = decode_pem_blocks(CERT, "CERTIFICATE").unwrap().remove(0);
        let joined: Vec<u8> = der.iter().chain(der.iter()).copied().collect();
        assert_eq!(split_der_sequence(&joined).len(), 2);
    }

    #[test]
    fn pem_round_trip() {
        let der = decode_pem_blocks(CERT, "CERTIFICATE").unwrap().remove(0);
        let pem = encode_pem("CERTIFICATE", &der);
        assert_eq!(decode_pem_blocks(&pem, "CERTIFICATE").unwrap(), vec![der]);
    }
}
