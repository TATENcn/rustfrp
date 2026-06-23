//! 插件上下文
//!
//! 提供插件与核心交互的接口。
//! 插件通过此上下文调用核心层暴露的能力。

use crate::{Permission, PluginError, PluginResult};
use std::collections::HashSet;
use std::sync::Arc;
use tokio::sync::RwLock;

/// 插件上下文
///
/// 每个插件实例拥有一个独立的上下文，包含：
/// - 插件名称
/// - 已授权权限
/// - 与核心通信的通道（未来）
#[derive(Debug, Clone)]
pub struct PluginContext {
    /// 插件名称
    name: String,
    /// 已授权权限
    permissions: Arc<RwLock<HashSet<Permission>>>,
}

impl PluginContext {
    /// 创建插件上下文
    pub fn new(name: &str, permissions: Vec<Permission>) -> Self {
        Self {
            name: name.to_string(),
            permissions: Arc::new(RwLock::new(permissions.into_iter().collect())),
        }
    }

    /// 获取插件名称
    pub fn name(&self) -> &str {
        &self.name
    }

    /// 检查是否拥有指定权限
    pub async fn has_permission(&self, perm: &Permission) -> bool {
        let perms = self.permissions.read().await;
        perms.contains(perm)
    }

    /// 校验权限（无权限时返回 Err）
    ///
    /// 这是插件调用核心能力前必须执行的操作。
    pub async fn check_permission(&self, perm_name: &str) -> PluginResult<()> {
        let perm: Permission = perm_name.parse().map_err(|_| {
            PluginError::InvalidArgument(format!("Unknown permission: {perm_name}"))
        })?;

        if !self.has_permission(&perm).await {
            return Err(PluginError::PermissionDenied(format!(
                "Plugin '{}' does not declare permission '{perm_name}'",
                self.name
            )));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_permission_check() {
        let ctx = PluginContext::new("test", vec![Permission::ReadTraffic]);
        assert!(ctx.check_permission("read-traffic").await.is_ok());
        assert!(ctx.check_permission("write-config").await.is_err());
    }

    #[tokio::test]
    async fn test_unknown_permission() {
        let ctx = PluginContext::new("test", vec![]);
        let result = ctx.check_permission("nonexistent").await;
        assert!(result.is_err());
    }
}
