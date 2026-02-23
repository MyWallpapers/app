//! MyWallpaper Desktop Application
//!
//! Tauri backend for the MyWallpaper animated wallpaper application.

mod commands;
mod commands_core;
mod tray;
mod window_layer;

use log::{error, info};

fn mw_init_script() -> String {
    format!(
        r#"window.__MW_INIT__ = {{ isTauri: true, platform: "{}", arch: "{}", appVersion: "{}", tauriVersion: "{}", debug: {} }};"#,
        std::env::consts::OS,
        std::env::consts::ARCH,
        env!("CARGO_PKG_VERSION"),
        tauri::VERSION,
        cfg!(debug_assertions),
    )
}

pub use commands::*;

pub fn main() {
    #[cfg(target_os = "windows")]
    if let Some(local_appdata) = std::env::var_os("LOCALAPPDATA") {
        let log_dir = std::path::Path::new(&local_appdata)
            .join("com.mywallpaper.desktop")
            .join("logs");
        if let Ok(entries) = std::fs::read_dir(&log_dir) {
            entries
                .flatten()
                .filter(|e| e.path().extension().is_some_and(|ext| ext == "log"))
                .for_each(|e| {
                    let _ = std::fs::remove_file(e.path());
                });
        }
    }

    info!(
        "[main] Starting MyWallpaper Desktop v{} ({}/{})",
        env!("CARGO_PKG_VERSION"),
        std::env::consts::OS,
        std::env::consts::ARCH
    );
    start_with_tauri_webview();
}

fn start_with_tauri_webview() {
    use tauri::{webview::PageLoadEvent, Emitter, Listener, Manager};

    let app = tauri::Builder::default()
        .plugin(
            tauri_plugin_log::Builder::new()
                .level(if cfg!(debug_assertions) {
                    log::LevelFilter::Debug
                } else {
                    log::LevelFilter::Info
                })
                .clear_targets()
                .target(tauri_plugin_log::Target::new(
                    tauri_plugin_log::TargetKind::Webview,
                ))
                .target(tauri_plugin_log::Target::new(
                    tauri_plugin_log::TargetKind::LogDir { file_name: None },
                ))
                .target(tauri_plugin_log::Target::new(
                    tauri_plugin_log::TargetKind::Stdout,
                ))
                .build(),
        )
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec!["--minimized"]),
        ))
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_deep_link::init())
        .plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                args.into_iter()
                    .filter(|a| a.starts_with("mywallpaper://"))
                    .for_each(|url| {
                        let _ = window.emit("deep-link", url);
                    });
            }
        }))
        .on_page_load(|webview, payload| {
            if payload.event() == PageLoadEvent::Started {
                let _ = webview.eval(mw_init_script());
            }
        })
        .setup(|app| {
            let handle = app.handle().clone();
            if let Err(e) = tray::setup_tray(&handle) {
                error!("[setup] Failed to setup system tray: {}", e);
            }

            let deep_link_handle = handle.clone();
            app.listen("deep-link://new-url", move |event| {
                if let Ok(urls) = serde_json::from_str::<Vec<String>>(event.payload()) {
                    if let Some(window) = deep_link_handle.get_webview_window("main") {
                        urls.into_iter()
                            .filter(|u| u.starts_with("mywallpaper://"))
                            .for_each(|url| {
                                let _ = window.emit("deep-link", url);
                            });
                    }
                }
            });

            if let Some(window) = app.get_webview_window("main") {
                let _ = window.set_background_color(Some(tauri::webview::Color(0, 0, 0, 255)));
                window_layer::setup_desktop_window(&window);
                let _ = window.show();
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_system_info,
            commands::check_for_updates,
            commands::download_and_install_update,
            commands::restart_app,
            commands::open_oauth_in_browser,
            commands::reload_window,
            window_layer::set_desktop_icons_visible,
        ])
        .build(tauri::generate_context!())
        .expect("Error while building MyWallpaper Desktop");

    app.run(|_app_handle, event| {
        if let tauri::RunEvent::ExitRequested { .. } | tauri::RunEvent::Exit = event {
            window_layer::restore_desktop_icons_and_unhook();
        }
    });
}
