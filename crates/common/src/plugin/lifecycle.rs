//! 插件生命周期状态机
//!
//! 管理插件从加载到卸载的完整生命周期（PLG-003）：
//!
//! ```text
//! Unloaded → [load] → Loaded → [init] → Ready → [start] → Running
//! Running → [stop] → Ready → [unload] → Unloaded
//! 任何状态 → [error] → Error → [unload] → Unloaded
//! ```

/// 插件生命周期状态
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LifecycleState {
    /// 未加载
    Unloaded,
    /// 已加载（文件校验通过，manifest 已解析）
    Loaded,
    /// 已初始化（init 完成，资源申请就绪）
    Ready,
    /// 运行中
    Running,
    /// 正在停止
    Stopping,
    /// 已停止
    Stopped,
    /// 错误状态
    Error(String),
}

impl LifecycleState {
    /// 是否允许过渡到目标状态
    ///
    /// 生命周期顺序约束（PLG-003）：
    /// - init 必须在 start 之前
    /// - stop 必须在 unload 之前
    /// - init 失败后不得调用 start
    pub fn can_transition_to(&self, target: &LifecycleState) -> bool {
        use LifecycleState::*;
        matches!(
            (self, target),
            // 加载路径
            (Unloaded, Loaded)
                | (Loaded, Ready)
                | (Ready, Running)
                | (Loaded, Error(_))
                | (Ready, Error(_))
                | (Running, Error(_))
                // 停止路径
                | (Running, Stopping)
                | (Stopping, Stopped)
                | (Stopped, Unloaded)
                | (Stopped, Ready) // 允许重新初始化
                // 从 Error 恢复
                | (Error(_), Unloaded)
        )
    }

    /// 是否表示插件处于活跃状态
    pub fn is_active(&self) -> bool {
        matches!(self, LifecycleState::Running)
    }

    /// 是否表示插件已出错
    pub fn is_error(&self) -> bool {
        matches!(self, LifecycleState::Error(_))
    }
}

impl std::fmt::Display for LifecycleState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LifecycleState::Unloaded => write!(f, "unloaded"),
            LifecycleState::Loaded => write!(f, "loaded"),
            LifecycleState::Ready => write!(f, "ready"),
            LifecycleState::Running => write!(f, "running"),
            LifecycleState::Stopping => write!(f, "stopping"),
            LifecycleState::Stopped => write!(f, "stopped"),
            LifecycleState::Error(e) => write!(f, "error: {e}"),
        }
    }
}

/// 生命周期状态管理器
///
/// 线程安全的状态过渡管理器。
#[derive(Debug, Clone)]
pub struct LifecycleManager {
    state: std::sync::Arc<tokio::sync::RwLock<LifecycleState>>,
}

impl LifecycleManager {
    /// 创建新的生命周期管理器，初始状态为 Unloaded
    pub fn new() -> Self {
        Self {
            state: std::sync::Arc::new(tokio::sync::RwLock::new(LifecycleState::Unloaded)),
        }
    }

    /// 获取当前状态
    pub async fn state(&self) -> LifecycleState {
        self.state.read().await.clone()
    }

    /// 尝试过渡到目标状态
    ///
    /// 返回 Ok(()) 表示过渡成功，Err 包含原因。
    pub async fn transition_to(&self, target: LifecycleState) -> Result<(), String> {
        let mut current = self.state.write().await;

        if !current.can_transition_to(&target) {
            return Err(format!(
                "Lifecycle transition violation: cannot transition from {} to {}",
                current, target
            ));
        }

        tracing::debug!(from = %current, to = %target, "Plugin lifecycle state change");
        *current = target;
        Ok(())
    }

    /// 强制设置为 Error 状态（插件 panic 时使用，PLG-004）
    pub async fn set_error(&self, error: String) {
        let mut state = self.state.write().await;
        *state = LifecycleState::Error(error);
    }
}

impl Default for LifecycleManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_normal_lifecycle() {
        let lm = LifecycleManager::new();
        assert_eq!(lm.state().await, LifecycleState::Unloaded);

        // Load → Ready → Running → Stopping → Stopped → Unloaded
        assert!(lm.transition_to(LifecycleState::Loaded).await.is_ok());
        assert!(lm.transition_to(LifecycleState::Ready).await.is_ok());
        assert!(lm.transition_to(LifecycleState::Running).await.is_ok());
        assert!(lm.transition_to(LifecycleState::Stopping).await.is_ok());
        assert!(lm.transition_to(LifecycleState::Stopped).await.is_ok());
        assert!(lm.transition_to(LifecycleState::Unloaded).await.is_ok());
    }

    #[tokio::test]
    async fn test_cannot_start_without_init() {
        let lm = LifecycleManager::new();
        // 跳过 Loaded → Ready，直接尝试 Running
        lm.transition_to(LifecycleState::Loaded).await.unwrap();
        let result = lm.transition_to(LifecycleState::Running).await;
        assert!(result.is_err()); // 必须经过 Ready
    }

    #[tokio::test]
    async fn test_cannot_unload_without_stop() {
        let lm = LifecycleManager::new();
        lm.transition_to(LifecycleState::Loaded).await.unwrap();
        lm.transition_to(LifecycleState::Ready).await.unwrap();
        lm.transition_to(LifecycleState::Running).await.unwrap();

        // 跳过 Stop，直接尝试 Unloaded
        let result = lm.transition_to(LifecycleState::Unloaded).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_error_recovery() {
        let lm = LifecycleManager::new();
        lm.transition_to(LifecycleState::Loaded).await.unwrap();
        lm.set_error("something went wrong".into()).await;
        assert!(lm.state().await.is_error());

        // 从 Error 可以到 Unloaded
        assert!(lm.transition_to(LifecycleState::Unloaded).await.is_ok());
    }
}
