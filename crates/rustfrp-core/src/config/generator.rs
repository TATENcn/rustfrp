//! SQLite → TOML 生成器
//!
//! 从 SQLite 读取配置，按 Profile 分组生成独立的 frpc TOML 文件。
//! 原子写入：tmp → rename（PERF-003）。

use crate::config::model::{
    FrpcConfig, HealthCheckConfig, ProxyEntry, TlsConfig, TransportConfig,
};
use crate::db::Database;
use crate::error::{CoreError, Result};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// 从 SQLite 读取配置，按 Profile 分组生成多个 frpc TOML 文件
///
/// 一个 FrpsProfile → 一个 `{safe_name}.toml` → 一个 frpc 进程实例（ARCH-009, ARCH-010）。
///
/// # Arguments
///
/// * `db` - 数据库实例
/// * `output_dir` - 输出目录（TOML 文件将写入此目录下）
///
/// # Returns
///
/// 成功生成的 TOML 文件路径列表。若无启用的绑定则返回空列表。
pub async fn generate_all_frpc_tomls(
    db: &Database,
    output_dir: &Path,
) -> Result<Vec<PathBuf>> {
    let bindings = db.list_active_bindings().await?;

    if bindings.is_empty() {
        tracing::info!("No active bindings, skipping TOML generation");
        return Ok(Vec::new());
    }

    // 按 profile_id 分组
    let mut groups: HashMap<i64, Vec<_>> = HashMap::new();
    for binding in &bindings {
        groups
            .entry(binding.profile_id)
            .or_default()
            .push(binding);
    }

    std::fs::create_dir_all(output_dir)
        .map_err(|e| CoreError::TomlWrite(format!("Failed to create output directory: {e}")))?;

    let mut generated = Vec::new();

    for (profile_id, group_bindings) in &groups {
        match db.get_profile(*profile_id).await {
            Ok(profile) => {
                let config = build_frpc_config(&profile, group_bindings, db).await?;

                let safe_name = sanitize_filename(&profile.name);
                let output_path = output_dir.join(format!("{safe_name}.toml"));

                let toml_str = toml::to_string_pretty(&config)
                    .map_err(|e| CoreError::TomlSerialization(e.to_string()))?;

                atomic_write(&output_path, &toml_str)?;

                tracing::info!(
                    path = %output_path.display(),
                    profile = %profile.name,
                    proxies = config.proxies.len(),
                    "frpc TOML generated"
                );

                generated.push(output_path);
            }
            Err(e) => {
                tracing::warn!(
                    profile_id,
                    error = %e,
                    "Skipping profile: not found"
                );
                continue;
            }
        }
    }

    if generated.is_empty() {
        tracing::warn!("No TOML files generated (all profiles missing or no proxies)");
    }

    Ok(generated)
}

/// 为单个 Profile 构建 FrpcConfig
async fn build_frpc_config(
    profile: &crate::config::model::FrpsProfile,
    bindings: &[&crate::config::model::BindingRule],
    db: &Database,
) -> Result<FrpcConfig> {
    let tls = if profile.tls_enable {
        Some(TlsConfig {
            enable: true,
            cert_file: profile.tls_cert_file.clone(),
            key_file: profile.tls_key_file.clone(),
            trusted_ca_file: profile.tls_trusted_ca_file.clone(),
        })
    } else {
        None
    };

    let transport = Some(TransportConfig {
        protocol: profile.transport_protocol.clone(),
        tls,
    });

    let token = if profile.token.is_empty() {
        None
    } else {
        Some(profile.token.clone())
    };

    let mut proxies = Vec::new();
    for binding in bindings {
        match db.get_proxy(binding.proxy_id).await {
            Ok(proxy) => {
                let entries = build_proxy_entries(&proxy);
                for entry in &entries {
                    validate_proxy_entry(entry)?;
                }
                proxies.extend(entries);
            }
            Err(e) => {
                tracing::warn!(
                    proxy_id = binding.proxy_id,
                    error = %e,
                    "Skipping invalid proxy"
                );
                continue;
            }
        }
    }

    Ok(FrpcConfig {
        server_addr: profile.server_addr.clone(),
        server_port: profile.server_port,
        token,
        transport,
        proxies,
    })
}

/// 将 LocalProxy 转换为 ProxyEntry 列表
fn build_proxy_entries(
    proxy: &crate::config::model::LocalProxy,
) -> Vec<ProxyEntry> {
    let health_check = proxy.health_check_type.as_ref().map(|ht| HealthCheckConfig {
        check_type: ht.clone(),
        timeout_s: proxy.health_check_timeout_s,
        max_failed: proxy.health_check_max_failed,
        interval_s: proxy.health_check_interval_s,
    });

    let entry = ProxyEntry {
        name: proxy.name.clone(),
        proxy_type: proxy.proxy_type.to_string(),
        local_ip: proxy.local_ip.clone(),
        local_port: proxy.local_port,
        remote_port: proxy.remote_port,
        custom_domains: proxy.custom_domains.clone(),
        subdomain: proxy.subdomain.clone(),
        use_encryption: proxy.use_encryption,
        use_compression: proxy.use_compression,
        bandwidth_limit: proxy.bandwidth_limit.clone(),
        plugin: proxy.plugin_config.clone(),
        health_check,
    };

    vec![entry]
}

/// 校验 ProxyEntry 基本合法性
fn validate_proxy_entry(entry: &ProxyEntry) -> Result<()> {
    if entry.name.trim().is_empty() {
        return Err(CoreError::ConfigValidation(
            "Proxy name cannot be empty".into(),
        ));
    }

    if !["tcp", "udp", "http", "https", "stcp", "xtcp"].contains(&entry.proxy_type.as_str()) {
        return Err(CoreError::ConfigValidation(format!(
            "Unsupported proxy type: {}",
            entry.proxy_type
        )));
    }

    if entry.local_port == 0 {
        return Err(CoreError::InvalidPort(format!(
            "Proxy '{}' local_port cannot be 0",
            entry.name
        )));
    }

    Ok(())
}

/// 原子写入文件（tmp → rename）
fn atomic_write(path: &Path, content: &str) -> Result<()> {
    let tmp_path = path.with_extension("toml.tmp");

    std::fs::write(&tmp_path, content)
        .map_err(|e| CoreError::TomlWrite(format!("Failed to write temp file: {e}")))?;

    std::fs::rename(&tmp_path, path)
        .map_err(|e| CoreError::TomlWrite(format!("Atomic rename failed: {e}")))?;

    Ok(())
}

/// 文件名安全处理：替换空格、去除非安全字符
fn sanitize_filename(name: &str) -> String {
    name.chars()
        .map(|c| match c {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' => c,
            ' ' => '_',
            _ => '_',
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::model::{BindingRule, FrpsProfile, LocalProxy};
    use tempfile::TempDir;

    async fn setup_db() -> Database {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let db = Database::open(tmp.path().to_str().unwrap())
            .await
            .unwrap();
        crate::db::migrate::run(&*db.lock().await).unwrap();
        db
    }

    #[tokio::test]
    async fn test_generate_empty_tomls() {
        let db = setup_db().await;
        let output_dir = TempDir::new().unwrap();

        let paths = generate_all_frpc_tomls(&db, output_dir.path()).await.unwrap();
        // 无绑定，返回空列表
        assert!(paths.is_empty());
    }

    #[tokio::test]
    async fn test_generate_with_single_profile() {
        let db = setup_db().await;

        let profile = FrpsProfile {
            name: "Test Server".into(),
            server_addr: "frp.example.com".into(),
            server_port: 7000,
            token: "test123".into(),
            ..Default::default()
        };
        let profile_id = db.insert_profile(&profile).await.unwrap();

        let proxy = LocalProxy {
            name: "RDP".into(),
            local_ip: "127.0.0.1".into(),
            local_port: 3389,
            remote_port: Some(13389),
            ..Default::default()
        };
        let proxy_id = db.insert_proxy(&proxy).await.unwrap();

        let binding = BindingRule {
            id: None,
            profile_id,
            proxy_id,
            enabled: true,
            priority: 0,
            group_name: None,
            group_key: None,
            created_at: String::new(),
            updated_at: String::new(),
        };
        db.insert_binding(&binding).await.unwrap();

        let output_dir = TempDir::new().unwrap();
        let paths = generate_all_frpc_tomls(&db, output_dir.path()).await.unwrap();

        assert_eq!(paths.len(), 1);
        let content = std::fs::read_to_string(&paths[0]).unwrap();
        assert!(content.contains("frp.example.com"));
        assert!(content.contains("RDP"));
        assert!(content.contains("3389"));
        // 文件名应为 sanitized profile name
        assert!(paths[0].file_name().unwrap().to_string_lossy().contains("Test_Server"));
    }

    #[tokio::test]
    async fn test_generate_multiple_profiles() {
        let db = setup_db().await;

        // 创建两个 Profile
        let p1 = FrpsProfile {
            name: "Server Alpha".into(),
            server_addr: "alpha.example.com".into(),
            server_port: 7000,
            token: "tok1".into(),
            ..Default::default()
        };
        let p1_id = db.insert_profile(&p1).await.unwrap();

        let p2 = FrpsProfile {
            name: "Server Beta".into(),
            server_addr: "beta.example.com".into(),
            server_port: 7000,
            token: "tok2".into(),
            ..Default::default()
        };
        let p2_id = db.insert_profile(&p2).await.unwrap();

        // 各绑定一个 Proxy
        let proxy = LocalProxy {
            name: "Web".into(),
            local_ip: "127.0.0.1".into(),
            local_port: 80,
            remote_port: Some(8080),
            ..Default::default()
        };
        let proxy_id = db.insert_proxy(&proxy).await.unwrap();

        for &pid in &[p1_id, p2_id] {
            db.insert_binding(&BindingRule {
                id: None,
                profile_id: pid,
                proxy_id,
                enabled: true,
                priority: 0,
                group_name: None,
                group_key: None,
                created_at: String::new(),
                updated_at: String::new(),
            })
            .await
            .unwrap();
        }

        let output_dir = TempDir::new().unwrap();
        let paths = generate_all_frpc_tomls(&db, output_dir.path()).await.unwrap();

        assert_eq!(paths.len(), 2);
        let names: Vec<String> = paths
            .iter()
            .map(|p| p.file_stem().unwrap().to_string_lossy().to_string())
            .collect();
        assert!(names.contains(&"Server_Alpha".to_string()));
        assert!(names.contains(&"Server_Beta".to_string()));
    }

    #[tokio::test]
    async fn test_atomic_write_no_orphan_temp() {
        let db = setup_db().await;
        let output_dir = TempDir::new().unwrap();

        // 无绑定时不生成文件，tmp 也不应残留
        let paths = generate_all_frpc_tomls(&db, output_dir.path()).await.unwrap();
        assert!(paths.is_empty());

        // 检查目录中无 tmp 残留
        let tmp_files: Vec<_> = std::fs::read_dir(output_dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.file_name()
                    .to_string_lossy()
                    .ends_with(".toml.tmp")
            })
            .collect();
        assert!(tmp_files.is_empty());
    }

    #[test]
    fn test_sanitize_filename() {
        assert_eq!(sanitize_filename("My Server"), "My_Server");
        assert_eq!(sanitize_filename("home-nas"), "home-nas");
        assert_eq!(sanitize_filename("a/b:c"), "a_b_c");
        assert_eq!(sanitize_filename("公司服务器"), "_____");
    }
}
