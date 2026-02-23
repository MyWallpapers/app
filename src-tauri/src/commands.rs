//! Tauri command handlers

use crate::commands_core;
use tauri::Emitter;
use log::info;

pub use commands_core::{SystemInfo, UpdateInfo};

#[tauri::command]
pub fn get_system_info() -> SystemInfo {
    commands_core::get_system_info()
}

// ============================================================================
// Auto-Update Commands
// ============================================================================

fn build_updater(app: &tauri::AppHandle, endpoint: Option<String>) -> Result<tauri_plugin_updater::Updater, String> {
    use tauri_plugin_updater::UpdaterExt;
    if let Some(url) = endpoint {
        let parsed: url::Url = url.parse().map_err(|e| format!("Invalid URL: {}", e))?;
        app.updater_builder()
            .endpoints(vec![parsed])
            .map_err(|e| format!("Invalid endpoint: {}", e))?
            .build()
            .map_err(|e| format!("Build failed: {}", e))
    } else {
        app.updater().map_err(|e| format!("Updater not available: {}", e))
    }
}

#[tauri::command]
pub async fn check_for_updates(app: tauri::AppHandle, endpoint: Option<String>) -> Result<Option<UpdateInfo>, String> {
    let updater = build_updater(&app, endpoint)?;

    match updater.check().await {
        Ok(Some(update)) => {
            info!("[updater] Update available: v{}", update.version);
            Ok(Some(UpdateInfo {
                version: update.version.clone(),
                current_version: env!("CARGO_PKG_VERSION").to_string(),
                body: update.body.clone(),
                date: update.date.map(|d| d.to_string()),
            }))
        }
        Ok(None) => Ok(None),
        Err(e) => Err(format!("Update check failed: {}", e)),
    }
}

#[tauri::command]
pub async fn download_and_install_update(app: tauri::AppHandle, endpoint: Option<String>) -> Result<(), String> {
    use tauri::Manager;

    let emit_status = |status: &str| {
        if let Some(window) = app.get_webview_window("main") {
            let _ = window.emit("update-progress", status);
        }
    };

    emit_status("checking");
    let updater = build_updater(&app, endpoint)?;
    let update = updater.check().await
        .map_err(|e| format!("Update check failed: {}", e))?
        .ok_or_else(|| "No update available".to_string())?;

    emit_status("downloading");
    update.download_and_install(
        |_, _| {},
        || info!("[updater] Download complete, installing..."),
    ).await.map_err(|e| format!("Update install failed: {}", e))?;

    emit_status("installed");
    Ok(())
}

#[tauri::command]
pub fn restart_app(app: tauri::AppHandle) {
    app.restart();
}

// ============================================================================
// OAuth & Window Commands
// ============================================================================

#[tauri::command]
pub async fn open_oauth_in_browser(app: tauri::AppHandle, url: String) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    commands_core::validate_oauth_url(&url)?;
    app.opener().open_url(&url, None::<&str>).map_err(|e| format!("Failed to open browser: {}", e))
}

#[tauri::command]
pub fn reload_window(app: tauri::AppHandle) -> Result<(), String> {
    use tauri::Manager;
    if let Some(window) = app.get_webview_window("main") {
        window.emit("reload-app", ()).map_err(|e| format!("Failed to emit event: {}", e))
    } else {
        Err("Main window not found".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_system_info() {
        let info = get_system_info();
        assert!(!info.os.is_empty());
        assert!(!info.arch.is_empty());
        assert!(!info.app_version.is_empty());
    }
}
