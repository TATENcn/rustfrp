//! Manifest 解析与校验
//!
//! 每个插件必须提供 `manifest.json`（PLG-001）。
//! 此模块负责解析和校验 manifest 的必填字段。

use serde::{Deserialize, Serialize};

/// 插件类型
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PluginType {
    /// WASM 插件（在 Wasmtime 沙箱中运行）
    Wasm,
    /// 动态库插件（libloading，进程内运行）
    Native,
    /// Sidecar 插件（独立子进程）
    Sidecar,
}

/// 插件权限声明
///
/// 插件在 manifest 中声明所需权限，核心层在每次调用前校验（PLG-002）。
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Permission {
    /// 读取配置
    ReadConfig,
    /// 修改配置
    WriteConfig,
    /// 读取流量数据
    ReadTraffic,
    /// 控制 FRP 进程
    ControlProcess,
    /// 订阅事件
    SubscribeEvents,
    /// 网络访问
    NetworkAccess,
    /// 文件系统访问
    FilesystemAccess,
}

impl std::str::FromStr for Permission {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s {
            "read-config" => Ok(Permission::ReadConfig),
            "write-config" => Ok(Permission::WriteConfig),
            "read-traffic" => Ok(Permission::ReadTraffic),
            "control-process" => Ok(Permission::ControlProcess),
            "subscribe-events" => Ok(Permission::SubscribeEvents),
            "network-access" => Ok(Permission::NetworkAccess),
            "filesystem-access" => Ok(Permission::FilesystemAccess),
            _ => Err(format!("unknown permission: {s}")),
        }
    }
}

impl std::fmt::Display for Permission {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Permission::ReadConfig => write!(f, "read-config"),
            Permission::WriteConfig => write!(f, "write-config"),
            Permission::ReadTraffic => write!(f, "read-traffic"),
            Permission::ControlProcess => write!(f, "control-process"),
            Permission::SubscribeEvents => write!(f, "subscribe-events"),
            Permission::NetworkAccess => write!(f, "network-access"),
            Permission::FilesystemAccess => write!(f, "filesystem-access"),
        }
    }
}

/// 插件 manifest
///
/// 定义在 `manifest.json` 中，描述插件的基本信息和所需权限。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginManifest {
    /// 插件名称（全局唯一）
    pub name: String,
    /// 版本号（语义化版本）
    pub version: String,
    /// 插件类型
    #[serde(rename = "type")]
    pub plugin_type: PluginType,
    /// 入口文件路径（wasm 文件、.so/.dll 或可执行文件）
    pub entry: String,
    /// 描述
    pub description: Option<String>,
    /// 权限声明
    #[serde(default)]
    pub permissions: Vec<Permission>,
    /// 依赖的其他插件（名称列表）
    #[serde(default)]
    pub dependencies: Vec<String>,
    /// 最低核心版本要求
    pub min_core_version: String,
}

impl PluginManifest {
    /// 从 JSON 字符串解析 manifest
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }

    /// 校验 manifest 必填字段和合法性（PLG-001）
    ///
    /// 返回 Ok(()) 表示通过，Err 包含具体违规信息。
    pub fn validate(&self) -> Result<(), Vec<String>> {
        let mut errors = Vec::new();

        // name 必填且非空
        if self.name.trim().is_empty() {
            errors.push("name cannot be empty".into());
        }

        // version 必填且符合 semver
        if self.version.trim().is_empty() {
            errors.push("version cannot be empty".into());
        } else if !is_semver(&self.version) {
            errors.push(format!(
                "version '{}' does not match semver format",
                self.version
            ));
        }

        // entry 必填且非空
        if self.entry.trim().is_empty() {
            errors.push("entry cannot be empty".into());
        }

        // min_core_version 必填
        if self.min_core_version.trim().is_empty() {
            errors.push("min_core_version cannot be empty".into());
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    /// 检查是否声明了某权限
    pub fn has_permission(&self, perm: &Permission) -> bool {
        self.permissions.contains(perm)
    }
}

/// 简单校验是否为语义化版本格式 (X.Y.Z)
fn is_semver(version: &str) -> bool {
    let parts: Vec<&str> = version.split('.').collect();
    if parts.len() < 2 || parts.len() > 4 {
        return false;
    }
    parts.iter().all(|p| p.parse::<u64>().is_ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_valid_manifest() {
        let json = r#"{
            "name": "traffic-monitor",
            "version": "1.0.0",
            "type": "wasm",
            "entry": "traffic_monitor.wasm",
            "description": "实时流量统计",
            "permissions": ["read-traffic", "subscribe-events"],
            "dependencies": [],
            "min_core_version": "1.0.0"
        }"#;

        let manifest = PluginManifest::from_json(json).unwrap();
        assert_eq!(manifest.name, "traffic-monitor");
        assert_eq!(manifest.plugin_type, PluginType::Wasm);
        assert!(manifest.validate().is_ok());
    }

    #[test]
    fn test_missing_required_fields() {
        let json =
            r#"{"name": "", "version": "", "type": "wasm", "entry": "", "min_core_version": ""}"#;
        let manifest = PluginManifest::from_json(json).unwrap();
        let result = manifest.validate();
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors.len() >= 3); // name, version, entry 都为空
    }

    #[test]
    fn test_invalid_semver() {
        let json = r#"{
            "name": "test", "version": "not-a-version",
            "type": "wasm", "entry": "test.wasm",
            "min_core_version": "1.0.0"
        }"#;
        let manifest = PluginManifest::from_json(json).unwrap();
        let result = manifest.validate();
        assert!(result.is_err());
    }

    #[test]
    fn test_semver_valid() {
        assert!(is_semver("1.0.0"));
        assert!(is_semver("0.1.0"));
        assert!(is_semver("10.20.30"));
        assert!(is_semver("1.0")); // valid: equivalent to 1.0.0
        assert!(!is_semver("not-semver"));
        assert!(!is_semver("v1.0.0")); // invalid: 'v' prefix
    }
}
