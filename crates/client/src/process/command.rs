//! Child-process construction that never flashes a console window.
//!
//! On Windows a console program spawned by a GUI process gets its own console
//! window unless the child is created with `CREATE_NO_WINDOW`. The desktop shell
//! is a GUI process (`windows_subsystem = "windows"`), so every `frpc` it runs —
//! `verify`, `reload`, and the supervised instances themselves — would otherwise
//! pop a black window. A failing profile makes that worse: the restart loop
//! spawns `frpc` repeatedly, so the window appears and disappears in a cycle.
//!
//! Centralised here so no call site can forget the flag.

/// Build a [`Command`] for a child that must stay invisible.
///
/// Everywhere but Windows this is a plain `Command::new(program)`.
pub fn child_command(program: impl AsRef<std::ffi::OsStr>) -> tokio::process::Command {
    #[allow(unused_mut)]
    let mut command = tokio::process::Command::new(program);
    hide_console_window(&mut command);
    command
}

#[cfg(windows)]
fn hide_console_window(command: &mut tokio::process::Command) {
    /// `CREATE_NO_WINDOW` from `winbase.h`. Hardcoded rather than pulled from a
    /// bindings crate: it is a stable ABI constant and the alternative is a new
    /// dependency in the core library for one `u32`.
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    command.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(not(windows))]
fn hide_console_window(_command: &mut tokio::process::Command) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(not(windows))]
    #[tokio::test]
    async fn child_command_runs_a_program() {
        let output = child_command("echo")
            .arg("frp")
            .output()
            .await
            .expect("echo must be spawnable");
        assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "frp");
    }

    /// A GUI process spawning a console child without `CREATE_NO_WINDOW` either
    /// opens a window or fails outright with `STATUS_DLL_INIT_FAILED`
    /// (0xC0000142) when it has no console to inherit. The flag is what makes the
    /// child succeed silently, so this asserts the child actually runs.
    #[cfg(windows)]
    #[tokio::test]
    async fn child_runs_silently_under_a_windowless_parent() {
        let output = child_command("cmd")
            .args(["/C", "echo quiet"])
            .output()
            .await
            .expect("cmd must be spawnable");
        assert!(
            output.status.success(),
            "child failed: {:?}",
            output.status.code()
        );
        assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "quiet");
    }
}
