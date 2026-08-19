//! DeepSeek Harness desktop shell.
//!
//! Spawns the DSH web backend as a Node sidecar and opens a WebView window
//! pointed at its loopback HTTP URL. The DSH upstream is never modified.
//!
//! Provides: system tray, single-instance enforcement, and auto-update wiring.

mod sidecar;

use sidecar::WebState;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{Manager, RunEvent};
use tauri_plugin_updater::UpdaterExt;

/// Check for updates in the background and install them (then restart) if found.
fn check_updates(app: &tauri::AppHandle) {
    let handle = app.clone();
    tauri::async_runtime::spawn(async move {
        let updater = match handle.updater() {
            Ok(u) => u,
            Err(e) => {
                eprintln!("dsh-desktop: updater unavailable: {e}");
                return;
            }
        };
        match updater.check().await {
            Ok(Some(update)) => {
                eprintln!("dsh-desktop: update available ({}), installing", update.version);
                match update.download_and_install(|_, _| {}, || {}).await {
                    Ok(_) => handle.restart(),
                    Err(e) => eprintln!("dsh-desktop: update install failed: {e}"),
                }
            }
            Ok(None) => {}
            Err(e) => eprintln!("dsh-desktop: update check failed: {e}"),
        }
    });
}

/// Open the live DSH web URL in the default browser (Windows: explorer.exe).
fn open_in_browser(app: &tauri::AppHandle) {
    let url = app
        .state::<WebState>()
        .0
        .lock()
        .map(|u| u.clone())
        .unwrap_or_default();
    if let Some(url) = url.filter(|u| !u.is_empty()) {
        let _ = std::process::Command::new("explorer").arg(&url).spawn();
    }
}

/// Show / focus the main window (created by the sidecar once the URL is known).
fn show_main_window(app: &tauri::AppHandle) {
    if let Some(win) = app.get_webview_window("main") {
        let _ = win.show();
        let _ = win.set_focus();
    }
}

fn setup_tray(app: &tauri::App) -> tauri::Result<()> {
    let show = MenuItem::with_id(app, "show", "Show / Focus", true, None::<&str>)?;
    let open_browser = MenuItem::with_id(app, "open_browser", "Open in Browser", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &open_browser, &quit])?;

    let icon = app.default_window_icon().cloned();
    let mut builder = TrayIconBuilder::with_id("main-tray")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "show" => show_main_window(app),
            "open_browser" => open_in_browser(app),
            "quit" => app.exit(0),
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
    if let Some(icon) = icon {
        builder = builder.icon(icon);
    }
    builder.build(app)?;
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            // A second launch only focuses the existing window.
            show_main_window(app);
        }))
        .plugin(tauri_plugin_updater::Builder::new().build())
        .setup(|app| {
            setup_tray(app)?;
            sidecar::start(app.handle());
            check_updates(app.handle());
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building dsh-desktop")
        .run(|app_handle, event| {
            // Reclaim the sidecar process when the app exits.
            if matches!(event, RunEvent::Exit) {
                sidecar::kill(app_handle);
            }
        });
}
