//! WASM 沙箱
//!
//! 定义 WASM 插件可用的 Host Functions 白名单。
//! WASM 插件无法直接访问文件系统、网络或进程——所有外部交互必须通过此模块暴露的函数完成。

use crate::plugin::manifest::Permission;

/// Host Functions 白名单
///
/// 根据插件声明的权限，动态注册可用的 Host Functions。
/// 插件只能调用已授权的函数。
#[derive(Debug, Clone)]
pub struct SandboxConfig {
    /// 插件已声明的权限
    pub permissions: Vec<Permission>,
    /// 最大内存（字节）
    pub max_memory_bytes: u64,
    /// 最大执行时间（秒）
    pub max_execution_seconds: u64,
}

impl Default for SandboxConfig {
    fn default() -> Self {
        Self {
            permissions: Vec::new(),
            max_memory_bytes: 50 * 1024 * 1024, // 50MB (PLG-005)
            max_execution_seconds: 30,
        }
    }
}

impl SandboxConfig {
    /// 创建带权限的沙箱配置
    pub fn with_permissions(permissions: Vec<Permission>) -> Self {
        Self {
            permissions,
            ..Default::default()
        }
    }

    /// 检查是否允许调用指定的 Host Function
    pub fn is_allowed(&self, function: &HostFunction) -> bool {
        self.permissions
            .iter()
            .any(|p| function.requires_permission(p))
    }
}

/// WASM 插件可用的 Host Function
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostFunction {
    /// 读取系统配置
    GetConfig,
    /// 读取流量统计
    GetTrafficStats,
    /// 订阅事件
    SubscribeEvent,
    /// 发布事件
    PublishEvent,
    /// 获取当前日志级别
    GetLogLevel,
    /// 记录日志（写入核心的 tracing 系统）
    Log,
    /// 请求核心启动、停止或切换 FRP 进程
    ControlProcess,
    /// 请求宿主发送 HTTP 通知（WASM 本身不持有 socket）
    HttpPost,
}

impl HostFunction {
    /// 调用此函数所需的最低权限
    pub fn required_permission(&self) -> Permission {
        match self {
            HostFunction::GetConfig => Permission::ReadConfig,
            HostFunction::GetTrafficStats => Permission::ReadTraffic,
            HostFunction::SubscribeEvent => Permission::SubscribeEvents,
            HostFunction::PublishEvent => Permission::SubscribeEvents,
            HostFunction::GetLogLevel => Permission::ReadConfig,
            HostFunction::Log => Permission::ReadConfig,
            HostFunction::ControlProcess => Permission::ControlProcess,
            HostFunction::HttpPost => Permission::NetworkAccess,
        }
    }

    /// 检查调用方是否拥有执行此函数的权限
    fn requires_permission(&self, perm: &Permission) -> bool {
        self.required_permission() == *perm
    }

    /// 函数名（用于注册到 Wasmtime linker）
    pub fn name(&self) -> &'static str {
        match self {
            HostFunction::GetConfig => "get_config",
            HostFunction::GetTrafficStats => "get_traffic_stats",
            HostFunction::SubscribeEvent => "subscribe_event",
            HostFunction::PublishEvent => "publish_event",
            HostFunction::GetLogLevel => "get_log_level",
            HostFunction::Log => "log",
            HostFunction::ControlProcess => "control_process",
            HostFunction::HttpPost => "http_post",
        }
    }
}

/// WASM 沙箱管理器
///
/// 负责：
/// 1. 根据 manifest 中的权限声明配置 Host Functions
/// 2. 限制 WASM 插件的资源使用（内存、执行时间）
/// 3. 在调用 Host Function 前校验权限
#[derive(Debug)]
pub struct Sandbox {
    config: SandboxConfig,
}

impl Sandbox {
    /// 创建新的沙箱实例
    pub fn new(config: SandboxConfig) -> Self {
        Self { config }
    }

    /// 获取沙箱配置
    pub fn config(&self) -> &SandboxConfig {
        &self.config
    }

    /// 检查 Host Function 调用权限
    ///
    /// 返回 Ok(()) 表示通过，Err 包含被拒绝的权限。
    pub fn check_permission(&self, function: &HostFunction) -> Result<(), String> {
        if !self.config.is_allowed(function) {
            let required = function.required_permission();
            return Err(format!(
                "Permission denied: function '{}' requires permission '{}' which is not declared",
                function.name(),
                required
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sandbox_allows_declared_permission() {
        let config = SandboxConfig::with_permissions(vec![Permission::ReadTraffic]);
        let sandbox = Sandbox::new(config);

        assert!(sandbox
            .check_permission(&HostFunction::GetTrafficStats)
            .is_ok());
    }

    #[test]
    fn test_sandbox_denies_undeclared_permission() {
        let config = SandboxConfig::with_permissions(vec![Permission::ReadConfig]);
        let sandbox = Sandbox::new(config);

        // 插件没有声明 read-traffic，但试图调用 GetTrafficStats
        let result = sandbox.check_permission(&HostFunction::GetTrafficStats);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Permission denied"));
    }

    #[test]
    fn test_sandbox_empty_permissions() {
        let config = SandboxConfig::default();
        let sandbox = Sandbox::new(config);

        // 没有任何权限，所有 Host Function 都应被拒绝
        assert!(sandbox.check_permission(&HostFunction::GetConfig).is_err());
        assert!(sandbox.check_permission(&HostFunction::Log).is_err());
    }
}
