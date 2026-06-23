//! 插件管理器
//!
//! 负责插件的加载、卸载、查询。
//! 支持 WASM / Native / Sidecar 三种插件形态。
//!
//! # 约束
//!
//! - PluginManager 是核心层唯一与插件交互的入口
//! - 插件崩溃不得拖垮核心（PLG-004）
//! - 权限不足的调用必须被拒绝（PLG-002）

use crate::error::{Result, SharedError};
use crate::plugin::lifecycle::LifecycleManager;
use crate::plugin::manifest::{PluginManifest, PluginType};
use crate::plugin::sandbox::{Sandbox, SandboxConfig};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::RwLock;

/// 已加载的插件实例
#[derive(Debug)]
struct PluginInstance {
    /// Manifest 信息
    manifest: PluginManifest,
    /// 生命周期状态
    lifecycle: LifecycleManager,
    /// WASM 沙箱（仅用于 WASM 类型）
    #[allow(dead_code)]
    sandbox: Option<Sandbox>,
    /// 插件目录路径
    #[allow(dead_code)]
    dir: PathBuf,
}

impl PluginInstance {
    fn new(manifest: PluginManifest, dir: PathBuf) -> Self {
        let sandbox = if manifest.plugin_type == PluginType::Wasm {
            Some(Sandbox::new(SandboxConfig::with_permissions(
                manifest.permissions.clone(),
            )))
        } else {
            None
        };

        Self {
            manifest,
            lifecycle: LifecycleManager::new(),
            sandbox,
            dir,
        }
    }
}

/// 插件管理器
///
/// 线程安全，支持热插拔。
#[derive(Debug)]
pub struct PluginManager {
    /// 插件目录
    plugins_dir: PathBuf,
    /// 已加载的插件实例（名称 → 实例）
    instances: Arc<RwLock<HashMap<String, PluginInstance>>>,
}

impl PluginManager {
    /// 创建插件管理器
    ///
    /// # Arguments
    ///
    /// * `plugins_dir` - 插件根目录（通常为 `~/.rustfrp/plugins/`）
    pub fn new(plugins_dir: PathBuf) -> Self {
        Self {
            plugins_dir,
            instances: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// 使用默认插件目录
    pub fn with_default_dir() -> Self {
        let dir = dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".rustfrp")
            .join("plugins");
        Self::new(dir)
    }

    /// 从插件目录加载所有插件
    ///
    /// 扫描 `plugins_dir` 下的每个子目录，寻找 `manifest.json`。
    pub async fn load_all(&self) -> Result<Vec<String>> {
        // 确保插件目录存在
        std::fs::create_dir_all(&self.plugins_dir).map_err(|e| {
            SharedError::PluginLoad(format!("Failed to create plugin directory: {e}"))
        })?;

        let mut loaded = Vec::new();

        let entries = std::fs::read_dir(&self.plugins_dir).map_err(|e| {
            SharedError::PluginLoad(format!("Failed to read plugin directory: {e}"))
        })?;

        for entry in entries {
            let entry = entry.map_err(|e| {
                SharedError::PluginLoad(format!("Failed to read directory entry: {e}"))
            })?;

            let dir = entry.path();
            if !dir.is_dir() {
                continue;
            }

            let manifest_path = dir.join("manifest.json");
            if !manifest_path.exists() {
                tracing::debug!(dir = %dir.display(), "跳过非插件目录（无 manifest.json）");
                continue;
            }

            match self.load_plugin(&dir).await {
                Ok(name) => loaded.push(name),
                Err(e) => {
                    tracing::warn!(
                        dir = %dir.display(),
                        error = %e,
                        "插件加载失败，继续加载其他插件（插件隔离原则）"
                    );
                }
            }
        }

        tracing::info!(count = loaded.len(), "插件加载完成");

        Ok(loaded)
    }

    /// 加载单个插件
    ///
    /// # Arguments
    ///
    /// * `dir` - 插件目录（需包含 manifest.json）
    pub async fn load_plugin(&self, dir: &Path) -> Result<String> {
        let manifest_path = dir.join("manifest.json");
        let manifest_json = std::fs::read_to_string(&manifest_path)
            .map_err(|e| SharedError::PluginLoad(format!("Failed to read manifest.json: {e}")))?;

        let manifest = PluginManifest::from_json(&manifest_json).map_err(|e| {
            SharedError::PluginValidation(format!("manifest.json parse failed: {e}"))
        })?;

        // 校验 manifest
        manifest
            .validate()
            .map_err(|errors| SharedError::PluginValidation(errors.join("; ")))?;

        let name = manifest.name.clone();

        // 检查插件是否已存在
        {
            let instances = self.instances.read().await;
            if instances.contains_key(&name) {
                return Err(SharedError::PluginLoad(format!(
                    "Plugin '{name}' is already loaded"
                )));
            }
        }

        // 验证入口文件存在
        let entry_path = dir.join(&manifest.entry);
        if !entry_path.exists() {
            return Err(SharedError::PluginLoad(format!(
                "Entry file does not exist: {}",
                entry_path.display()
            )));
        }

        // 校验依赖
        self.check_dependencies(&manifest).await?;

        // 创建插件实例
        let instance = PluginInstance::new(manifest, dir.to_path_buf());

        tracing::info!(
            name = %name,
            plugin_type = ?instance.manifest.plugin_type,
            "插件已加载"
        );

        self.instances.write().await.insert(name.clone(), instance);

        Ok(name)
    }

    /// 卸载插件
    ///
    /// 只在插件处于 Stopped 或 Error 状态时允许卸载。
    pub async fn unload_plugin(&self, name: &str) -> Result<()> {
        let mut instances = self.instances.write().await;

        let instance = instances
            .get(name)
            .ok_or_else(|| SharedError::PluginUnload(format!("Plugin '{name}' does not exist")))?;

        let state = instance.lifecycle.state().await;
        if !matches!(
            state,
            crate::plugin::lifecycle::LifecycleState::Stopped
                | crate::plugin::lifecycle::LifecycleState::Unloaded
                | crate::plugin::lifecycle::LifecycleState::Error(_)
                | crate::plugin::lifecycle::LifecycleState::Loaded
        ) {
            return Err(SharedError::PluginUnload(format!(
                "Cannot unload plugin '{name}': current state is {state}, stop first"
            )));
        }

        instances.remove(name);
        tracing::info!(name, "插件已卸载");
        Ok(())
    }

    /// 列出所有已加载的插件
    pub async fn list_plugins(&self) -> Vec<PluginInfo> {
        let instances = self.instances.read().await;
        let mut infos = Vec::new();

        for instance in instances.values() {
            infos.push(PluginInfo {
                name: instance.manifest.name.clone(),
                version: instance.manifest.version.clone(),
                plugin_type: instance.manifest.plugin_type.clone(),
                description: instance.manifest.description.clone(),
            });
        }

        infos.sort_by(|a, b| a.name.cmp(&b.name));
        infos
    }

    /// 获取插件数量
    pub async fn plugin_count(&self) -> usize {
        self.instances.read().await.len()
    }

    /// 校验插件依赖
    async fn check_dependencies(&self, manifest: &PluginManifest) -> Result<()> {
        let instances = self.instances.read().await;
        for dep in &manifest.dependencies {
            if !instances.contains_key(dep) {
                return Err(SharedError::PluginValidation(format!(
                    "Plugin '{}' depends on '{}' which is not loaded",
                    manifest.name, dep
                )));
            }
        }
        Ok(())
    }
}

/// 插件简要信息（供外部查询）
#[derive(Debug, Clone)]
pub struct PluginInfo {
    pub name: String,
    pub version: String,
    pub plugin_type: PluginType,
    pub description: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn write_manifest(dir: &Path, content: &str) {
        std::fs::write(dir.join("manifest.json"), content).unwrap();
    }

    fn write_entry(dir: &Path, name: &str) {
        std::fs::write(dir.join(name), "dummy content").unwrap();
    }

    #[tokio::test]
    async fn test_load_and_list_plugins() {
        let tmp = TempDir::new().unwrap();
        let plugin_dir = tmp.path().join("test-plugin");
        std::fs::create_dir_all(&plugin_dir).unwrap();

        write_manifest(
            &plugin_dir,
            r#"{
                "name": "test-plugin",
                "version": "1.0.0",
                "type": "wasm",
                "entry": "test.wasm",
                "description": "A test plugin",
                "permissions": ["read-config"],
                "dependencies": [],
                "min_core_version": "1.0.0"
            }"#,
        );
        write_entry(&plugin_dir, "test.wasm");

        let manager = PluginManager::new(tmp.path().to_path_buf());
        let loaded = manager.load_all().await.unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0], "test-plugin");

        let list = manager.list_plugins().await;
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].name, "test-plugin");
    }

    #[tokio::test]
    async fn test_skip_invalid_plugin() {
        let tmp = TempDir::new().unwrap();
        let plugin_dir = tmp.path().join("bad-plugin");
        std::fs::create_dir_all(&plugin_dir).unwrap();

        // 空 manifest
        write_manifest(&plugin_dir, "{}");

        let manager = PluginManager::new(tmp.path().to_path_buf());
        let loaded = manager.load_all().await.unwrap();
        // 无效插件应被跳过，不崩溃
        assert!(loaded.is_empty());
    }

    #[tokio::test]
    async fn test_unload_plugin() {
        let tmp = TempDir::new().unwrap();
        let plugin_dir = tmp.path().join("test-plugin");
        std::fs::create_dir_all(&plugin_dir).unwrap();

        write_manifest(
            &plugin_dir,
            r#"{
                "name": "test-plugin",
                "version": "1.0.0",
                "type": "wasm",
                "entry": "test.wasm",
                "permissions": [],
                "dependencies": [],
                "min_core_version": "1.0.0"
            }"#,
        );
        write_entry(&plugin_dir, "test.wasm");

        let manager = PluginManager::new(tmp.path().to_path_buf());
        manager.load_all().await.unwrap();
        assert_eq!(manager.plugin_count().await, 1);

        manager.unload_plugin("test-plugin").await.unwrap();
        assert_eq!(manager.plugin_count().await, 0);
    }
}
