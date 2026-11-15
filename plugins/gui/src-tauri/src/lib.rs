//! RustFRP desktop shell.
//!
//! The window is not a separate application: it is a Tauri webview pointed at the
//! same loopback HTTP API the browser and container deployments use
//! ([`rustfrp_daemon::DesktopSession`]). Keeping one API means the UI has no
//! desktop-only code paths and no IPC surface:
//!
//! * zero `#[tauri::command]`s — the webview only speaks HTTP,
//! * zero capability permissions — nothing to escalate from injected script,
//! * the bearer token reaches the page through `initialization_script`, never
//!   through the URL, so it cannot leak into history or a referer.
//!
//! Closing the window hides it to the tray: collapsing the frpc tunnels because
//! someone clicked the wrong button would defeat the purpose of a tunnel manager.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use rustfrp_daemon::{DesktopError, DesktopOptions, DesktopSession};
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, RunEvent, WebviewUrl, WebviewWindowBuilder, WindowEvent, Wry};

/// Tray menu item identifiers.
mod menu_id {
    pub const SHOW: &str = "show";
    pub const OPEN_DATA_DIR: &str = "open-data-dir";
    pub const QUIT: &str = "quit";
}

/// Label of the single application window.
const MAIN_WINDOW: &str = "main";

/// Environment override for the data directory, mirroring `--db-path`'s parent.
const DATA_DIR_ENV: &str = "RUSTFRP_DATA_DIR";

/// `Some` until the session is shut down.
struct SessionSlot(Mutex<Option<DesktopSession>>);

impl SessionSlot {
    fn new(session: DesktopSession) -> Self {
        Self(Mutex::new(Some(session)))
    }

    fn take(&self) -> Option<DesktopSession> {
        self.0.lock().expect("session mutex poisoned").take()
    }
}

/// Guards the exit handshake so shutdown runs exactly once.
#[derive(Default)]
struct QuitState {
    started: AtomicBool,
}

/// Build and run the desktop application. Blocks until the window closes.
pub fn run() {
    let app = build()
        .build(tauri::generate_context!())
        .expect("failed to build the RustFRP desktop application");

    app.run(|app, event| match event {
        RunEvent::ExitRequested { api, .. } => {
            if app
                .try_state::<QuitState>()
                .is_some_and(|state| state.started.load(Ordering::SeqCst))
            {
                // Second pass, triggered by `AppHandle::exit` after teardown.
                return;
            }
            // frpc children must be stopped through the normal signal chain,
            // which cannot happen once the event loop is gone.
            api.prevent_exit();
            request_quit(app);
        }
        RunEvent::WindowEvent {
            label,
            event: WindowEvent::CloseRequested { api, .. },
            ..
        } if label == MAIN_WINDOW => {
            api.prevent_close();
            if let Some(window) = app.get_webview_window(MAIN_WINDOW) {
                let _ = window.hide();
            }
        }
        _ => {}
    });
}

fn build() -> tauri::Builder<Wry> {
    tauri::Builder::default()
        // Must be registered first: it exits before any other plugin runs.
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            show_main_window(app);
        }))
        .plugin(tauri_plugin_window_state::Builder::default().build())
        .plugin(tauri_plugin_dialog::init())
        .manage(QuitState::default())
        .setup(|app| {
            // A tray is a convenience, not a requirement: the window is fully
            // usable without one. Reporting the error out of `setup` makes Tauri
            // panic and kill the whole application, which is what happened when
            // the shell was launched in a way that denied tray creation
            // (`error encountered during setup hook: tray icon error: Access is
            // denied`). Degrade instead, and say so.
            if let Err(error) = install_tray(app.handle()) {
                tracing::warn!(
                    %error,
                    "tray icon unavailable; continuing without one (the window still works)"
                );
            }
            start_session(app.handle().clone());
            Ok(())
        })
}

/// Start the daemon session, then open the window on the resulting URL.
///
/// Runs off the main thread: `ClientCore` opens SQLite and the process manager,
/// and blocking the event loop there would leave the app windowless and frozen
/// if the database is on slow storage.
fn start_session(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let options = options(&app);
        let dev_url = if tauri::is_dev() {
            app.config().build.dev_url.as_ref().map(ToString::to_string)
        } else {
            None
        };

        match DesktopSession::start(options).await {
            Ok(session) => {
                let target = dev_url.unwrap_or_else(|| session.url().to_owned());
                let script = session.initialization_script();
                let stopped = session.stopped();
                tracing::info!(%target, "opening the RustFRP window");
                app.manage(SessionSlot::new(session));
                if let Err(error) = open_window(&app, &target, &script) {
                    report_fatal(&app, &format!("Cannot open the window: {error}"));
                    return;
                }
                watch_backend(&app, stopped);
            }
            Err(error) => report_fatal(&app, &describe(&error)),
        }
    });
}

/// Exit when the embedded daemon stops on its own.
///
/// Without this the window survives a backend that was stopped from outside
/// (SIGTERM from a service manager, a crash in the server task) and shows an
/// unusable UI. Quitting is the only honest outcome: the tunnels are already
/// gone, and the tray would relaunch nothing.
fn watch_backend(app: &AppHandle, mut stopped: tokio::sync::watch::Receiver<bool>) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        if *stopped.borrow() {
            return;
        }
        // `Err` means the sender was dropped, i.e. the server task ended too.
        let _ = stopped.changed().await;
        if app
            .try_state::<QuitState>()
            .is_some_and(|state| state.started.load(Ordering::SeqCst))
        {
            // A tray "Quit" is already tearing down; it exits on its own.
            return;
        }
        tracing::warn!("the embedded daemon stopped; closing the desktop shell");
        app.exit(0);
    });
}

/// Session options: `~/.rustfrp`, an OS-assigned port, and no startup download.
///
/// In development the API must bind its documented port because the Vite dev
/// server proxies `/api` to it; a packaged build hides behind a random port.
fn options(app: &AppHandle) -> DesktopOptions {
    let data_dir = std::env::var_os(DATA_DIR_ENV)
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            app.path()
                .home_dir()
                .unwrap_or_else(|_| std::path::PathBuf::from("."))
                .join(".rustfrp")
        });

    DesktopOptions {
        config_dir: data_dir.join("runtime"),
        data_dir,
        listen: if tauri::is_dev() {
            // The Vite dev server proxies `/api` to this fixed port.
            rustfrp_daemon::desktop::DEV_LOOPBACK.to_owned()
        } else {
            rustfrp_daemon::desktop::EPHEMERAL_LOOPBACK.to_owned()
        },
        frpc_path: None,
    }
}

fn open_window(app: &AppHandle, url: &str, initial_script: &str) -> tauri::Result<()> {
    let url: tauri::Url = url.parse().map_err(tauri::Error::InvalidUrl)?;

    WebviewWindowBuilder::new(app, MAIN_WINDOW, WebviewUrl::External(url))
        .title("RustFRP")
        .inner_size(1280.0, 800.0)
        .min_inner_size(960.0, 600.0)
        .center()
        .initialization_script(initial_script)
        .build()?;

    Ok(())
}

/// Show the window, recreating it if the tray is all that is left.
fn show_main_window(app: &AppHandle) {
    match app.get_webview_window(MAIN_WINDOW) {
        Some(window) => {
            let _ = window.show();
            let _ = window.unminimize();
            let _ = window.set_focus();
        }
        None => {
            // The window is only missing before the session finished starting or
            // after a failed start; both cases already surfaced a message.
            tracing::warn!("main window is not open yet");
        }
    }
}

fn install_tray(app: &AppHandle) -> tauri::Result<()> {
    let menu = Menu::with_items(
        app,
        &[
            &MenuItem::with_id(app, menu_id::SHOW, "显示主窗口", true, None::<&str>)?,
            &MenuItem::with_id(
                app,
                menu_id::OPEN_DATA_DIR,
                "打开数据目录",
                true,
                None::<&str>,
            )?,
            &MenuItem::with_id(app, menu_id::QUIT, "退出 RustFRP", true, None::<&str>)?,
        ],
    )?;

    let mut builder = TrayIconBuilder::with_id("main")
        .menu(&menu)
        .tooltip("RustFRP")
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            menu_id::SHOW => show_main_window(app),
            menu_id::OPEN_DATA_DIR => open_data_dir(app),
            menu_id::QUIT => request_quit(app),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main_window(tray.app_handle());
            }
        });

    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }

    builder.build(app)?;
    Ok(())
}

/// Reveal the data directory in the platform file manager.
fn open_data_dir(app: &AppHandle) {
    // `try_state`: the tray exists before the session finishes starting, and a
    // failed start never registers the slot.
    let Some(slot) = app.try_state::<SessionSlot>() else {
        return;
    };
    let dir = match slot.0.lock().ok().and_then(|guard| {
        guard
            .as_ref()
            .map(|session| session.data_dir().to_path_buf())
    }) {
        Some(dir) => dir,
        None => return,
    };

    #[cfg(target_os = "windows")]
    let program = "explorer";
    #[cfg(target_os = "macos")]
    let program = "open";
    #[cfg(all(unix, not(target_os = "macos")))]
    let program = "xdg-open";

    if let Err(error) = std::process::Command::new(program).arg(&dir).spawn() {
        tracing::warn!(dir = %dir.display(), %error, "cannot open the data directory");
    }
}

/// Stop frpc children, then exit.
///
/// The event loop stays alive until teardown finishes so `SIGTERM` → 3s →
/// `SIGKILL` can run per instance instead of orphaning tunnels.
fn request_quit(app: &AppHandle) {
    if app
        .state::<QuitState>()
        .started
        .swap(true, Ordering::SeqCst)
    {
        return;
    }

    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        // A failed start leaves no session behind; exit directly in that case.
        let session = app.try_state::<SessionSlot>().and_then(|slot| slot.take());
        if let Some(session) = session {
            session.shutdown().await;
        }
        app.exit(0);
    });
}

/// Show a blocking message and exit: nothing can be managed without a session.
fn report_fatal(app: &AppHandle, message: &str) {
    tracing::error!(%message, "desktop session failed to start");
    use tauri_plugin_dialog::{DialogExt, MessageDialogKind};
    app.dialog()
        .message(message)
        .title("RustFRP")
        .kind(MessageDialogKind::Error)
        .show(|_| {});
    app.exit(1);
}

fn describe(error: &DesktopError) -> String {
    match error {
        DesktopError::InstanceLock(lock) => format!(
            "{lock}\n\nStop the other RustFRP process (daemon, container, or another \
             desktop window) and try again."
        ),
        DesktopError::Core(error) => format!("RustFRP cannot start:\n{error}"),
    }
}
