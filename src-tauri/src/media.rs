//! Media playback info and control via WinRT (Windows only).

use crate::error::{AppError, AppResult};
use serde::Serialize;
use typeshare::typeshare;

#[typeshare]
#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct MediaInfo {
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    /// "playing", "paused", "stopped", "unknown"
    pub playback_status: String,
    pub source_app: Option<String>,
}

/// Get current media playback info from the system.
#[cfg(target_os = "windows")]
pub fn get_media_info() -> AppResult<MediaInfo> {
    use windows::Media::Control::GlobalSystemMediaTransportControlsSessionManager;

    let manager = GlobalSystemMediaTransportControlsSessionManager::RequestAsync()
        .map_err(|e| AppError::Media(format!("RequestAsync failed: {}", e)))?
        .get()
        .map_err(|e| AppError::Media(format!("Manager get failed: {}", e)))?;

    let session = match manager.GetCurrentSession() {
        Ok(s) => s,
        Err(_) => {
            return Ok(MediaInfo {
                playback_status: "stopped".into(),
                ..Default::default()
            })
        }
    };

    let status = session
        .GetPlaybackInfo()
        .ok()
        .and_then(|info| info.PlaybackStatus().ok())
        .map(|s| {
            use windows::Media::Control::GlobalSystemMediaTransportControlsSessionPlaybackStatus;
            match s {
                GlobalSystemMediaTransportControlsSessionPlaybackStatus::Playing => "playing",
                GlobalSystemMediaTransportControlsSessionPlaybackStatus::Paused => "paused",
                GlobalSystemMediaTransportControlsSessionPlaybackStatus::Stopped => "stopped",
                GlobalSystemMediaTransportControlsSessionPlaybackStatus::Closed => "stopped",
                _ => "unknown",
            }
        })
        .unwrap_or("unknown")
        .to_string();

    let props = session
        .TryGetMediaPropertiesAsync()
        .ok()
        .and_then(|a| a.get().ok());

    let (title, artist, album) = match props {
        Some(ref p) => (
            p.Title().ok().map(|s| s.to_string()).filter(|s| !s.is_empty()),
            p.Artist().ok().map(|s| s.to_string()).filter(|s| !s.is_empty()),
            p.AlbumTitle()
                .ok()
                .map(|s| s.to_string())
                .filter(|s| !s.is_empty()),
        ),
        None => (None, None, None),
    };

    let source_app = session
        .SourceAppUserModelId()
        .ok()
        .map(|s| s.to_string())
        .filter(|s| !s.is_empty());

    Ok(MediaInfo {
        title,
        artist,
        album,
        playback_status: status,
        source_app,
    })
}

#[cfg(not(target_os = "windows"))]
pub fn get_media_info() -> AppResult<MediaInfo> {
    Ok(MediaInfo {
        playback_status: "unknown".into(),
        ..Default::default()
    })
}

/// Toggle play/pause on the current media session.
#[cfg(target_os = "windows")]
pub fn media_play_pause() -> AppResult<()> {
    use windows::Media::Control::GlobalSystemMediaTransportControlsSessionManager;

    let manager = GlobalSystemMediaTransportControlsSessionManager::RequestAsync()
        .map_err(|e| AppError::Media(format!("RequestAsync failed: {}", e)))?
        .get()
        .map_err(|e| AppError::Media(format!("Manager get failed: {}", e)))?;

    let session = manager
        .GetCurrentSession()
        .map_err(|e| AppError::Media(format!("No active session: {}", e)))?;

    session
        .TryTogglePlayPauseAsync()
        .map_err(|e| AppError::Media(format!("TogglePlayPause failed: {}", e)))?
        .get()
        .map_err(|e| AppError::Media(format!("TogglePlayPause get failed: {}", e)))?;

    Ok(())
}

#[cfg(not(target_os = "windows"))]
pub fn media_play_pause() -> AppResult<()> {
    Err(AppError::Media("Not supported on this platform".into()))
}

/// Skip to next track.
#[cfg(target_os = "windows")]
pub fn media_next() -> AppResult<()> {
    use windows::Media::Control::GlobalSystemMediaTransportControlsSessionManager;

    let manager = GlobalSystemMediaTransportControlsSessionManager::RequestAsync()
        .map_err(|e| AppError::Media(format!("RequestAsync failed: {}", e)))?
        .get()
        .map_err(|e| AppError::Media(format!("Manager get failed: {}", e)))?;

    let session = manager
        .GetCurrentSession()
        .map_err(|e| AppError::Media(format!("No active session: {}", e)))?;

    session
        .TrySkipNextAsync()
        .map_err(|e| AppError::Media(format!("SkipNext failed: {}", e)))?
        .get()
        .map_err(|e| AppError::Media(format!("SkipNext get failed: {}", e)))?;

    Ok(())
}

#[cfg(not(target_os = "windows"))]
pub fn media_next() -> AppResult<()> {
    Err(AppError::Media("Not supported on this platform".into()))
}

/// Skip to previous track.
#[cfg(target_os = "windows")]
pub fn media_prev() -> AppResult<()> {
    use windows::Media::Control::GlobalSystemMediaTransportControlsSessionManager;

    let manager = GlobalSystemMediaTransportControlsSessionManager::RequestAsync()
        .map_err(|e| AppError::Media(format!("RequestAsync failed: {}", e)))?
        .get()
        .map_err(|e| AppError::Media(format!("Manager get failed: {}", e)))?;

    let session = manager
        .GetCurrentSession()
        .map_err(|e| AppError::Media(format!("No active session: {}", e)))?;

    session
        .TrySkipPreviousAsync()
        .map_err(|e| AppError::Media(format!("SkipPrevious failed: {}", e)))?
        .get()
        .map_err(|e| AppError::Media(format!("SkipPrevious get failed: {}", e)))?;

    Ok(())
}

#[cfg(not(target_os = "windows"))]
pub fn media_prev() -> AppResult<()> {
    Err(AppError::Media("Not supported on this platform".into()))
}
