//! Certificate storage and apply behaviour, exercised through the database.
//!
//! These tests target the properties that matter for correctness in production:
//! idempotent import, encrypted-at-rest material, tenant isolation, refusal to
//! delete a bound certificate, and apply keeping the declared files in sync.

use rustfrp_client::cert::{parse_bundle, CertificateSecrets};
use rustfrp_client::config::model::LocalProxy;
use rustfrp_client::db::certificate::{ImportRequest, SOURCE_MANUAL};
use rustfrp_client::db::{migrate, Database};

const CERT: &str = include_str!("fixtures/cert_ec.pem");
const KEY: &str = include_str!("fixtures/cert_ec.key");

async fn database() -> (tempfile::TempDir, Database) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.db");
    let db = Database::open(path.to_str().unwrap()).await.unwrap();
    {
        let conn = db.lock().await;
        migrate::run(&conn).unwrap();
    }
    (dir, db)
}

fn import_request() -> ImportRequest {
    ImportRequest {
        parsed: parse_bundle(CERT, KEY).unwrap(),
        source: SOURCE_MANUAL.into(),
        source_detail: None,
    }
}

#[tokio::test]
async fn import_is_idempotent_and_stores_no_plaintext_key() {
    let (_dir, db) = database().await;

    let first = db
        .import_certificate("default", import_request())
        .await
        .unwrap();
    let second = db
        .import_certificate("default", import_request())
        .await
        .unwrap();
    assert_eq!(first.id, second.id);
    assert_eq!(
        db.list_certificates("default").await.unwrap().len(),
        1,
        "re-importing the same fingerprint must not add a row"
    );

    // The database file must not contain the private key in readable form.
    let bytes = std::fs::read(db.path()).unwrap();
    let haystack = String::from_utf8_lossy(&bytes);
    assert!(!haystack.contains("BEGIN PRIVATE KEY"));
    assert!(!haystack.contains("BEGIN CERTIFICATE"));

    // Material still round-trips for the apply engine.
    let secrets = db.certificate_secrets(first.id, "default").await.unwrap();
    assert!(secrets.fullchain_pem().contains("BEGIN CERTIFICATE"));
    assert!(secrets.key_pem().contains("BEGIN PRIVATE KEY"));
}

#[tokio::test]
async fn certificate_is_scoped_to_its_tenant() {
    let (_dir, db) = database().await;
    let certificate = db
        .import_certificate("alpha", import_request())
        .await
        .unwrap();

    assert!(db.list_certificates("beta").await.unwrap().is_empty());
    assert!(db
        .certificate_in_tenant(certificate.id, "beta")
        .await
        .is_err());
    assert!(db
        .certificate_secrets(certificate.id, "beta")
        .await
        .is_err());
}

#[tokio::test]
async fn deletion_is_blocked_while_bound() {
    let (_dir, db) = database().await;
    let certificate = db
        .import_certificate("default", import_request())
        .await
        .unwrap();

    let proxy = proxy_with_certificate_paths(db.path().parent().unwrap());
    let proxy_id = db.insert_proxy(&proxy).await.unwrap();

    let binding = db
        .bind_certificate(certificate.id, proxy_id, "default")
        .await
        .unwrap();
    assert_eq!(binding.cert_id, certificate.id);

    let error = db
        .delete_certificate(certificate.id, "default")
        .await
        .unwrap_err();
    assert!(error.to_string().contains("unbind"), "got: {error}");

    db.unbind_certificate(binding.id, "default", false)
        .await
        .unwrap();
    db.delete_certificate(certificate.id, "default")
        .await
        .unwrap();
    assert!(db.list_certificates("default").await.unwrap().is_empty());
}

#[tokio::test]
async fn binding_requires_a_plugin_that_holds_a_certificate() {
    let (_dir, db) = database().await;
    let certificate = db
        .import_certificate("default", import_request())
        .await
        .unwrap();

    let proxy = LocalProxy {
        name: "plain-http".into(),
        proxy_type: rustfrp_client::config::model::ProxyType::Http,
        custom_domains: Some(vec!["plain.example.com".into()]),
        plugin_config: None,
        ..LocalProxy::default()
    };
    let proxy_id = db.insert_proxy(&proxy).await.unwrap();

    let error = db
        .bind_certificate(certificate.id, proxy_id, "default")
        .await
        .unwrap_err();
    assert!(
        error.to_string().contains("plugin configuration"),
        "got: {error}"
    );
}

#[tokio::test]
async fn apply_writes_the_files_the_proxy_declares() {
    let (_dir, db) = database().await;
    let certificate = db
        .import_certificate("default", import_request())
        .await
        .unwrap();

    let dir = tempfile::tempdir().unwrap();
    let proxy = proxy_with_certificate_paths(dir.path());
    let proxy_id = db.insert_proxy(&proxy).await.unwrap();
    db.bind_certificate(certificate.id, proxy_id, "default")
        .await
        .unwrap();

    let (applied, failed) = db
        .apply_certificate(certificate.id, "default")
        .await
        .unwrap();
    assert!(failed.is_empty(), "apply failed: {failed:?}");
    assert_eq!(applied.len(), 1);

    let crt = std::fs::read_to_string(dir.path().join("fullchain.pem")).unwrap();
    let key = std::fs::read_to_string(dir.path().join("privkey.pem")).unwrap();
    assert!(crt.contains("BEGIN CERTIFICATE"));
    assert!(key.contains("BEGIN PRIVATE KEY"));

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(dir.path().join("privkey.pem"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600, "private key must not be readable by others");
    }

    // The binding records what was written, which is what drift detection reads.
    let refreshed = db
        .certificate_in_tenant(certificate.id, "default")
        .await
        .unwrap();
    assert_eq!(
        refreshed.bindings[0].applied_fingerprint.as_deref(),
        Some(certificate.fingerprint_sha256.as_str())
    );
}

#[tokio::test]
async fn apply_reports_each_failing_binding_separately() {
    let (_dir, db) = database().await;
    let certificate = db
        .import_certificate("default", import_request())
        .await
        .unwrap();

    let good_dir = tempfile::tempdir().unwrap();
    let good_id = db
        .insert_proxy(&proxy_with_certificate_paths(good_dir.path()))
        .await
        .unwrap();

    // A file where a directory would have to be: creating the path fails.
    let blocked = good_dir.path().join("blocked");
    std::fs::write(&blocked, b"not a directory").unwrap();
    let mut bad = proxy_with_certificate_paths(&blocked.join("sub"));
    bad.name = "blocked-proxy".into();
    let bad_id = db.insert_proxy(&bad).await.unwrap();

    db.bind_certificate(certificate.id, good_id, "default")
        .await
        .unwrap();
    db.bind_certificate(certificate.id, bad_id, "default")
        .await
        .unwrap();

    let (applied, failed) = db
        .apply_certificate(certificate.id, "default")
        .await
        .unwrap();
    assert_eq!(applied.len(), 1, "the writable binding must still succeed");
    assert_eq!(failed.len(), 1);
    assert!(
        good_dir.path().join("fullchain.pem").exists(),
        "a failing sibling must not undo a successful write"
    );

    let refreshed = db
        .certificate_in_tenant(certificate.id, "default")
        .await
        .unwrap();
    let failed_binding = refreshed
        .bindings
        .iter()
        .find(|binding| binding.proxy_id == bad_id)
        .unwrap();
    assert!(failed_binding.last_error.is_some());
    assert!(failed_binding.applied_fingerprint.is_none());
}

#[tokio::test]
async fn ingest_token_is_hashed_and_domain_scoped() {
    let (_dir, db) = database().await;
    let created = db
        .create_source(
            "default",
            rustfrp_client::db::cert_ingest::SourceInput {
                certificate_id: None,
                kind: rustfrp_client::db::cert_ingest::SourceKind::Push,
                name: "certd".into(),
                domain: String::new(),
                base_url: String::new(),
                domains: vec!["Test.Example.com".into()],
                key_id: None,
                key_secret: None,
                enabled: true,
            },
        )
        .await
        .unwrap();
    let plaintext = created.plaintext.clone().unwrap();

    // Only the digest is stored.
    let stored = {
        let conn = db.lock().await;
        conn.query_row(
            "SELECT token_sha256 FROM certificate_source WHERE id = ?1",
            [created.source.id],
            |row| row.get::<_, String>(0),
        )
        .unwrap()
    };
    assert_ne!(stored, plaintext);
    assert_eq!(
        stored,
        rustfrp_client::db::cert_ingest::token_digest(&plaintext)
    );

    let grant = db
        .authenticate_source_token(&plaintext)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(grant.tenant, "default");
    assert_eq!(grant.allowed_domains, vec!["test.example.com".to_string()]);
    assert!(
        grant.certificate_id.is_none(),
        "an unbound token stays unbound"
    );

    assert!(db
        .authenticate_source_token("wrong-token-value-xxxxxxxx")
        .await
        .unwrap()
        .is_none());

    // Deleting the channel invalidates its token immediately.
    db.delete_source(created.source.id, "default")
        .await
        .unwrap();
    assert!(db
        .authenticate_source_token(&plaintext)
        .await
        .unwrap()
        .is_none());
}

#[tokio::test]
async fn pull_source_without_credentials_reads_as_empty_secret() {
    let (_dir, db) = database().await;
    let source = db
        .create_source(
            "default",
            rustfrp_client::db::cert_ingest::SourceInput {
                certificate_id: None,
                kind: rustfrp_client::db::cert_ingest::SourceKind::Url,
                name: "plain-url".into(),
                domain: "test.example.com".into(),
                base_url: "https://example.com/bundle.json".into(),
                domains: vec![],
                key_id: None,
                key_secret: None,
                enabled: true,
            },
        )
        .await
        .unwrap();

    assert!(!source.source.has_secret);
    // A NULL `enc_secret` must read as "no credentials", not as a row that
    // does not exist and not as a type error from rusqlite.
    let secret = db.source_secret(source.source.id, "default").await.unwrap();
    assert!(secret.key_id.is_none() && secret.key_secret.is_none());
}

#[tokio::test]
async fn pull_source_secret_never_leaves_the_database_layer() {
    let (_dir, db) = database().await;
    let source = db
        .create_source(
            "default",
            rustfrp_client::db::cert_ingest::SourceInput {
                certificate_id: None,
                kind: rustfrp_client::db::cert_ingest::SourceKind::Certd,
                name: "certd".into(),
                domain: "test.example.com".into(),
                base_url: "https://certd.example.com".into(),
                domains: vec![],
                key_id: Some("kid".into()),
                key_secret: Some("super-secret".into()),
                enabled: true,
            },
        )
        .await
        .unwrap();

    assert!(source.source.has_secret);
    let listed = serde_json::to_string(&db.list_sources("default", None).await.unwrap()).unwrap();
    assert!(!listed.contains("super-secret"), "secret leaked: {listed}");

    let secret = db.source_secret(source.source.id, "default").await.unwrap();
    assert_eq!(secret.key_secret.as_deref(), Some("super-secret"));
}

fn proxy_with_certificate_paths(dir: &std::path::Path) -> LocalProxy {
    LocalProxy {
        name: "nas-web".into(),
        proxy_type: rustfrp_client::config::model::ProxyType::Https,
        local_ip: "127.0.0.1".into(),
        local_port: 8080,
        custom_domains: Some(vec!["test.example.com".into()]),
        plugin_config: Some(serde_json::json!({
            "type": "https2http",
            "localAddr": "127.0.0.1:8080",
            "crtPath": dir.join("fullchain.pem").to_string_lossy(),
            "keyPath": dir.join("privkey.pem").to_string_lossy(),
        })),
        ..LocalProxy::default()
    }
}

/// Silence the unused import when only the metadata type is needed.
#[allow(dead_code)]
fn _assert_secrets_is_not_serializable(_: CertificateSecrets) {}

/// A certificate exists before it has material: that is what lets an operator
/// configure a channel first and fill the certificate later.
#[tokio::test]
async fn a_certificate_can_exist_before_any_material_arrives() {
    let (_dir, db) = database().await;

    let certificate = db
        .ensure_certificate("default", "Test.Example.com")
        .await
        .unwrap();
    assert_eq!(certificate.primary_domain, "test.example.com");
    assert!(certificate.material_id.is_none());
    assert!(
        certificate.days_remaining.is_none(),
        "no expiry before material"
    );
    assert!(certificate.not_after.is_empty());
    // A certificate with no material is `pending`, and the reason lives in
    // `status_reason` rather than the free-form warning list.
    assert_eq!(
        certificate.status,
        rustfrp_client::cert::CertificateStatus::Pending
    );
    assert!(!certificate.usable);
    assert!(certificate
        .status_reason
        .as_deref()
        .unwrap_or_default()
        .contains("no certificate has been received"));

    // Asking for the encryption material is a state error, not a 404.
    let error = db
        .certificate_secrets(certificate.id, "default")
        .await
        .unwrap_err();
    assert!(error.to_string().contains("no material yet"), "{error}");

    // Ensuring twice must not create a second identity.
    let again = db
        .ensure_certificate("default", "test.example.com")
        .await
        .unwrap();
    assert_eq!(again.id, certificate.id);
    assert_eq!(db.list_certificates("default").await.unwrap().len(), 1);
}

/// The identity is the domain, so a renewal replaces the newest material
/// instead of appending a second certificate.
#[tokio::test]
async fn reissuing_a_certificate_updates_instead_of_duplicating() {
    let (_dir, db) = database().await;

    let first = db
        .import_certificate("default", import_request())
        .await
        .unwrap();
    assert_eq!(first.fingerprint_sha256.len(), 64);

    // Same bundle again: idempotent.
    let same = db
        .import_certificate("default", import_request())
        .await
        .unwrap();
    assert_eq!(same.id, first.id);
    assert_eq!(same.fingerprint_sha256, first.fingerprint_sha256);
    assert_eq!(db.list_certificates("default").await.unwrap().len(), 1);

    // A different bundle for the same domain is a renewal: same identity, new
    // material. The fixture's RSA pair is a different certificate than the EC
    // one, and both cover the same domain set.
    let renewed = db
        .import_certificate(
            "default",
            ImportRequest {
                parsed: parse_bundle(
                    include_str!("fixtures/cert_rsa.pem"),
                    include_str!("fixtures/cert_rsa.key"),
                )
                .unwrap(),
                source: SOURCE_MANUAL.into(),
                source_detail: Some("renewal".into()),
            },
        )
        .await
        .unwrap();

    assert_eq!(renewed.id, first.id, "the identity must not change");
    assert_ne!(renewed.fingerprint_sha256, first.fingerprint_sha256);
    assert_eq!(db.list_certificates("default").await.unwrap().len(), 1);
    assert_eq!(renewed.source_detail.as_deref(), Some("renewal"));
}

/// Channels are switchable: the same row moves between certificates, and exactly
/// one channel per certificate can be primary.
#[tokio::test]
async fn a_channel_can_move_between_certificates() {
    let (_dir, db) = database().await;
    let first = db
        .ensure_certificate("default", "a.example.com")
        .await
        .unwrap();
    let second = db
        .ensure_certificate("default", "b.example.com")
        .await
        .unwrap();

    let push = db
        .create_source(
            "default",
            rustfrp_client::db::cert_ingest::SourceInput {
                certificate_id: Some(first.id),
                kind: rustfrp_client::db::cert_ingest::SourceKind::Push,
                name: "certd".into(),
                domain: String::new(),
                base_url: String::new(),
                domains: vec!["*.example.com".into()],
                key_id: None,
                key_secret: None,
                enabled: true,
            },
        )
        .await
        .unwrap();

    // The first channel of a certificate becomes its primary automatically.
    assert!(push.source.is_primary);
    assert_eq!(push.source.certificate_id, Some(first.id));

    // Move it. The old certificate must not keep a stale primary.
    let moved = db
        .set_source_certificate(push.source.id, Some(second.id), "default")
        .await
        .unwrap();
    assert_eq!(moved.certificate_id, Some(second.id));
    assert!(moved.is_primary, "the new certificate adopts it as primary");
    assert!(db
        .list_sources("default", Some(first.id))
        .await
        .unwrap()
        .is_empty());

    // Detaching keeps the channel (and its token) alive.
    let detached = db
        .set_source_certificate(push.source.id, None, "default")
        .await
        .unwrap();
    assert!(detached.certificate_id.is_none());
    assert!(db
        .authenticate_source_token(&push.plaintext.clone().unwrap())
        .await
        .unwrap()
        .is_some());
}

/// A certificate with two channels uses exactly one as primary, and switching it
/// demotes the other rather than violating the partial unique index.
#[tokio::test]
async fn only_one_channel_per_certificate_is_primary() {
    let (_dir, db) = database().await;
    let certificate = db
        .ensure_certificate("default", "a.example.com")
        .await
        .unwrap();

    let push = db
        .create_source(
            "default",
            rustfrp_client::db::cert_ingest::SourceInput {
                certificate_id: Some(certificate.id),
                kind: rustfrp_client::db::cert_ingest::SourceKind::Push,
                name: "certd".into(),
                domain: String::new(),
                base_url: String::new(),
                domains: vec!["a.example.com".into()],
                key_id: None,
                key_secret: None,
                enabled: true,
            },
        )
        .await
        .unwrap();
    let pull = db
        .create_source(
            "default",
            rustfrp_client::db::cert_ingest::SourceInput {
                certificate_id: Some(certificate.id),
                kind: rustfrp_client::db::cert_ingest::SourceKind::Certd,
                name: "nightly".into(),
                domain: "a.example.com".into(),
                base_url: "https://certd.example.com".into(),
                domains: vec![],
                key_id: Some("kid".into()),
                key_secret: Some("secret".into()),
                enabled: true,
            },
        )
        .await
        .unwrap();

    assert!(push.source.is_primary, "the first channel is primary");
    assert!(
        !pull.source.is_primary,
        "a second channel does not steal it"
    );

    let promoted = db
        .set_primary_source(pull.source.id, "default")
        .await
        .unwrap();
    assert!(promoted.is_primary);

    let sources = db
        .list_sources("default", Some(certificate.id))
        .await
        .unwrap();
    assert_eq!(sources.iter().filter(|source| source.is_primary).count(), 1);
    assert!(sources
        .iter()
        .any(|source| source.id == push.source.id && !source.is_primary));
}

/// A pull channel's target domain must be one the attached certificate covers.
/// Creating it is where the mistake is cheap to fix; the first fetch is not.
#[tokio::test]
async fn a_pull_channel_target_must_be_covered_by_the_certificate() {
    let (_dir, db) = database().await;
    let certificate = db
        .import_certificate("default", import_request())
        .await
        .unwrap();
    assert_eq!(certificate.primary_domain, "test.example.com");

    let input = |domain: &str| rustfrp_client::db::cert_ingest::SourceInput {
        certificate_id: Some(certificate.id),
        kind: rustfrp_client::db::cert_ingest::SourceKind::Url,
        name: "pull".into(),
        domain: domain.into(),
        base_url: "https://example.com/bundle.json".into(),
        domains: vec![],
        key_id: None,
        key_secret: None,
        enabled: true,
    };

    // A covered domain is accepted.
    db.create_source("default", input("test.example.com"))
        .await
        .unwrap();

    // `other.test` is not in the certificate's SANs.
    let error = db
        .create_source("default", input("other.test"))
        .await
        .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("does not cover the channel's domain"),
        "{error}"
    );

    // An unattached channel has nothing to check against, so it is allowed:
    // this is how a certificate is bootstrapped before any material exists.
    let mut unattached = input("other.test");
    unattached.certificate_id = None;
    unattached.name = "bootstrap".into();
    db.create_source("default", unattached).await.unwrap();
}

/// "Newest material" is the most recently stored one, even when it expires
/// earlier. A reissue with a shorter validity must win, or the daemon would keep
/// serving an older bundle that merely has a later `not_after`.
#[tokio::test]
async fn a_reissue_with_a_shorter_validity_still_becomes_current() {
    let (_dir, db) = database().await;

    // The rule under test is the ordering, so the fixtures' dates are irrelevant.
    let first = db
        .import_certificate("default", import_request())
        .await
        .unwrap();
    let second = db
        .import_certificate(
            "default",
            ImportRequest {
                parsed: parse_bundle(
                    include_str!("fixtures/cert_rsa.pem"),
                    include_str!("fixtures/cert_rsa.key"),
                )
                .unwrap(),
                source: SOURCE_MANUAL.into(),
                source_detail: None,
            },
        )
        .await
        .unwrap();

    assert_eq!(second.id, first.id);
    assert_ne!(
        second.fingerprint_sha256, first.fingerprint_sha256,
        "a reissue is a different bundle"
    );

    // The reported material must be the one that was imported last, regardless
    // of which expiry date happens to be later.
    let latest_id = {
        let conn = db.lock().await;
        conn.query_row(
            "SELECT MAX(id) FROM certificate_material WHERE certificate_id = ?1",
            [first.id],
            |row| row.get::<_, i64>(0),
        )
        .unwrap()
    };
    assert_eq!(
        second.material_id,
        Some(latest_id),
        "the newest stored material wins, not the one expiring last"
    );

    // And the secrets read back must match that same version.
    let secrets = db.certificate_secrets(first.id, "default").await.unwrap();
    let expected = parse_bundle(
        include_str!("fixtures/cert_rsa.pem"),
        include_str!("fixtures/cert_rsa.key"),
    )
    .unwrap();
    assert_eq!(secrets.cert_der, expected.cert_der);
}

/// Configuring a pull channel must show up immediately. A channel whose first
/// fetch fails used to leave the certificate list empty, which reads as "my
/// configuration was not saved".
#[tokio::test]
async fn a_pull_channel_creates_the_certificate_it_will_fill() {
    let (_dir, db) = database().await;

    let created = db
        .create_source(
            "default",
            rustfrp_client::db::cert_ingest::SourceInput {
                certificate_id: None,
                kind: rustfrp_client::db::cert_ingest::SourceKind::Certd,
                name: "certd".into(),
                domain: "*.powercess.com".into(),
                base_url: "https://certd.example.com".into(),
                domains: vec![],
                key_id: Some("kid".into()),
                key_secret: Some("secret".into()),
                enabled: true,
            },
        )
        .await
        .unwrap();

    // The channel is bound, and the certificate exists with no material yet.
    assert!(created.source.certificate_id.is_some());
    assert!(
        created.source.is_primary,
        "its only channel is the primary one"
    );

    let certificates = db.list_certificates("default").await.unwrap();
    assert_eq!(certificates.len(), 1, "the certificate is visible at once");
    assert_eq!(certificates[0].primary_domain, "*.powercess.com");
    assert!(certificates[0].material_id.is_none());
    assert_eq!(certificates[0].id, created.source.certificate_id.unwrap());

    // A second channel for the same domain reuses that identity.
    let second = db
        .create_source(
            "default",
            rustfrp_client::db::cert_ingest::SourceInput {
                certificate_id: None,
                kind: rustfrp_client::db::cert_ingest::SourceKind::Url,
                name: "fallback".into(),
                domain: "*.powercess.com".into(),
                base_url: "https://example.com/bundle.json".into(),
                domains: vec![],
                key_id: None,
                key_secret: None,
                enabled: true,
            },
        )
        .await
        .unwrap();
    assert_eq!(second.source.certificate_id, created.source.certificate_id);
    assert!(!second.source.is_primary, "the first channel stays primary");
    assert_eq!(db.list_certificates("default").await.unwrap().len(), 1);
}

/// A push channel's allowlist is not a domain it serves, so it must not invent a
/// certificate: the domain only becomes known when a bundle arrives.
#[tokio::test]
async fn a_push_channel_does_not_invent_a_certificate() {
    let (_dir, db) = database().await;
    let created = db
        .create_source(
            "default",
            rustfrp_client::db::cert_ingest::SourceInput {
                certificate_id: None,
                kind: rustfrp_client::db::cert_ingest::SourceKind::Push,
                name: "certd".into(),
                domain: String::new(),
                base_url: String::new(),
                domains: vec!["*.powercess.com".into()],
                key_id: None,
                key_secret: None,
                enabled: true,
            },
        )
        .await
        .unwrap();
    assert!(created.source.certificate_id.is_none());
    assert!(db.list_certificates("default").await.unwrap().is_empty());
}

/// An expired or pending certificate must not be written over a working file.
#[tokio::test]
async fn apply_refuses_a_certificate_that_cannot_be_served() {
    let (dir, db) = database().await;
    let certificate = db
        .ensure_certificate("default", "test.example.com")
        .await
        .unwrap();
    assert_eq!(
        certificate.status,
        rustfrp_client::cert::CertificateStatus::Pending
    );

    let crt_path = dir.path().join("fullchain.pem");
    let key_path = dir.path().join("privkey.pem");
    let proxy = proxy_with_certificate_paths(dir.path());
    let proxy_id = db.insert_proxy_for_tenant(&proxy, "default").await.unwrap();
    db.bind_certificate(certificate.id, proxy_id, "default")
        .await
        .unwrap();

    let error = db
        .apply_certificate(certificate.id, "default")
        .await
        .unwrap_err();
    assert!(error.to_string().contains("refusing to write"), "{error}");
    assert!(
        !crt_path.exists() && !key_path.exists(),
        "nothing may be written for a certificate that cannot be served"
    );
}
