//! MyWallpaper Desktop Application
//!
//! Tauri backend for the MyWallpaper animated wallpaper application.

mod commands;
mod commands_core;
mod system_monitor;
mod tray;
mod window_layer;

use log::{error, info};
use std::sync::LazyLock;

static MW_INIT_SCRIPT: LazyLock<String> = LazyLock::new(|| {
    format!(
        r#"window.__MW_INIT__ = {{ isTauri: true, platform: "{}", arch: "{}", appVersion: "{}", tauriVersion: "{}", debug: {} }};"#,
        std::env::consts::OS,
        std::env::consts::ARCH,
        env!("CARGO_PKG_VERSION"),
        tauri::VERSION,
        cfg!(debug_assertions),
    )
});

pub fn main() {
    // Clean up old log files, keeping the most recent ones for forensics.
    #[cfg(target_os = "windows")]
    if let Some(local_appdata) = std::env::var_os("LOCALAPPDATA") {
        const MAX_LOG_FILES: usize = 5;
        let base = std::path::Path::new(&local_appdata);
        let log_dir = base.join("com.mywallpaper.desktop").join("logs");
        // Resolve symlinks and verify the log dir is still under LOCALAPPDATA
        if let (Ok(canonical_dir), Ok(canonical_base)) =
            (log_dir.canonicalize(), base.canonicalize())
        {
            if canonical_dir.starts_with(&canonical_base) {
                if let Ok(entries) = std::fs::read_dir(&canonical_dir) {
                    let mut logs: Vec<_> = entries
                        .flatten()
                        .filter(|e| e.path().extension().is_some_and(|ext| ext == "log"))
                        .filter_map(|e| {
                            let modified = e.metadata().ok()?.modified().ok()?;
                            Some((e.path(), modified))
                        })
                        .collect();
                    // Sort newest first, delete everything beyond the retention limit
                    logs.sort_by(|a, b| b.1.cmp(&a.1));
                    for (path, _) in logs.into_iter().skip(MAX_LOG_FILES) {
                        let _ = std::fs::remove_file(path);
                    }
                }
            }
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
        // MacosLauncher is required by the API but inert on Windows
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
                    .filter_map(|a| commands_core::validate_deep_link(&a))
                    .for_each(|url| {
                        let _ = window.emit("deep-link", url);
                    });
            }
        }))
        .on_page_load(|webview, payload| {
            if payload.event() == PageLoadEvent::Started {
                let _ = webview.eval(&*MW_INIT_SCRIPT);
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
                            .filter_map(|u| commands_core::validate_deep_link(&u))
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

            system_monitor::start_monitor(handle.clone(), 3);

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_system_info,
            commands::get_system_data,
            commands::subscribe_system_data,
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
