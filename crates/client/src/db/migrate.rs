//! 数据库增量迁移
//!
//! 迁移采用版本号递增策略。每个迁移是一个 `ALTER TABLE` / `CREATE TABLE IF NOT EXISTS`
//! 或数据迁移操作。启动时校验 schema checksum，不匹配则拒绝启动。

use crate::error::{ClientError, Result};
use rusqlite::Connection;
use sha2::{Digest, Sha256};

/// 当前最新的 schema 版本
const LATEST_VERSION: i32 = 24;

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
        15 => migrate_v15(&tx)?,
        16 => migrate_v16(&tx)?,
        17 => migrate_v17(&tx)?,
        18 => migrate_v18(&tx)?,
        19 => migrate_v19(&tx)?,
        20 => migrate_v20(&tx)?,
        21 => migrate_v21(&tx)?,
        22 => migrate_v22(&tx)?,
        23 => migrate_v23(&tx)?,
        24 => migrate_v24(&tx)?,
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

// Runtime policy belongs to profile_runtime, never to generated FRP TOML.
fn migrate_v15(tx: &rusqlite::Transaction) -> Result<()> {
    tx.execute(
        "ALTER TABLE profile_runtime ADD COLUMN frpc_version TEXT",
        [],
    )
    .map_err(|e| ClientError::DatabaseMigration(format!("Failed to add runtime version: {e}")))?;
    Ok(())
}

/// V16: persist the per-profile frpc admin port and the tcpmux multiplexer.
///
/// The admin port enables `frpc reload` / `frpc status` (the only documented
/// hot-reload path); `multiplexer` is required for valid tcpmux proxies.
fn migrate_v16(tx: &rusqlite::Transaction) -> Result<()> {
    for statement in [
        "ALTER TABLE local_proxy ADD COLUMN multiplexer TEXT DEFAULT NULL",
        "ALTER TABLE profile_runtime ADD COLUMN admin_port INTEGER DEFAULT NULL",
        "ALTER TABLE profile_runtime ADD COLUMN admin_password TEXT DEFAULT NULL",
    ] {
        if let Err(error) = tx.execute(statement, []) {
            if !error.to_string().contains("duplicate column") {
                return Err(ClientError::DatabaseMigration(format!(
                    "Failed to apply V16 migration: {error}"
                )));
            }
            tracing::info!("V16 column already exists, skipping: {statement}");
        }
    }
    tracing::info!("V16 Migration: Add tcpmux multiplexer and frpc admin port");
    Ok(())
}

/// V17: remaining frpc common-config fields that were modelled but never
/// persisted, so they could not survive a restart.
fn migrate_v17(tx: &rusqlite::Transaction) -> Result<()> {
    let columns = [
        ("client_id", "TEXT"),
        ("wire_protocol", "TEXT"),
        ("auth_additional_scopes", "TEXT"),
        ("oidc_additional_params", "TEXT"),
        ("oidc_trusted_ca_file", "TEXT"),
        ("oidc_proxy_url", "TEXT"),
        ("tls_server_name", "TEXT"),
        ("log_to", "TEXT"),
        ("log_level", "TEXT"),
        ("log_max_days", "INTEGER"),
        ("virtual_net_address", "TEXT"),
    ];
    for (column, kind) in columns {
        match tx.execute(
            &format!("ALTER TABLE frps_profile ADD COLUMN {column} {kind} DEFAULT NULL"),
            [],
        ) {
            Ok(_) => {}
            Err(error) if error.to_string().contains("duplicate column") => {
                tracing::info!(column, "V17 column already exists, skipping");
            }
            Err(error) => {
                return Err(ClientError::DatabaseMigration(format!(
                    "Failed to add {column} column: {error}"
                )));
            }
        }
    }
    for column in ["oidc_insecure_skip_verify", "tls_disable_custom_first_byte"] {
        match tx.execute(
            &format!("ALTER TABLE frps_profile ADD COLUMN {column} INTEGER DEFAULT NULL"),
            [],
        ) {
            Ok(_) => {}
            Err(error) if error.to_string().contains("duplicate column") => {
                tracing::info!(column, "V17 column already exists, skipping");
            }
            Err(error) => {
                return Err(ClientError::DatabaseMigration(format!(
                    "Failed to add {column} column: {error}"
                )));
            }
        }
    }
    tracing::info!("V17 Migration: Add remaining frpc common config columns");
    Ok(())
}

/// V18: profile names must be unique.
///
/// Generated artifacts are keyed by profile id, so duplicates no longer corrupt
/// each other's configuration, but two identically named profiles remain
/// indistinguishable in the UI and in logs. Existing duplicates are renamed with
/// a numeric suffix, keeping the oldest row untouched.
fn migrate_v18(tx: &rusqlite::Transaction) -> Result<()> {
    let duplicates: Vec<String> = {
        let mut statement = tx
            .prepare("SELECT name FROM frps_profile GROUP BY name HAVING COUNT(*) > 1")
            .map_err(|e| {
                ClientError::DatabaseMigration(format!("Failed to scan duplicate profiles: {e}"))
            })?;
        let rows = statement
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(|e| {
                ClientError::DatabaseMigration(format!("Failed to scan duplicate profiles: {e}"))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|e| {
                ClientError::DatabaseMigration(format!("Failed to scan duplicate profiles: {e}"))
            })?;
        rows
    };

    for name in duplicates {
        let ids: Vec<i64> = {
            let mut statement = tx
                .prepare("SELECT id FROM frps_profile WHERE name = ?1 ORDER BY id")
                .map_err(|e| {
                    ClientError::DatabaseMigration(format!("Failed to load duplicate rows: {e}"))
                })?;
            let rows = statement
                .query_map([&name], |row| row.get::<_, i64>(0))
                .map_err(|e| {
                    ClientError::DatabaseMigration(format!("Failed to load duplicate rows: {e}"))
                })?
                .collect::<std::result::Result<Vec<_>, _>>()
                .map_err(|e| {
                    ClientError::DatabaseMigration(format!("Failed to load duplicate rows: {e}"))
                })?;
            rows
        };
        for (index, id) in ids.into_iter().skip(1).enumerate() {
            let renamed = format!("{name}-{}", index + 2);
            tx.execute(
                "UPDATE frps_profile SET name = ?1 WHERE id = ?2",
                rusqlite::params![renamed, id],
            )
            .map_err(|e| {
                ClientError::DatabaseMigration(format!("Failed to rename duplicate profile: {e}"))
            })?;
        }
        tracing::info!(name, "V18 renamed duplicate profile names");
    }

    tx.execute(
        "CREATE UNIQUE INDEX IF NOT EXISTS uq_frps_profile_name ON frps_profile(name)",
        [],
    )
    .map_err(|e| {
        ClientError::DatabaseMigration(format!("Failed to add profile name uniqueness: {e}"))
    })?;
    tracing::info!("V18 Migration: Enforce unique profile names");
    Ok(())
}

/// V19: RustFRP runs a single fixed frp version, so the per-profile pin is gone.
///
/// The column is dropped rather than left unused: a stale per-profile version
/// would suggest a capability the daemon no longer has.
fn migrate_v19(tx: &rusqlite::Transaction) -> Result<()> {
    if let Err(error) = tx.execute("ALTER TABLE profile_runtime DROP COLUMN frpc_version", []) {
        // SQLite older than 3.35 cannot drop columns; clearing the values keeps
        // the stale policy from being read.
        if error.to_string().contains("syntax") || error.to_string().contains("near \"DROP\"") {
            tx.execute("UPDATE profile_runtime SET frpc_version = NULL", [])
                .map_err(|e| {
                    ClientError::DatabaseMigration(format!(
                        "Failed to clear per-profile frp version: {e}"
                    ))
                })?;
            tracing::warn!("V19: SQLite cannot drop columns; cleared frpc_version instead");
        } else {
            return Err(ClientError::DatabaseMigration(format!(
                "Failed to drop per-profile frp version: {error}"
            )));
        }
    }
    tracing::info!("V19 Migration: Remove per-profile frp version pinning");
    Ok(())
}

/// V20: give proxies and server configurations a tenant, so API tokens issued for
/// different tenants cannot see each other's data.
///
/// Membership used to be derived through `environment.tenant_id`, which forced
/// every handler to repeat a two-hop join and let several handlers skip the check
/// entirely. The tenant now lives on the row itself; visitors and bindings reach
/// it through their `profile_id`, which already exists.
///
/// Existing rows inherit the tenant of the environment their profile belongs to,
/// falling back to `default` for unassigned profiles and unbound proxies.
fn migrate_v20(tx: &rusqlite::Transaction) -> Result<()> {
    for statement in [
        "ALTER TABLE frps_profile ADD COLUMN tenant_id TEXT NOT NULL DEFAULT 'default'",
        "ALTER TABLE local_proxy ADD COLUMN tenant_id TEXT NOT NULL DEFAULT 'default'",
    ] {
        if let Err(error) = tx.execute(statement, []) {
            if !error.to_string().contains("duplicate column") {
                return Err(ClientError::DatabaseMigration(format!(
                    "Failed to add tenant column: {error}"
                )));
            }
            tracing::info!("V20 column already exists, skipping: {statement}");
        }
    }

    tx.execute(
        "UPDATE frps_profile
            SET tenant_id = COALESCE((
                SELECT e.tenant_id
                  FROM profile_environment pe
                  JOIN environment e ON e.id = pe.environment_id
                 WHERE pe.profile_id = frps_profile.id
            ), 'default')",
        [],
    )
    .map_err(|e| {
        ClientError::DatabaseMigration(format!("Failed to backfill profile tenants: {e}"))
    })?;

    tx.execute(
        "UPDATE local_proxy
            SET tenant_id = COALESCE((
                SELECT p.tenant_id
                  FROM binding_rule b
                  JOIN frps_profile p ON p.id = b.profile_id
                 WHERE b.proxy_id = local_proxy.id
                 LIMIT 1
            ), 'default')",
        [],
    )
    .map_err(|e| {
        ClientError::DatabaseMigration(format!("Failed to backfill proxy tenants: {e}"))
    })?;

    tx.execute(
        "CREATE INDEX IF NOT EXISTS idx_frps_profile_tenant ON frps_profile(tenant_id)",
        [],
    )
    .ok();
    tx.execute(
        "CREATE INDEX IF NOT EXISTS idx_local_proxy_tenant ON local_proxy(tenant_id)",
        [],
    )
    .ok();
    tracing::info!("V20 Migration: Add tenant ownership to profiles and proxies");
    Ok(())
}

/// V21: certificate management.
///
/// These tables belong to the platform, not to FRP: nothing here is serialized
/// into a generated TOML. The only FRP-facing effect is that an apply writes the
/// file named by a proxy's `plugin.crtPath` / `keyPath`, which are FRP-native
/// fields (ARCH-004 / CFG-001).
///
/// Certificate bodies and keys are stored as XChaCha20-Poly1305 blobs, not as
/// PEM text, so the database never contains a readable private key.
fn migrate_v21(tx: &rusqlite::Transaction) -> Result<()> {
    tx.execute(
        "CREATE TABLE IF NOT EXISTS certificate (
            id                 INTEGER PRIMARY KEY AUTOINCREMENT,
            tenant_id          TEXT    NOT NULL DEFAULT 'default',
            primary_domain     TEXT    NOT NULL,
            sans_json          TEXT    NOT NULL DEFAULT '[]',
            issuer             TEXT    NOT NULL DEFAULT '',
            serial             TEXT    NOT NULL DEFAULT '',
            fingerprint_sha256 TEXT    NOT NULL,
            key_algo           TEXT    NOT NULL DEFAULT '',
            not_before         TEXT    NOT NULL,
            not_after          TEXT    NOT NULL,
            source             TEXT    NOT NULL,
            source_detail      TEXT,
            enc_cert_der       BLOB    NOT NULL,
            enc_chain_der      BLOB,
            enc_key_der        BLOB    NOT NULL,
            created_at         TEXT    NOT NULL,
            updated_at         TEXT    NOT NULL
        )",
        [],
    )
    .map_err(|e| ClientError::DatabaseMigration(format!("Failed to create certificate: {e}")))?;

    tx.execute(
        "CREATE UNIQUE INDEX IF NOT EXISTS idx_certificate_fingerprint
            ON certificate (tenant_id, fingerprint_sha256)",
        [],
    )
    .map_err(|e| {
        ClientError::DatabaseMigration(format!("Failed to index certificate fingerprint: {e}"))
    })?;

    tx.execute(
        "CREATE INDEX IF NOT EXISTS idx_certificate_expiry ON certificate (tenant_id, not_after)",
        [],
    )
    .ok();

    tx.execute(
        "CREATE TABLE IF NOT EXISTS certificate_binding (
            id                  INTEGER PRIMARY KEY AUTOINCREMENT,
            tenant_id           TEXT    NOT NULL DEFAULT 'default',
            cert_id             INTEGER NOT NULL,
            proxy_id            INTEGER NOT NULL,
            applied_fingerprint TEXT,
            applied_at          TEXT,
            last_error          TEXT,
            created_at          TEXT    NOT NULL,
            updated_at          TEXT    NOT NULL,
            FOREIGN KEY (cert_id) REFERENCES certificate(id) ON DELETE RESTRICT,
            FOREIGN KEY (proxy_id) REFERENCES local_proxy(id) ON DELETE CASCADE
        )",
        [],
    )
    .map_err(|e| {
        ClientError::DatabaseMigration(format!("Failed to create certificate_binding: {e}"))
    })?;

    // One proxy holds one certificate: a second certificate for the same proxy
    // is an import mistake, not a configuration to reconcile.
    tx.execute(
        "CREATE UNIQUE INDEX IF NOT EXISTS idx_certificate_binding_proxy
            ON certificate_binding (proxy_id)",
        [],
    )
    .map_err(|e| {
        ClientError::DatabaseMigration(format!("Failed to index certificate bindings: {e}"))
    })?;

    tx.execute(
        "CREATE TABLE IF NOT EXISTS cert_ingest_token (
            id           INTEGER PRIMARY KEY AUTOINCREMENT,
            tenant_id    TEXT    NOT NULL DEFAULT 'default',
            name         TEXT    NOT NULL,
            token_sha256 TEXT    NOT NULL,
            domains_json TEXT    NOT NULL DEFAULT '[]',
            expires_at   TEXT,
            revoked_at   TEXT,
            last_used_at TEXT,
            use_count    INTEGER NOT NULL DEFAULT 0,
            created_at   TEXT    NOT NULL
        )",
        [],
    )
    .map_err(|e| {
        ClientError::DatabaseMigration(format!("Failed to create cert_ingest_token: {e}"))
    })?;

    tx.execute(
        "CREATE UNIQUE INDEX IF NOT EXISTS idx_cert_ingest_token_sha
            ON cert_ingest_token (token_sha256)",
        [],
    )
    .map_err(|e| {
        ClientError::DatabaseMigration(format!("Failed to index ingest token digest: {e}"))
    })?;

    tx.execute(
        "CREATE TABLE IF NOT EXISTS cert_pull_source (
            id           INTEGER PRIMARY KEY AUTOINCREMENT,
            tenant_id    TEXT    NOT NULL DEFAULT 'default',
            name         TEXT    NOT NULL,
            kind         TEXT    NOT NULL,
            base_url     TEXT    NOT NULL,
            domain       TEXT    NOT NULL,
            enc_secret   BLOB,
            enabled      INTEGER NOT NULL DEFAULT 1,
            last_run_at  TEXT,
            last_status  TEXT,
            last_error   TEXT,
            created_at   TEXT    NOT NULL,
            updated_at   TEXT    NOT NULL
        )",
        [],
    )
    .map_err(|e| {
        ClientError::DatabaseMigration(format!("Failed to create cert_pull_source: {e}"))
    })?;

    tracing::info!("V21 Migration: Add certificate management tables");
    Ok(())
}

/// V22: platform settings that an operator changes at runtime.
///
/// A key/value table rather than a column per setting: these are switches read by
/// long-lived tasks (the ingest listener), and adding another one must not mean
/// another migration.
fn migrate_v22(tx: &rusqlite::Transaction) -> Result<()> {
    tx.execute(
        "CREATE TABLE IF NOT EXISTS platform_setting (
            key        TEXT PRIMARY KEY,
            value      TEXT NOT NULL,
            updated_at TEXT NOT NULL
        )",
        [],
    )
    .map_err(|e| {
        ClientError::DatabaseMigration(format!("Failed to create platform_setting: {e}"))
    })?;

    tracing::info!("V22 Migration: Add platform settings");
    Ok(())
}

/// V23: certificate identity by domain, material split out, channels unified.
///
/// The v21 shape made the fingerprint the identity of a certificate, which is
/// wrong for renewal: every reissue is a new fingerprint, so "replace the
/// certificate for example.com" created a new row instead of updating one. This
/// migration makes `(tenant_id, primary_domain)` the identity and moves the
/// material — fingerprint, validity, encrypted bodies — into
/// `certificate_material`, one row per issued bundle.
///
/// Channels (push token, Certd pull, plain URL pull) become rows of
/// `certificate_source` attached to a certificate, so a certificate can exist
/// before it has any material and its channels can change without re-importing.
/// Legacy rows from `cert_ingest_token` / `cert_pull_source` are carried over
/// with `certificate_id = NULL`, which keeps them working as unbound channels.
fn migrate_v23(tx: &rusqlite::Transaction) -> Result<()> {
    // The rebuild below deletes rows that `certificate_binding` and
    // `certificate_material` point at, so FK checks must be deferred to commit.
    tx.execute_batch("PRAGMA defer_foreign_keys = ON")
        .map_err(|e| ClientError::DatabaseMigration(format!("Failed to defer foreign keys: {e}")))?;

    // --- 1. Material moves to its own table, one row per issued bundle. ------
    tx.execute_batch(
        "CREATE TABLE IF NOT EXISTS certificate_material (
            id                 INTEGER PRIMARY KEY AUTOINCREMENT,
            tenant_id          TEXT    NOT NULL DEFAULT 'default',
            certificate_id     INTEGER NOT NULL,
            fingerprint_sha256 TEXT    NOT NULL,
            sans_json          TEXT    NOT NULL DEFAULT '[]',
            issuer             TEXT    NOT NULL DEFAULT '',
            serial             TEXT    NOT NULL DEFAULT '',
            key_algo           TEXT    NOT NULL DEFAULT '',
            not_before         TEXT    NOT NULL,
            not_after          TEXT    NOT NULL,
            source             TEXT    NOT NULL DEFAULT '',
            source_detail      TEXT,
            enc_cert_der       BLOB    NOT NULL,
            enc_chain_der      BLOB,
            enc_key_der        BLOB    NOT NULL,
            created_at         TEXT    NOT NULL,
            updated_at         TEXT    NOT NULL,
            FOREIGN KEY (certificate_id) REFERENCES certificate(id) ON DELETE CASCADE
        )",
    )
    .map_err(|e| ClientError::DatabaseMigration(format!("Failed to create certificate_material: {e}")))?;

    tx.execute(
        "INSERT INTO certificate_material
            (tenant_id, certificate_id, fingerprint_sha256, sans_json, issuer, serial, key_algo,
             not_before, not_after, source, source_detail,
             enc_cert_der, enc_chain_der, enc_key_der, created_at, updated_at)
         SELECT tenant_id, id, fingerprint_sha256, sans_json, issuer, serial, key_algo,
                not_before, not_after, source, source_detail,
                enc_cert_der, enc_chain_der, enc_key_der, created_at, updated_at
           FROM certificate",
        [],
    )
    .map_err(|e| ClientError::DatabaseMigration(format!("Failed to copy certificate material: {e}")))?;

    // --- 2. Collapse rows that share a domain. ------------------------------
    // A renewal used to append a row; here they become one identity with several
    // material versions, and the newest is what the UI reports.
    tx.execute_batch(
        "CREATE TABLE _cert_dedupe (old_id INTEGER PRIMARY KEY, keep_id INTEGER NOT NULL);
         INSERT INTO _cert_dedupe
         SELECT id, MIN(id) OVER (PARTITION BY tenant_id, primary_domain) FROM certificate;",
    )
    .map_err(|e| ClientError::DatabaseMigration(format!("Failed to map duplicate certificates: {e}")))?;

    tx.execute(
        "UPDATE certificate_material SET certificate_id =
            (SELECT keep_id FROM _cert_dedupe WHERE old_id = certificate_material.certificate_id)",
        [],
    )
    .map_err(|e| ClientError::DatabaseMigration(format!("Failed to repoint material: {e}")))?;

    tx.execute(
        "UPDATE certificate_binding SET cert_id =
            (SELECT keep_id FROM _cert_dedupe WHERE old_id = certificate_binding.cert_id)",
        [],
    )
    .map_err(|e| ClientError::DatabaseMigration(format!("Failed to repoint bindings: {e}")))?;

    tx.execute(
        "DELETE FROM certificate WHERE id NOT IN (SELECT keep_id FROM _cert_dedupe)",
        [],
    )
    .map_err(|e| ClientError::DatabaseMigration(format!("Failed to collapse certificate rows: {e}")))?;

    tx.execute_batch("DROP TABLE _cert_dedupe")
        .map_err(|e| ClientError::DatabaseMigration(format!("Failed to drop the dedupe map: {e}")))?;

    // --- 3. `certificate` keeps identity only. -----------------------------
    // The fingerprint and expiry indexes must go before their columns can be
    // dropped; SQLite refuses a DROP COLUMN that an index still references.
    tx.execute_batch(
        "DROP INDEX IF EXISTS idx_certificate_fingerprint;
         DROP INDEX IF EXISTS idx_certificate_expiry;",
    )
    .map_err(|e| ClientError::DatabaseMigration(format!("Failed to drop certificate indexes: {e}")))?;

    for column in [
        "fingerprint_sha256",
        "sans_json",
        "issuer",
        "serial",
        "key_algo",
        "not_before",
        "not_after",
        "source",
        "source_detail",
        "enc_cert_der",
        "enc_chain_der",
        "enc_key_der",
    ] {
        tx.execute(&format!("ALTER TABLE certificate DROP COLUMN {column}"), [])
            .map_err(|e| {
                ClientError::DatabaseMigration(format!("Failed to drop certificate.{column}: {e}"))
            })?;
    }

    tx.execute_batch(
        "CREATE UNIQUE INDEX IF NOT EXISTS idx_certificate_domain
            ON certificate (tenant_id, primary_domain);
         CREATE UNIQUE INDEX IF NOT EXISTS idx_certificate_material_fingerprint
            ON certificate_material (certificate_id, fingerprint_sha256);
         CREATE INDEX IF NOT EXISTS idx_certificate_material_recent
            ON certificate_material (certificate_id, not_after);",
    )
    .map_err(|e| ClientError::DatabaseMigration(format!("Failed to index certificates: {e}")))?;

    // --- 4. One table for every way a certificate gets updated. -------------
    tx.execute_batch(
        "CREATE TABLE IF NOT EXISTS certificate_source (
            id             INTEGER PRIMARY KEY AUTOINCREMENT,
            tenant_id      TEXT    NOT NULL DEFAULT 'default',
            certificate_id INTEGER,
            kind           TEXT    NOT NULL,
            name           TEXT    NOT NULL,
            domain         TEXT    NOT NULL DEFAULT '',
            base_url       TEXT    NOT NULL DEFAULT '',
            enc_secret     BLOB,
            domains_json   TEXT    NOT NULL DEFAULT '[]',
            token_sha256   TEXT,
            expires_at     TEXT,
            revoked_at     TEXT,
            last_used_at   TEXT,
            use_count      INTEGER NOT NULL DEFAULT 0,
            is_primary     INTEGER NOT NULL DEFAULT 0,
            enabled        INTEGER NOT NULL DEFAULT 1,
            last_run_at    TEXT,
            last_status    TEXT,
            last_error     TEXT,
            next_run_at    TEXT,
            created_at     TEXT    NOT NULL,
            updated_at     TEXT    NOT NULL,
            FOREIGN KEY (certificate_id) REFERENCES certificate(id) ON DELETE CASCADE
        );
        CREATE UNIQUE INDEX IF NOT EXISTS idx_certificate_source_primary
            ON certificate_source (certificate_id) WHERE is_primary = 1;
        CREATE UNIQUE INDEX IF NOT EXISTS idx_certificate_source_token
            ON certificate_source (tenant_id, token_sha256) WHERE token_sha256 IS NOT NULL;
        CREATE INDEX IF NOT EXISTS idx_certificate_source_certificate
            ON certificate_source (tenant_id, certificate_id);",
    )
    .map_err(|e| ClientError::DatabaseMigration(format!("Failed to create certificate_source: {e}")))?;

    // Pull sources keep their behaviour; the domain decides whether they are
    // already bound to an existing certificate.
    tx.execute(
        "INSERT INTO certificate_source
            (tenant_id, certificate_id, kind, name, domain, base_url, enc_secret,
             is_primary, enabled, last_run_at, last_status, last_error, created_at, updated_at)
         SELECT s.tenant_id,
                (SELECT c.id FROM certificate c
                  WHERE c.tenant_id = s.tenant_id AND c.primary_domain = s.domain),
                s.kind, s.name, s.domain, s.base_url, s.enc_secret,
                0, s.enabled, s.last_run_at, s.last_status, s.last_error,
                s.created_at, s.updated_at
           FROM cert_pull_source s",
        [],
    )
    .map_err(|e| ClientError::DatabaseMigration(format!("Failed to move pull sources: {e}")))?;

    // Push tokens keep working as unbound channels: their allowlist is stored in
    // `domains_json` until they are attached to a certificate.
    tx.execute(
        "INSERT INTO certificate_source
            (tenant_id, certificate_id, kind, name, domains_json, token_sha256,
             expires_at, revoked_at, last_used_at, use_count, is_primary, enabled,
             created_at, updated_at)
         SELECT tenant_id, NULL, 'push', name, domains_json, token_sha256,
                expires_at, revoked_at, last_used_at, use_count, 0, 1,
                created_at, created_at
           FROM cert_ingest_token",
        [],
    )
    .map_err(|e| ClientError::DatabaseMigration(format!("Failed to move ingest tokens: {e}")))?;

    tx.execute_batch(
        "DROP TABLE IF EXISTS cert_pull_source;
         DROP TABLE IF EXISTS cert_ingest_token;",
    )
    .map_err(|e| ClientError::DatabaseMigration(format!("Failed to drop legacy channel tables: {e}")))?;

    tracing::info!("V23 Migration: certificate identity by domain, channels unified");
    Ok(())
}

/// V24: remove deployment environments.
///
/// The platform is single-user: one daemon, one SQLite file, one operator. An
/// environment was a second grouping axis on top of that, and it never worked as
/// one — the UI could switch it but not create it, and switching also rewrote
/// the profile's `tenant_id`, silently moving a server between tenants.
///
/// `tenant_id` survives on `frps_profile` and `local_proxy` because it still
/// separates API tokens; nothing here removes that.
fn migrate_v24(tx: &rusqlite::Transaction) -> Result<()> {
    // Dependants first: the mapping table references `environment`.
    tx.execute_batch("DROP TABLE IF EXISTS profile_environment;")
        .map_err(|e| {
            ClientError::DatabaseMigration(format!("Failed to drop profile_environment: {e}"))
        })?;
    tx.execute_batch("DROP TABLE IF EXISTS environment;")
        .map_err(|e| ClientError::DatabaseMigration(format!("Failed to drop environment: {e}")))?;
    tracing::info!("V24 Migration: remove deployment environments");
    Ok(())
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

    /// Upgrading a v21/v22 database must collapse same-domain rows into one
    /// identity, keep the newest material, and carry old channels over.
    #[test]
    fn v23_merges_same_domain_certificates_and_keeps_channels() {
        let conn = Connection::open_in_memory().unwrap();
        conn.pragma_update(None, "foreign_keys", "ON").unwrap();
        ensure_meta_table(&conn).unwrap();
        for version in 1..=22 {
            apply_migration(&conn, version).unwrap();
        }

        // Two certificates for one domain, as a renewal used to produce.
        for (id, fingerprint, not_after) in [
            (1, "old-fingerprint", "2026-01-01T00:00:00+00:00"),
            (2, "new-fingerprint", "2027-01-01T00:00:00+00:00"),
        ] {
            conn.execute(
                "INSERT INTO certificate
                    (id, tenant_id, primary_domain, sans_json, issuer, serial,
                     fingerprint_sha256, key_algo, not_before, not_after,
                     source, source_detail, enc_cert_der, enc_chain_der, enc_key_der,
                     created_at, updated_at)
                 VALUES (?1, 'default', 'a.example.com', '[\"a.example.com\"]', 'CA', 's',
                         ?2, 'rsa', '2025-01-01T00:00:00+00:00', ?3, 'manual', NULL,
                         x'01', NULL, x'02', '2025-01-01T00:00:00+00:00',
                         '2025-01-01T00:00:00+00:00')",
                rusqlite::params![id, fingerprint, not_after],
            )
            .unwrap();
        }
        // A proxy + binding on the older row, which must follow the merge.
        conn.execute(
            "INSERT INTO local_proxy (id, name, proxy_type, local_ip, local_port,
                                      created_at, updated_at)
             VALUES (1, 'web', 'https', '127.0.0.1', 80, 'x', 'x')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO certificate_binding (tenant_id, cert_id, proxy_id, created_at, updated_at)
             VALUES ('default', 1, 1, 'x', 'x')",
            [],
        )
        .unwrap();
        // A legacy token and a legacy pull source.
        conn.execute(
            "INSERT INTO cert_ingest_token (tenant_id, name, token_sha256, domains_json, created_at)
             VALUES ('default', 'certd', 'abc123', '[\"a.example.com\"]', 'x')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO cert_pull_source (tenant_id, name, kind, base_url, domain, created_at, updated_at)
             VALUES ('default', 'pull', 'certd', 'https://certd', 'a.example.com', 'x', 'x')",
            [],
        )
        .unwrap();

        apply_migration(&conn, 23).unwrap();

        let identities: i64 = conn
            .query_row("SELECT COUNT(*) FROM certificate", [], |row| row.get(0))
            .unwrap();
        assert_eq!(identities, 1, "one row per domain after the merge");

        let kept_id: i64 = conn
            .query_row("SELECT id FROM certificate WHERE primary_domain = 'a.example.com'", [], |row| row.get(0))
            .unwrap();
        assert_eq!(kept_id, 1, "the oldest id is the identity that survives");

        let materials: i64 = conn
            .query_row("SELECT COUNT(*) FROM certificate_material", [], |row| row.get(0))
            .unwrap();
        assert_eq!(materials, 2, "both issued bundles survive as material rows");

        let binding_cert: i64 = conn
            .query_row("SELECT cert_id FROM certificate_binding", [], |row| row.get(0))
            .unwrap();
        assert_eq!(binding_cert, kept_id, "bindings follow the merged identity");

        let newest: String = conn
            .query_row(
                "SELECT fingerprint_sha256 FROM certificate_material
                  ORDER BY not_after DESC LIMIT 1",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(newest, "new-fingerprint");

        // Channels survive, and the one whose domain matches is already attached.
        let (pull_kind, pull_cert): (String, Option<i64>) = conn
            .query_row(
                "SELECT kind, certificate_id FROM certificate_source WHERE kind != 'push'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(pull_kind, "certd");
        assert_eq!(pull_cert, Some(kept_id), "a matching pull source binds itself");

        let push_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM certificate_source WHERE kind = 'push'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(push_count, 1, "legacy tokens become unbound push channels");
    }

    /// V24 removes the environment tables while leaving tenant ownership on the
    /// rows that still use it.
    #[test]
    fn v24_drops_environments_but_keeps_tenant_ownership() {
        let conn = Connection::open_in_memory().unwrap();
        conn.pragma_update(None, "foreign_keys", "ON").unwrap();
        ensure_meta_table(&conn).unwrap();
        for version in 1..=23 {
            apply_migration(&conn, version).unwrap();
        }
        // A profile with an environment mapping, as a pre-V24 database has.
        conn.execute(
            "INSERT INTO frps_profile (id, name, server_addr, server_port, token, tenant_id,
                                       created_at, updated_at)
             VALUES (1, 'p', '127.0.0.1', 7000, '', 'default', 'x', 'x')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO profile_environment (profile_id, environment_id)
             SELECT 1, id FROM environment LIMIT 1",
            [],
        )
        .unwrap();

        apply_migration(&conn, 24).unwrap();

        for table in ["environment", "profile_environment"] {
            let exists: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?1",
                    [table],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(exists, 0, "{table} must be gone");
        }

        // Tenant ownership is what still separates API tokens, so it stays.
        let tenant: String = conn
            .query_row("SELECT tenant_id FROM frps_profile WHERE id = 1", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(tenant, "default");
        conn.query_row("SELECT tenant_id FROM local_proxy LIMIT 0", [], |_| Ok(()))
            .or_else(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => Ok(()),
                other => Err(other),
            })
            .expect("local_proxy still carries tenant_id");
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
