//! Platform-independent command logic
//!
//! These functions contain the actual business logic, free of `tauri::` types.
//! Tauri command wrappers in `commands.rs` call into these.

use serde::{Deserialize, Serialize};
use typeshare::typeshare;

/// System information response
#[typeshare]
#[derive(Debug, Serialize, Deserialize)]
pub struct SystemInfo {
    pub os: String,
    pub os_version: String,
    pub arch: String,
    pub app_version: String,
    pub tauri_version: String,
}

/// Update information response
#[typeshare]
#[derive(Debug, Serialize, Deserialize)]
pub struct UpdateInfo {
    pub version: String,
    pub current_version: String,
    pub body: Option<String>,
    pub date: Option<String>,
}

// ============================================================================
// System Information
// ============================================================================

/// Get system information (platform-independent)
pub fn get_system_info() -> SystemInfo {
    SystemInfo {
        os: std::env::consts::OS.to_string(),
        os_version: os_info::get().version().to_string(),
        arch: std::env::consts::ARCH.to_string(),
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        tauri_version: tauri::VERSION.to_string(),
    }
}

// ============================================================================
// Updater Endpoint Validation
// ============================================================================

const ALLOWED_UPDATER_HOST: &str = "github.com";
const ALLOWED_UPDATER_PATH_PREFIX: &str = "/MyWallpapers/app/releases/download/";

/// Validate that an updater endpoint URL points to our GitHub releases.
pub fn validate_updater_endpoint(endpoint: &str) -> Result<(), String> {
    let parsed = url::Url::parse(endpoint)
        .map_err(|_| "Invalid endpoint URL".to_string())?;
    if parsed.scheme() != "https" {
        return Err("Endpoint must use HTTPS".into());
    }
    if parsed.host_str() != Some(ALLOWED_UPDATER_HOST) {
        return Err("Endpoint must be on github.com".into());
    }
    if !parsed.path().starts_with(ALLOWED_UPDATER_PATH_PREFIX) {
        return Err("Endpoint must point to MyWallpapers/app releases".into());
    }
    Ok(())
}

// ============================================================================
// OAuth
// ============================================================================

/// Validate an OAuth URL:
/// - Must be valid HTTPS, or HTTP only for localhost/127.0.0.1
/// - Blocks private/internal IP ranges (SSRF prevention)
pub fn validate_oauth_url(url_str: &str) -> Result<(), String> {
    let parsed = url::Url::parse(url_str)
        .map_err(|_| "Invalid URL".to_string())?;

    match parsed.scheme() {
        "https" => {}
        "http" => {
            let host = parsed.host_str().unwrap_or("");
            if host != "localhost" && host != "127.0.0.1" && host != "[::1]" {
                return Err("HTTP is only allowed for localhost".into());
            }
            return Ok(());
        }
        _ => return Err("URL must use https:// (or http:// for localhost)".into()),
    }

    // Block private/internal IPs via HTTPS (SSRF)
    if let Some(url::Host::Ipv4(ip)) = parsed.host() {
        if ip.is_private() || ip.is_loopback() || ip.is_link_local() || ip.is_unspecified() {
            return Err("HTTPS to private/internal IPs is not allowed".into());
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- Updater endpoint validation ----

    #[test]
    fn test_valid_updater_endpoint() {
        assert!(validate_updater_endpoint(
            "https://github.com/MyWallpapers/app/releases/download/v1.0.0/latest.json"
        ).is_ok());
    }

    #[test]
    fn test_valid_updater_endpoint_dev_tag() {
        assert!(validate_updater_endpoint(
            "https://github.com/MyWallpapers/app/releases/download/v1.0.0-dev/latest.json"
        ).is_ok());
    }

    #[test]
    fn test_updater_rejects_http() {
        assert!(validate_updater_endpoint(
            "http://github.com/MyWallpapers/app/releases/download/v1.0.0/latest.json"
        ).is_err());
    }

    #[test]
    fn test_updater_rejects_wrong_host() {
        assert!(validate_updater_endpoint(
            "https://evil.com/MyWallpapers/app/releases/download/v1.0.0/latest.json"
        ).is_err());
    }

    #[test]
    fn test_updater_rejects_wrong_path() {
        assert!(validate_updater_endpoint(
            "https://github.com/evil/repo/releases/download/v1.0.0/latest.json"
        ).is_err());
    }

    #[test]
    fn test_updater_rejects_garbage() {
        assert!(validate_updater_endpoint("not a url").is_err());
    }

    // ---- OAuth URL validation ----

    #[test]
    fn test_validate_oauth_url_https() {
        assert!(validate_oauth_url("https://accounts.google.com/o/oauth2/auth?client_id=123").is_ok());
    }

    #[test]
    fn test_validate_oauth_url_localhost_http() {
        assert!(validate_oauth_url("http://localhost:3000/callback").is_ok());
        assert!(validate_oauth_url("http://127.0.0.1:8080/callback").is_ok());
    }

    #[test]
    fn test_validate_oauth_url_rejects_non_localhost_http() {
        assert!(validate_oauth_url("http://evil.com/phish").is_err());
        assert!(validate_oauth_url("http://example.com").is_err());
    }

    #[test]
    fn test_validate_oauth_url_rejects_private_ips() {
        assert!(validate_oauth_url("https://10.0.0.1/callback").is_err());
        assert!(validate_oauth_url("https://192.168.1.1/callback").is_err());
        assert!(validate_oauth_url("https://172.16.0.1/callback").is_err());
    }

    #[test]
    fn test_validate_oauth_url_rejects_schemes() {
        assert!(validate_oauth_url("ftp://example.com").is_err());
        assert!(validate_oauth_url("javascript:alert(1)").is_err());
        assert!(validate_oauth_url("data:text/html,<h1>hi</h1>").is_err());
        assert!(validate_oauth_url("file:///etc/passwd").is_err());
    }

    #[test]
    fn test_validate_oauth_url_rejects_garbage() {
        assert!(validate_oauth_url("not a url").is_err());
        assert!(validate_oauth_url("").is_err());
    }
}
