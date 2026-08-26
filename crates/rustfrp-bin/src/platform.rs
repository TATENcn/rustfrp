//! 平台探测
//!
//! 根据编译目标推断 FRP GitHub Release 的 platform 字符串，
//! 形如 `linux_amd64` / `linux_arm64` / `darwin_amd64` / `windows_amd64`。

/// 已识别的目标平台字符串
///
/// 对应 fatedier/frp Release 资产文件名中的平台段：
/// `frp_{version}_{platform}.tar.gz`
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Platform {
    /// 完整平台串，如 `linux_amd64`
    pub slug: String,
}

impl Platform {
    /// 从当前编译目标推断平台
    ///
    /// 失败（如未知组合）时返回 `None`，调用方应回退到显式指定。
    pub fn detect() -> Option<Self> {
        let os = match std::env::consts::OS {
            "linux" => "linux",
            "macos" => "darwin",
            "windows" => "windows",
            _ => return None,
        };
        let arch = match std::env::consts::ARCH {
            "x86_64" => "amd64",
            "aarch64" => "arm64",
            "arm" => "arm",
            "riscv64" => "riscv64",
            _ => return None,
        };
        Some(Self {
            slug: format!("{os}_{arch}"),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_returns_known_slug_on_common_targets() {
        // 仅验证在常见目标上能产出非空且含下划线的串
        if let Some(p) = Platform::detect() {
            assert!(p.slug.contains('_'), "platform slug must be os_arch");
        }
    }
}
