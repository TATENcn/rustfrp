//! Desktop entry point.
//!
//! A GUI process has no console on Windows and macOS, so the default
//! human-readable stderr log would vanish. Switch to the file sink unless the
//! operator asked for something else.

// Without this the Windows release binary is a console program: launching it
// from Explorer opens a second black window alongside the UI. Kept out of debug
// builds so `tauri dev` still shows panics and log output in the terminal.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    if std::env::var_os("RUSTFRP_LOG_MODE").is_none() {
        std::env::set_var("RUSTFRP_LOG_MODE", "json");
    }
    rustfrp_common::logging::init();
    rustfrp_common::panic_hook::install();

    rustfrp_desktop::run();
}
