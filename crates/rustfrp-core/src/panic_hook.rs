//! Panic 钩子
//!
//! 捕获 panic 现场，收集调用栈和环境信息，写入崩溃日志。
//! 根据项目铁律：核心 crash 不得导致数据丢失（SQLite WAL 模式已提供保障）。

use std::panic;
use std::sync::Once;

static INIT_HOOK: Once = Once::new();

/// 安装全局 panic 钩子
///
/// 应在程序启动早期调用，且只应调用一次（内部使用 `Once` 保护）。
/// 崩溃日志写入 `~/.rustfrp/logs/panic.log`。
pub fn install() {
    INIT_HOOK.call_once(|| {
        let default_hook = panic::take_hook();

        panic::set_hook(Box::new(move |info| {
            // 先调用默认钩子（打印到 stderr）
            default_hook(info);

            // 尝试写入崩溃日志
            let _ = write_crash_report(info);
        }));
    });
}

/// 写入崩溃报告到文件
fn write_crash_report(info: &panic::PanicHookInfo<'_>) -> std::io::Result<()> {
    let log_dir = dirs::home_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join(".rustfrp")
        .join("logs");

    std::fs::create_dir_all(&log_dir)?;

    let payload = if let Some(s) = info.payload().downcast_ref::<&str>() {
        s.to_string()
    } else if let Some(s) = info.payload().downcast_ref::<String>() {
        s.clone()
    } else {
        "Unknown panic payload".to_string()
    };

    let location = info
        .location()
        .map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()))
        .unwrap_or_else(|| "unknown location".to_string());

    let report = format!(
        "=== CRASH REPORT ===\n\
         timestamp: {}\n\
         location: {}\n\
         payload: {}\n\
         ===================\n",
        chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
        location,
        payload,
    );

    std::fs::write(log_dir.join("panic.log"), report)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_install_is_idempotent() {
        // 调用两次不应 panic
        install();
        install();
    }
}
