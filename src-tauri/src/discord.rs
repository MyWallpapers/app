//! Discord Rich Presence — shows "Using MyWallpaper" in Discord.
//! Fails silently if Discord is not running.

use crate::error::AppResult;
use discord_rich_presence::{activity, DiscordIpc, DiscordIpcClient};
use log::{info, warn};
use std::sync::Mutex;

// MyWallpaper Discord application ID (create at https://discord.com/developers/applications)
const DISCORD_APP_ID: &str = "1307092087033782272";

static CLIENT: Mutex<Option<DiscordIpcClient>> = Mutex::new(None);

/// Connect to Discord RPC. Fails silently if Discord is not running.
pub fn init() {
    std::thread::spawn(|| match DiscordIpcClient::new(DISCORD_APP_ID) {
        Ok(mut client) => {
            if client.connect().is_ok() {
                let activity = activity::Activity::new()
                    .state("Animated Wallpaper")
                    .details("Using MyWallpaper")
                    .assets(
                        activity::Assets::new()
                            .large_image("logo")
                            .large_text("MyWallpaper Desktop"),
                    );
                let _ = client.set_activity(activity);
                *CLIENT.lock().unwrap() = Some(client);
                info!("[discord] Rich Presence connected");
            } else {
                warn!("[discord] Discord not running, skipping Rich Presence");
            }
        }
        Err(e) => {
            warn!("[discord] Failed to create IPC client: {}", e);
        }
    });
}

/// Update the Discord Rich Presence activity.
pub fn update_presence(details: &str, state: &str) -> AppResult<()> {
    let mut guard = CLIENT.lock().unwrap();
    if let Some(ref mut client) = *guard {
        let activity = activity::Activity::new()
            .state(state)
            .details(details)
            .assets(
                activity::Assets::new()
                    .large_image("logo")
                    .large_text("MyWallpaper Desktop"),
            );
        client
            .set_activity(activity)
            .map_err(|e| crate::error::AppError::Io(std::io::Error::other(e.to_string())))?;
    }
    Ok(())
}
