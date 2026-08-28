//! 数据库增量迁移
//!
//! 迁移采用版本号递增策略。每个迁移是一个 `ALTER TABLE` / `CREATE TABLE IF NOT EXISTS`
//! 或数据迁移操作。启动时校验 schema checksum，不匹配则拒绝启动。

use crate::error::{ClientError, Result};
use rusqlite::Connection;
use sha2::{Digest, Sha256};

/// 当前最新的 schema 版本
const LATEST_VERSION: i32 = 14;

/// 运行所有待执行的迁移
///
/// 幂等：已执行的迁移不会重复执行。
/// 以事务方式执行每条迁移。
pub fn run(conn: &Connection) -> Result<()> {
    // 确保版本管理表存在
    ensure_meta_table(conn)?;

    let current_version = get_current_version(conn)?;

    tracing::info!(
        current_version,
        latest_version = LATEST_VERSION,
        "Start database migration"
    );

    if current_version >= LATEST_VERSION {
        tracing::info!("The database is already at the latest version, no migration needed.");
        verify_checksum(conn)?;
        return Ok(());
    }

    for version in (current_version + 1)..=LATEST_VERSION {
        apply_migration(conn, version)?;
    }

    // After successful migration, update the checksum to match the new schema.
    // Verification is only needed when NO migration ran (schema must be unchanged).
    update_checksum(conn)?;

    tracing::info!(
        from = current_version,
        to = LATEST_VERSION,
        "Migration completed"
    );
    Ok(())
}

/// 确保元数据表存在
fn ensure_meta_table(conn: &Connection) -> Result<()> {
    conn.execute(
        "CREATE TABLE IF NOT EXISTS _schema_meta (
            key   TEXT PRIMARY KEY,
            value TEXT NOT NULL
        )",
        [],
    )
    .map_err(|e| ClientError::DatabaseMigration(format!("Failed to create _schema_meta: {e}")))?;
    Ok(())
}

/// 获取当前 schema 版本
fn get_current_version(conn: &Connection) -> Result<i32> {
    let version: String = conn
        .query_row(
            "SELECT value FROM _schema_meta WHERE key = 'version'",
            [],
            |row| row.get(0),
        )
        .unwrap_or_else(|_| "0".to_string());

    version
        .parse::<i32>()
        .map_err(|_| ClientError::DatabaseMigration(format!("Invalid schema version: {version}")))
}

/// 应用指定版本的迁移
fn apply_migration(conn: &Connection, version: i32) -> Result<()> {
    let tx = conn
        .unchecked_transaction()
        .map_err(|e| ClientError::DatabaseMigration(format!("Failed to begin transaction: {e}")))?;

    match version {
        1 => migrate_v1(&tx)?,
        2 => migrate_v2(&tx)?,
        3 => migrate_v3(&tx)?,
        4 => migrate_v4(&tx)?,
        5 => migrate_v5(&tx)?,
        6 => migrate_v6(&tx)?,
        7 => migrate_v7(&tx)?,
        8 => migrate_v8(&tx)?,
        9 => migrate_v9(&tx)?,
        10 => migrate_v10(&tx)?,
        11 => migrate_v11(&tx)?,
        12 => migrate_v12(&tx)?,
        13 => migrate_v13(&tx)?,
        14 => migrate_v14(&tx)?,
        _ => {
            return Err(ClientError::DatabaseMigration(format!(
                "Unknown migration version: {version}"
            )));
        }
    }

    // 记录版本
    tx.execute(
        "INSERT OR REPLACE INTO _schema_meta (key, value) VALUES ('version', ?1)",
        [version.to_string()],
    )
    .map_err(|e| ClientError::DatabaseMigration(format!("Failed to update version number: {e}")))?;

    tx.commit()
        .map_err(|e| ClientError::DatabaseMigration(format!("Migration commit failed: {e}")))?;

    tracing::info!(version, "Migration finished");
    Ok(())
}

/// V1 迁移：创建三张核心表
///
/// 表字段 1:1 对应 FRP 官方 TOML 规范（ARCH-004）。
fn migrate_v1(tx: &rusqlite::Transaction) -> Result<()> {
    // === FrpsProfile：服务端连接配置 ===
    tx.execute(
        "CREATE TABLE IF NOT EXISTS frps_profile (
            id                 INTEGER PRIMARY KEY AUTOINCREMENT,
            name               TEXT NOT NULL,
            server_addr        TEXT NOT NULL,
            server_port        INTEGER NOT NULL DEFAULT 7000,
            token              TEXT NOT NULL DEFAULT '',
            tls_enable         INTEGER NOT NULL DEFAULT 0,
            tls_cert_file      TEXT DEFAULT NULL,
            tls_key_file       TEXT DEFAULT NULL,
            tls_trusted_ca_file TEXT DEFAULT NULL,
            transport_protocol TEXT NOT NULL DEFAULT 'tcp',
            heartbeat_interval INTEGER DEFAULT 30,
            heartbeat_timeout  INTEGER DEFAULT 90,
            created_at         TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at         TEXT NOT NULL DEFAULT (datetime('now'))
        )",
        [],
    )
    .map_err(|e| {
        ClientError::DatabaseMigration(format!("Failed to create frps_profile table: {e}"))
    })?;

    // === LocalProxy：本地代理配置 ===
    tx.execute(
        "CREATE TABLE IF NOT EXISTS local_proxy (
            id                 INTEGER PRIMARY KEY AUTOINCREMENT,
            name               TEXT NOT NULL,
            proxy_type         TEXT NOT NULL DEFAULT 'tcp',
            local_ip           TEXT NOT NULL DEFAULT '127.0.0.1',
            local_port         INTEGER NOT NULL,
            remote_port        INTEGER DEFAULT NULL,
            custom_domains     TEXT DEFAULT NULL,
            subdomain          TEXT DEFAULT NULL,
            use_encryption     INTEGER NOT NULL DEFAULT 1,
            use_compression    INTEGER NOT NULL DEFAULT 1,
            bandwidth_limit    TEXT DEFAULT NULL,
            health_check_type  TEXT DEFAULT NULL,
            health_check_timeout_s INTEGER DEFAULT 3,
            health_check_max_failed INTEGER DEFAULT 3,
            health_check_interval_s INTEGER DEFAULT 10,
            created_at         TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at         TEXT NOT NULL DEFAULT (datetime('now'))
        )",
        [],
    )
    .map_err(|e| {
        ClientError::DatabaseMigration(format!("Failed to create local_proxy table: {e}"))
    })?;

    // === BindingRule：多对多绑定规则 ===
    tx.execute(
        "CREATE TABLE IF NOT EXISTS binding_rule (
            id            INTEGER PRIMARY KEY AUTOINCREMENT,
            profile_id    INTEGER NOT NULL,
            proxy_id      INTEGER NOT NULL,
            enabled       INTEGER NOT NULL DEFAULT 1,
            priority      INTEGER NOT NULL DEFAULT 0,
            group_name    TEXT DEFAULT NULL,
            group_key     TEXT DEFAULT NULL,
            created_at    TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at    TEXT NOT NULL DEFAULT (datetime('now')),
            FOREIGN KEY (profile_id) REFERENCES frps_profile(id) ON DELETE CASCADE,
            FOREIGN KEY (proxy_id)   REFERENCES local_proxy(id)    ON DELETE CASCADE
        )",
        [],
    )
    .map_err(|e| {
        ClientError::DatabaseMigration(format!("Failed to create binding_rule table: {e}"))
    })?;

    // 创建索引
    tx.execute(
        "CREATE INDEX IF NOT EXISTS idx_binding_rule_profile ON binding_rule(profile_id)",
        [],
    )
    .ok();
    tx.execute(
        "CREATE INDEX IF NOT EXISTS idx_binding_rule_proxy ON binding_rule(proxy_id)",
        [],
    )
    .ok();
    tx.execute(
        "CREATE INDEX IF NOT EXISTS idx_binding_rule_enabled ON binding_rule(enabled)",
        [],
    )
    .ok();

    tracing::info!(
        "V1 Migration: Create three core tables frps_profile / local_proxy / binding_rule"
    );
    Ok(())
}

/// V2 迁移：为 local_proxy 添加 FRP 原生插件配置字段
fn migrate_v2(tx: &rusqlite::Transaction) -> Result<()> {
    tx.execute(
        "ALTER TABLE local_proxy ADD COLUMN plugin_config TEXT DEFAULT NULL",
        [],
    )
    .map_err(|e| {
        if e.to_string().contains("duplicate column") {
            tracing::info!("plugin_config column already exists, skipping");
        }
        ClientError::DatabaseMigration(format!("Failed to add plugin_config column: {e}"))
    })?;

    tracing::info!("V2 Migration: Add local_proxy.plugin_config column");
    Ok(())
}

/// V3 迁移：添加 health_check_path / health_check_http_headers / bandwidth_limit_mode 列
fn migrate_v3(tx: &rusqlite::Transaction) -> Result<()> {
    tx.execute(
        "ALTER TABLE local_proxy ADD COLUMN health_check_path TEXT DEFAULT NULL",
        [],
    )
    .map_err(|e| {
        if e.to_string().contains("duplicate column") {
            tracing::info!("health_check_path column already exists, skipping");
        }
        ClientError::DatabaseMigration(format!("Failed to add health_check_path column: {e}"))
    })?;

    tx.execute(
        "ALTER TABLE local_proxy ADD COLUMN health_check_http_headers TEXT DEFAULT NULL",
        [],
    )
    .map_err(|e| {
        if e.to_string().contains("duplicate column") {
            tracing::info!("health_check_http_headers column already exists, skipping");
        }
        ClientError::DatabaseMigration(format!(
            "Failed to add health_check_http_headers column: {e}"
        ))
    })?;

    tx.execute(
        "ALTER TABLE local_proxy ADD COLUMN bandwidth_limit_mode TEXT DEFAULT NULL",
        [],
    )
    .map_err(|e| {
        if e.to_string().contains("duplicate column") {
            tracing::info!("bandwidth_limit_mode column already exists, skipping");
        }
        ClientError::DatabaseMigration(format!("Failed to add bandwidth_limit_mode column: {e}"))
    })?;

    tracing::info!(
        "V3 Migration: Add local_proxy health_check_path / health_check_http_headers / bandwidth_limit_mode columns"
    );
    Ok(())
}

/// V4 迁移：添加 secret_key 列
fn migrate_v4(tx: &rusqlite::Transaction) -> Result<()> {
    tx.execute(
        "ALTER TABLE local_proxy ADD COLUMN secret_key TEXT DEFAULT NULL",
        [],
    )
    .map_err(|e| {
        if e.to_string().contains("duplicate column") {
            tracing::info!("secret_key column already exists, skipping");
        }
        ClientError::DatabaseMigration(format!("Failed to add secret_key column: {e}"))
    })?;

    tracing::info!("V4 Migration: Add local_proxy.secret_key column");
    Ok(())
}

/// V5 迁移：创建 local_visitor 表
fn migrate_v5(tx: &rusqlite::Transaction) -> Result<()> {
    tx.execute(
        "CREATE TABLE IF NOT EXISTS local_visitor (
            id                  INTEGER PRIMARY KEY AUTOINCREMENT,
            name                TEXT NOT NULL,
            visitor_type        TEXT NOT NULL DEFAULT 'stcp',
            server_name         TEXT NOT NULL,
            server_user         TEXT DEFAULT NULL,
            bind_addr           TEXT DEFAULT NULL,
            bind_port           INTEGER NOT NULL DEFAULT -1,
            secret_key          TEXT DEFAULT NULL,
            enabled             INTEGER NOT NULL DEFAULT 1,
            use_encryption      INTEGER NOT NULL DEFAULT 1,
            use_compression     INTEGER NOT NULL DEFAULT 1,
            xtcp_protocol       TEXT DEFAULT NULL,
            keep_tunnel_open    INTEGER DEFAULT NULL,
            max_retries_an_hour INTEGER DEFAULT NULL,
            min_retry_interval  INTEGER DEFAULT NULL,
            fallback_to         TEXT DEFAULT NULL,
            fallback_timeout_ms INTEGER DEFAULT NULL,
            plugin_config       TEXT DEFAULT NULL,
            profile_id          INTEGER NOT NULL,
            annotations         TEXT DEFAULT NULL,
            created_at          TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at          TEXT NOT NULL DEFAULT (datetime('now')),
            FOREIGN KEY (profile_id) REFERENCES frps_profile(id) ON DELETE CASCADE
        )",
        [],
    )
    .map_err(|e| {
        ClientError::DatabaseMigration(format!("Failed to create local_visitor table: {e}"))
    })?;

    tx.execute(
        "CREATE INDEX IF NOT EXISTS idx_local_visitor_profile ON local_visitor(profile_id)",
        [],
    )
    .ok();
    tx.execute(
        "CREATE INDEX IF NOT EXISTS idx_local_visitor_enabled ON local_visitor(enabled)",
        [],
    )
    .ok();

    tracing::info!("V5 Migration: Create local_visitor table");
    Ok(())
}

/// V6 迁移：添加 HTTP 高级特性字段
fn migrate_v6(tx: &rusqlite::Transaction) -> Result<()> {
    for col in &[
        "locations",
        "http_user",
        "http_password",
        "host_header_rewrite",
        "request_headers",
        "response_headers",
        "route_by_http_user",
    ] {
        let sql = format!("ALTER TABLE local_proxy ADD COLUMN {col} TEXT DEFAULT NULL");
        tx.execute(&sql, []).map_err(|e| {
            if e.to_string().contains("duplicate column") {
                tracing::info!("{col} column already exists, skipping");
            }
            ClientError::DatabaseMigration(format!("Failed to add {col} column: {e}"))
        })?;
    }

    tracing::info!("V6 Migration: Add local_proxy HTTP advanced feature columns");
    Ok(())
}

/// V7 迁移：添加传输层高级参数和 OIDC 认证字段
fn migrate_v7(tx: &rusqlite::Transaction) -> Result<()> {
    // Transport layer fields (integer type columns)
    for col in &[
        "dial_server_timeout",
        "dial_server_keepalive",
        "pool_count",
        "tcp_mux",
        "tcp_mux_keepalive_interval",
    ] {
        let sql = format!("ALTER TABLE frps_profile ADD COLUMN {col} INTEGER DEFAULT NULL");
        tx.execute(&sql, []).map_err(|e| {
            if e.to_string().contains("duplicate column") {
                tracing::info!("{col} column already exists, skipping");
            }
            ClientError::DatabaseMigration(format!("Failed to add {col} column: {e}"))
        })?;
    }

    // Transport layer fields (text type columns)
    for col in &["connect_server_local_ip", "proxy_url"] {
        let sql = format!("ALTER TABLE frps_profile ADD COLUMN {col} TEXT DEFAULT NULL");
        tx.execute(&sql, []).map_err(|e| {
            if e.to_string().contains("duplicate column") {
                tracing::info!("{col} column already exists, skipping");
            }
            ClientError::DatabaseMigration(format!("Failed to add {col} column: {e}"))
        })?;
    }

    // OIDC fields (text type columns)
    for col in &[
        "auth_method",
        "oidc_client_id",
        "oidc_client_secret",
        "oidc_token_endpoint_url",
        "oidc_audience",
        "oidc_scope",
        "oidc_additional_endpoint_params",
    ] {
        let sql = format!("ALTER TABLE frps_profile ADD COLUMN {col} TEXT DEFAULT NULL");
        tx.execute(&sql, []).map_err(|e| {
            if e.to_string().contains("duplicate column") {
                tracing::info!("{col} column already exists, skipping");
            }
            ClientError::DatabaseMigration(format!("Failed to add {col} column: {e}"))
        })?;
    }

    tracing::info!("V7 Migration: Add transport layer and OIDC auth columns to frps_profile");
    Ok(())
}

/// V8 迁移：添加 annotations/metadatas/user 以及 ClientCommonConfig 字段
fn migrate_v8(tx: &rusqlite::Transaction) -> Result<()> {
    // local_proxy: annotations + metadatas
    tx.execute(
        "ALTER TABLE local_proxy ADD COLUMN annotations TEXT DEFAULT NULL",
        [],
    )
    .map_err(|e| {
        if e.to_string().contains("duplicate column") {
            tracing::info!("annotations column already exists, skipping");
        }
        ClientError::DatabaseMigration(format!("Failed to add annotations column: {e}"))
    })?;
    tx.execute(
        "ALTER TABLE local_proxy ADD COLUMN metadatas TEXT DEFAULT NULL",
        [],
    )
    .map_err(|e| {
        if e.to_string().contains("duplicate column") {
            tracing::info!("metadatas column already exists, skipping");
        }
        ClientError::DatabaseMigration(format!("Failed to add metadatas column: {e}"))
    })?;

    // frps_profile: quic config fields
    for col in &[
        "quic_keepalive_period",
        "quic_max_idle_timeout",
        "quic_max_incoming_streams",
    ] {
        let sql = format!("ALTER TABLE frps_profile ADD COLUMN {col} INTEGER DEFAULT NULL");
        tx.execute(&sql, []).map_err(|e| {
            if e.to_string().contains("duplicate column") {
                tracing::info!("{col} column already exists, skipping");
            }
            ClientError::DatabaseMigration(format!("Failed to add {col} column: {e}"))
        })?;
    }

    // frps_profile: user + metadatas
    tx.execute(
        "ALTER TABLE frps_profile ADD COLUMN user TEXT DEFAULT NULL",
        [],
    )
    .map_err(|e| {
        if e.to_string().contains("duplicate column") {
            tracing::info!("user column already exists, skipping");
        }
        ClientError::DatabaseMigration(format!("Failed to add user column: {e}"))
    })?;
    tx.execute(
        "ALTER TABLE frps_profile ADD COLUMN metadatas TEXT DEFAULT NULL",
        [],
    )
    .map_err(|e| {
        if e.to_string().contains("duplicate column") {
            tracing::info!("metadatas column already exists, skipping");
        }
        ClientError::DatabaseMigration(format!("Failed to add metadatas column: {e}"))
    })?;

    // frps_profile: remaining common config fields
    for col in &[
        "login_fail_exit",
        "dns_server",
        "nat_hole_stun_server",
        "store_path",
    ] {
        let sql = format!("ALTER TABLE frps_profile ADD COLUMN {col} TEXT DEFAULT NULL");
        tx.execute(&sql, []).map_err(|e| {
            if e.to_string().contains("duplicate column") {
                tracing::info!("{col} column already exists, skipping");
            }
            ClientError::DatabaseMigration(format!("Failed to add {col} column: {e}"))
        })?;
    }

    // udp_packet_size is integer
    tx.execute(
        "ALTER TABLE frps_profile ADD COLUMN udp_packet_size INTEGER DEFAULT NULL",
        [],
    )
    .map_err(|e| {
        if e.to_string().contains("duplicate column") {
            tracing::info!("udp_packet_size column already exists, skipping");
        }
        ClientError::DatabaseMigration(format!("Failed to add udp_packet_size column: {e}"))
    })?;

    // start and includes are stored as comma-separated TEXT (Vec<String>)
    tx.execute(
        "ALTER TABLE frps_profile ADD COLUMN start TEXT DEFAULT NULL",
        [],
    )
    .map_err(|e| {
        if e.to_string().contains("duplicate column") {
            tracing::info!("start column already exists, skipping");
        }
        ClientError::DatabaseMigration(format!("Failed to add start column: {e}"))
    })?;
    tx.execute(
        "ALTER TABLE frps_profile ADD COLUMN includes TEXT DEFAULT NULL",
        [],
    )
    .map_err(|e| {
        if e.to_string().contains("duplicate column") {
            tracing::info!("includes column already exists, skipping");
        }
        ClientError::DatabaseMigration(format!("Failed to add includes column: {e}"))
    })?;

    tracing::info!("V8 Migration: Add annotations/metadatas/user and common config fields");
    Ok(())
}

/// V9 迁移：添加 allowUsers / natTraversal / proxyProtocolVersion / featureGates
fn migrate_v9(tx: &rusqlite::Transaction) -> Result<()> {
    // local_proxy: allow_users (comma-separated TEXT)
    tx.execute(
        "ALTER TABLE local_proxy ADD COLUMN allow_users TEXT DEFAULT NULL",
        [],
    )
    .map_err(|e| {
        if e.to_string().contains("duplicate column") {
            tracing::info!("allow_users already exists, skipping");
        }
        ClientError::DatabaseMigration(format!("Failed to add allow_users column: {e}"))
    })?;
    // local_proxy: nat_traversal_disable_assisted_addrs (bool as INTEGER)
    tx.execute(
        "ALTER TABLE local_proxy ADD COLUMN nat_traversal_disable_assisted_addrs INTEGER DEFAULT NULL",
        [],
    )
    .map_err(|e| {
        if e.to_string().contains("duplicate column") { tracing::info!("nat_traversal_disable_assisted_addrs already exists, skipping"); }
        ClientError::DatabaseMigration(format!("Failed to add nat_traversal_disable_assisted_addrs column: {e}"))
    })?;
    // local_proxy: proxy_protocol_version
    tx.execute(
        "ALTER TABLE local_proxy ADD COLUMN proxy_protocol_version TEXT DEFAULT NULL",
        [],
    )
    .map_err(|e| {
        if e.to_string().contains("duplicate column") {
            tracing::info!("proxy_protocol_version already exists, skipping");
        }
        ClientError::DatabaseMigration(format!("Failed to add proxy_protocol_version column: {e}"))
    })?;
    // frps_profile: feature_gates (JSON TEXT)
    tx.execute(
        "ALTER TABLE frps_profile ADD COLUMN feature_gates TEXT DEFAULT NULL",
        [],
    )
    .map_err(|e| {
        if e.to_string().contains("duplicate column") {
            tracing::info!("feature_gates already exists, skipping");
        }
        ClientError::DatabaseMigration(format!("Failed to add feature_gates column: {e}"))
    })?;

    tracing::info!(
        "V9 Migration: Add allowUsers / natTraversal / proxyProtocolVersion / featureGates columns"
    );
    Ok(())
}

/// V10 迁移：为 binding_rule 添加 running 列
///
/// `running` 与 `enabled` 正交：
/// - `enabled` = 配置已完成，允许被启动（资格）
/// - `running` = 代理正在 frpc 进程中运行（事实）
///
/// 存量数据：已有 binding 默认 running=0（Standby 状态），需用户手动启动。
fn migrate_v10(tx: &rusqlite::Transaction) -> Result<()> {
    tx.execute(
        "ALTER TABLE binding_rule ADD COLUMN running INTEGER NOT NULL DEFAULT 0",
        [],
    )
    .map_err(|e| {
        if e.to_string().contains("duplicate column") {
            tracing::info!("running column already exists, skipping");
        }
        ClientError::DatabaseMigration(format!("Failed to add running column: {e}"))
    })?;

    tracing::info!("V10 Migration: Add binding_rule.running column");
    Ok(())
}

/// V11: deployment environments remain separate from the FRP TOML model.
fn migrate_v11(tx: &rusqlite::Transaction) -> Result<()> {
    tx.execute_batch(
        "CREATE TABLE IF NOT EXISTS environment (
            id          INTEGER PRIMARY KEY AUTOINCREMENT,
            name        TEXT NOT NULL UNIQUE,
            description TEXT,
            color       TEXT NOT NULL DEFAULT '#18a058',
            is_default  INTEGER NOT NULL DEFAULT 0,
            created_at  TEXT NOT NULL,
            updated_at  TEXT NOT NULL
        );
        CREATE UNIQUE INDEX IF NOT EXISTS idx_environment_single_default
            ON environment(is_default) WHERE is_default = 1;
        CREATE TABLE IF NOT EXISTS profile_environment (
            profile_id    INTEGER PRIMARY KEY,
            environment_id INTEGER NOT NULL,
            FOREIGN KEY (profile_id) REFERENCES frps_profile(id) ON DELETE CASCADE,
            FOREIGN KEY (environment_id) REFERENCES environment(id) ON DELETE RESTRICT
        );
        CREATE INDEX IF NOT EXISTS idx_profile_environment_environment
            ON profile_environment(environment_id);",
    )
    .map_err(|e| ClientError::DatabaseMigration(format!("Failed to create environments: {e}")))?;

    let now = chrono::Utc::now().to_rfc3339();
    tx.execute(
        "INSERT OR IGNORE INTO environment
            (id, name, description, color, is_default, created_at, updated_at)
         VALUES (1, 'Default', 'Default deployment environment', '#18a058', 1, ?1, ?1)",
        [&now],
    )
    .map_err(|e| {
        ClientError::DatabaseMigration(format!("Failed to seed default environment: {e}"))
    })?;
    tx.execute(
        "INSERT OR IGNORE INTO profile_environment (profile_id, environment_id)
         SELECT id, 1 FROM frps_profile",
        [],
    )
    .map_err(|e| {
        ClientError::DatabaseMigration(format!("Failed to assign existing profiles: {e}"))
    })?;
    tracing::info!("V11 Migration: Add deployment environments");
    Ok(())
}

/// V12: make deployment environments the isolation boundary for API tenants.
fn migrate_v12(tx: &rusqlite::Transaction) -> Result<()> {
    tx.execute(
        "ALTER TABLE environment ADD COLUMN tenant_id TEXT NOT NULL DEFAULT 'default'",
        [],
    )?;
    tx.execute("DROP INDEX IF EXISTS idx_environment_single_default", [])?;
    tx.execute_batch(
        "CREATE UNIQUE INDEX idx_environment_tenant_default
           ON environment(tenant_id) WHERE is_default = 1;
         CREATE INDEX idx_environment_tenant ON environment(tenant_id);",
    )?;
    Ok(())
}

/// V13: move desired runtime state to the Profile execution unit and prevent
/// duplicate Profile/Proxy memberships.
fn migrate_v13(tx: &rusqlite::Transaction) -> Result<()> {
    tx.execute_batch(
        "CREATE TABLE IF NOT EXISTS profile_runtime (
            profile_id      INTEGER PRIMARY KEY,
            desired_running INTEGER NOT NULL DEFAULT 0,
            updated_at      TEXT NOT NULL DEFAULT (datetime('now')),
            FOREIGN KEY (profile_id) REFERENCES frps_profile(id) ON DELETE CASCADE
        );
        INSERT OR IGNORE INTO profile_runtime (profile_id, desired_running)
        SELECT profile_id, MAX(running) FROM binding_rule GROUP BY profile_id;
        DELETE FROM binding_rule
         WHERE id NOT IN (
            SELECT MIN(id) FROM binding_rule GROUP BY profile_id, proxy_id
         );
        CREATE UNIQUE INDEX IF NOT EXISTS uq_binding_profile_proxy
            ON binding_rule(profile_id, proxy_id);",
    )
    .map_err(|e| ClientError::DatabaseMigration(format!("Failed to add profile runtime: {e}")))?;
    tracing::info!("V13 Migration: Add profile runtime and unique binding membership");
    Ok(())
}

/// V14: persist whether a running Profile is using an outdated generated
/// configuration after its proxy membership changes.
fn migrate_v14(tx: &rusqlite::Transaction) -> Result<()> {
    tx.execute(
        "ALTER TABLE profile_runtime ADD COLUMN config_pending INTEGER NOT NULL DEFAULT 0",
        [],
    )
    .map_err(|e| {
        ClientError::DatabaseMigration(format!("Failed to add profile runtime pending state: {e}"))
    })?;
    tracing::info!("V14 Migration: Add profile runtime pending configuration state");
    Ok(())
}

/// 校验当前 schema checksum
///
/// 计算所有表结构的 SHA256，与存储的 checksum 比对。
/// 不匹配则拒绝启动，防止手动修改数据库导致的不一致。
pub fn verify_checksum(conn: &Connection) -> Result<()> {
    let schema_hash = compute_schema_hash(conn)?;

    let stored: Option<String> = conn
        .query_row(
            "SELECT value FROM _schema_meta WHERE key = 'checksum'",
            [],
            |row| row.get(0),
        )
        .ok();

    match stored {
        Some(stored_hash) if stored_hash != schema_hash => {
            Err(ClientError::DatabaseMigration(format!(
                "Schema checksum mismatch! Stored: {}, Current: {}. Do not manually modify the database schema.",
                &stored_hash[..16],
                &schema_hash[..16]
            )))
        }
        None => {
            // 首次运行，记录 checksum
            conn.execute(
                "INSERT OR REPLACE INTO _schema_meta (key, value) VALUES ('checksum', ?1)",
                [&schema_hash],
            )
            .map_err(|e| ClientError::DatabaseMigration(format!("Failed to store checksum: {e}")))?;
            tracing::info!(checksum = %&schema_hash[..16], "Schema checksum recorded");
            Ok(())
        }
        _ => Ok(()),
    }
}

/// 更新存储的 schema checksum（迁移成功后调用）
fn update_checksum(conn: &Connection) -> Result<()> {
    let schema_hash = compute_schema_hash(conn)?;
    conn.execute(
        "INSERT OR REPLACE INTO _schema_meta (key, value) VALUES ('checksum', ?1)",
        [&schema_hash],
    )
    .map_err(|e| ClientError::DatabaseMigration(format!("Failed to update checksum: {e}")))?;
    tracing::info!(checksum = %&schema_hash[..16], "Schema checksum updated");
    Ok(())
}

/// 计算当前数据库 schema 的 SHA256 哈希
fn compute_schema_hash(conn: &Connection) -> Result<String> {
    let mut stmt = conn
        .prepare(
            "SELECT sql FROM sqlite_master
             WHERE type IN ('table', 'index', 'trigger')
             AND name NOT LIKE 'sqlite_%'
             AND name NOT LIKE '_schema_meta%'
             ORDER BY name, type",
        )
        .map_err(|e| ClientError::DatabaseMigration(format!("Failed to read schema: {e}")))?;

    let schemas: Vec<String> = stmt
        .query_map([], |row| row.get(0))?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(ClientError::DatabaseQuery)?;

    let concatenated = schemas.join("\n");
    let mut hasher = Sha256::new();
    hasher.update(concatenated.as_bytes());
    let result = hasher.finalize();

    Ok(format!("{:x}", result))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_migration_is_idempotent() {
        let conn = Connection::open_in_memory().unwrap();
        // 第一次运行
        run(&conn).unwrap();
        // 第二次运行（幂等）
        run(&conn).unwrap();

        // 验证三表存在
        let table_count: i32 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table'
                 AND name IN ('frps_profile', 'local_proxy', 'binding_rule')",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(table_count, 3);
    }

    #[test]
    fn test_schema_hash_detects_changes() {
        let conn = Connection::open_in_memory().unwrap();
        run(&conn).unwrap();

        // 手动修改 schema（模拟未被追踪的修改）
        conn.execute("CREATE TABLE IF NOT EXISTS hacker_table (id INTEGER)", [])
            .unwrap();

        // checksum 验证应该失败
        let result = verify_checksum(&conn);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("checksum mismatch"));
    }
}
