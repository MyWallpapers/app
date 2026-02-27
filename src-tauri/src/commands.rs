//! Tauri command handlers

use crate::commands_core;
use crate::error::{AppError, AppResult};
use crate::events::{AppEvent, EmitAppEvent};
use log::info;

use crate::system_monitor;
pub use commands_core::{SystemInfo, UpdateInfo};

#[tauri::command]
pub fn get_system_info() -> SystemInfo {
    commands_core::get_system_info()
}

// ============================================================================
// System Data Commands
// ============================================================================

#[tauri::command]
pub fn get_system_data(categories: Vec<String>) -> system_monitor::SystemData {
    let valid = commands_core::validate_system_categories(&categories);
    system_monitor::collect_system_data(&valid)
}

#[tauri::command]
pub fn subscribe_system_data(categories: Vec<String>) {
    let valid = commands_core::validate_system_categories(&categories);
    system_monitor::set_poll_categories(valid);
}

// ============================================================================
// Auto-Update Commands
// ============================================================================

fn build_updater(
    app: &tauri::AppHandle,
    endpoint: Option<String>,
) -> AppResult<tauri_plugin_updater::Updater> {
    use tauri_plugin_updater::UpdaterExt;
    if let Some(url) = endpoint {
        commands_core::validate_updater_endpoint(&url)?;
        let parsed: url::Url = url
            .parse()
            .map_err(|e| AppError::Updater(format!("Invalid URL: {}", e)))?;
        app.updater_builder()
            .endpoints(vec![parsed])
            .map_err(|e| AppError::Updater(format!("Invalid endpoint: {}", e)))?
            .build()
            .map_err(|e| AppError::Updater(format!("Build failed: {}", e)))
    } else {
        app.updater()
            .map_err(|e| AppError::Updater(format!("Updater not available: {}", e)))
    }
}

#[tauri::command]
pub async fn check_for_updates(
    app: tauri::AppHandle,
    endpoint: Option<String>,
) -> AppResult<Option<UpdateInfo>> {
    let updater = build_updater(&app, endpoint)?;

    match updater.check().await {
        Ok(Some(update)) => {
            // Reject downgrades to prevent rollback attacks
            commands_core::validate_update_version(env!("CARGO_PKG_VERSION"), &update.version)?;
            info!("[updater] Update available: v{}", update.version);
            Ok(Some(UpdateInfo {
                version: update.version.clone(),
                current_version: env!("CARGO_PKG_VERSION").to_string(),
                body: update.body.clone(),
                date: update.date.map(|d| d.to_string()),
            }))
        }
        Ok(None) => Ok(None),
        Err(e) => Err(AppError::Updater(format!("Update check failed: {}", e))),
    }
}

#[tauri::command]
pub async fn download_and_install_update(
    app: tauri::AppHandle,
    endpoint: Option<String>,
) -> AppResult<()> {
    let emit_status = |status: &str| {
        let _ = app.emit_app_event(&AppEvent::UpdateProgress {
            status: status.to_string(),
        });
    };

    emit_status("checking");
    let updater = build_updater(&app, endpoint)?;
    let update = updater
        .check()
        .await
        .map_err(|e| AppError::Updater(format!("Update check failed: {}", e)))?
        .ok_or_else(|| AppError::Updater("No update available".to_string()))?;

    // Reject downgrades to prevent rollback attacks
    commands_core::validate_update_version(env!("CARGO_PKG_VERSION"), &update.version)?;

    emit_status("downloading");
    update
        .download_and_install(
            |_, _| {},
            || info!("[updater] Download complete, installing..."),
        )
        .await
        .map_err(|e| AppError::Updater(format!("Update install failed: {}", e)))?;

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
pub async fn open_oauth_in_browser(app: tauri::AppHandle, url: String) -> AppResult<()> {
    use tauri_plugin_opener::OpenerExt;
    commands_core::validate_oauth_url(&url)?;
    app.opener()
        .open_url(&url, None::<&str>)
        .map_err(|e| AppError::OAuth(format!("Failed to open browser: {}", e)))
}

#[tauri::command]
pub fn reload_window(app: tauri::AppHandle) -> AppResult<()> {
    app.emit_app_event(&AppEvent::ReloadApp)?;
    Ok(())
}

// ============================================================================
// Media Commands
// ============================================================================

#[tauri::command]
pub fn get_media_info() -> AppResult<crate::media::MediaInfo> {
    crate::media::get_media_info()
}

#[tauri::command]
pub fn media_play_pause() -> AppResult<()> {
    crate::media::media_play_pause()
}

#[tauri::command]
pub fn media_next() -> AppResult<()> {
    crate::media::media_next()
}

#[tauri::command]
pub fn media_prev() -> AppResult<()> {
    crate::media::media_prev()
}

// ============================================================================
// Discord Commands
// ============================================================================

#[tauri::command]
pub fn update_discord_presence(details: String, state: String) -> AppResult<()> {
    crate::discord::update_presence(&details, &state)
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
