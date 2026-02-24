//! Platform-independent command logic
//!
//! These functions contain the actual business logic, free of `tauri::` types.
//! Tauri command wrappers in `commands.rs` call into these.

use serde::Serialize;
use typeshare::typeshare;

/// System information response
#[typeshare]
#[derive(Debug, Serialize)]
pub struct SystemInfo {
    pub os: String,
    pub os_version: String,
    pub arch: String,
    pub app_version: String,
    pub tauri_version: String,
}

/// Update information response
#[typeshare]
#[derive(Debug, Serialize)]
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
    let parsed = url::Url::parse(endpoint).map_err(|_| "Invalid endpoint URL".to_string())?;
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
    let parsed = url::Url::parse(url_str).map_err(|_| "Invalid URL".to_string())?;

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
    match parsed.host() {
        Some(url::Host::Ipv4(ip)) => {
            if ip.is_private() || ip.is_loopback() || ip.is_link_local() || ip.is_unspecified() {
                return Err("HTTPS to private/internal IPs is not allowed".into());
            }
        }
        Some(url::Host::Ipv6(ip)) => {
            if ip.is_loopback() || ip.is_unspecified() {
                return Err("HTTPS to private/internal IPs is not allowed".into());
            }
            let segs = ip.segments();
            // fc00::/7 — unique local
            if segs[0] & 0xfe00 == 0xfc00 {
                return Err("HTTPS to private/internal IPs is not allowed".into());
            }
            // fe80::/10 — link-local
            if segs[0] & 0xffc0 == 0xfe80 {
                return Err("HTTPS to private/internal IPs is not allowed".into());
            }
            // ::ffff:0:0/96 — IPv4-mapped (check underlying IPv4)
            if let Some(ipv4) = ip.to_ipv4_mapped() {
                if ipv4.is_private()
                    || ipv4.is_loopback()
                    || ipv4.is_link_local()
                    || ipv4.is_unspecified()
                {
                    return Err("HTTPS to private/internal IPs is not allowed".into());
                }
            }
        }
        _ => {}
    }

    Ok(())
}

// ============================================================================
// Update Version Validation
// ============================================================================

/// Reject updates that would downgrade to an older version.
/// Compares semver-style version strings (major.minor.patch).
pub fn validate_update_version(current: &str, candidate: &str) -> Result<(), String> {
    let parse = |v: &str| -> Result<(u32, u32, u32), String> {
        let v = v.trim_start_matches('v');
        // Strip any pre-release suffix (e.g., "1.0.0-dev")
        let v = v.split('-').next().unwrap_or(v);
        let parts: Vec<&str> = v.split('.').collect();
        if parts.len() != 3 {
            return Err(format!("Invalid version format: {}", v));
        }
        Ok((
            parts[0].parse().map_err(|_| "Invalid major version")?,
            parts[1].parse().map_err(|_| "Invalid minor version")?,
            parts[2].parse().map_err(|_| "Invalid patch version")?,
        ))
    };
    let current = parse(current)?;
    let candidate = parse(candidate)?;
    if candidate < current {
        return Err(format!(
            "Refusing downgrade from {}.{}.{} to {}.{}.{}",
            current.0, current.1, current.2, candidate.0, candidate.1, candidate.2
        ));
    }
    Ok(())
}

// ============================================================================
// Deep-Link Validation
// ============================================================================

/// Allowed deep-link actions (the "host" part of `mywallpaper://<action>/...`).
const ALLOWED_DEEP_LINK_ACTIONS: &[&str] = &["callback", "auth", "oauth", "login", "app"];

/// Validate and sanitize a `mywallpaper://` deep-link URL.
/// Returns the sanitized URL or None if invalid.
pub fn validate_deep_link(raw: &str) -> Option<String> {
    let parsed = url::Url::parse(raw).ok()?;
    if parsed.scheme() != "mywallpaper" {
        return None;
    }
    // In custom scheme URLs, the "host" is the action/route (e.g., mywallpaper://callback/...)
    if let Some(host) = parsed.host_str() {
        if !host.is_empty() && !ALLOWED_DEEP_LINK_ACTIONS.contains(&host) {
            return None;
        }
    }
    // Return the parsed (normalized) URL to strip any injection attempts
    Some(parsed.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- Updater endpoint validation ----

    #[test]
    fn test_valid_updater_endpoint() {
        assert!(validate_updater_endpoint(
            "https://github.com/MyWallpapers/app/releases/download/v1.0.0/latest.json"
        )
        .is_ok());
    }

    #[test]
    fn test_valid_updater_endpoint_dev_tag() {
        assert!(validate_updater_endpoint(
            "https://github.com/MyWallpapers/app/releases/download/v1.0.0-dev/latest.json"
        )
        .is_ok());
    }

    #[test]
    fn test_updater_rejects_http() {
        assert!(validate_updater_endpoint(
            "http://github.com/MyWallpapers/app/releases/download/v1.0.0/latest.json"
        )
        .is_err());
    }

    #[test]
    fn test_updater_rejects_wrong_host() {
        assert!(validate_updater_endpoint(
            "https://evil.com/MyWallpapers/app/releases/download/v1.0.0/latest.json"
        )
        .is_err());
    }

    #[test]
    fn test_updater_rejects_wrong_path() {
        assert!(validate_updater_endpoint(
            "https://github.com/evil/repo/releases/download/v1.0.0/latest.json"
        )
        .is_err());
    }

    #[test]
    fn test_updater_rejects_garbage() {
        assert!(validate_updater_endpoint("not a url").is_err());
    }

    // ---- OAuth URL validation ----

    #[test]
    fn test_validate_oauth_url_https() {
        assert!(
            validate_oauth_url("https://accounts.google.com/o/oauth2/auth?client_id=123").is_ok()
        );
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

    #[test]
    fn test_validate_oauth_url_rejects_ipv6_private() {
        // Loopback
        assert!(validate_oauth_url("https://[::1]/callback").is_err());
        // Unique local (fc00::/7)
        assert!(validate_oauth_url("https://[fd12::1]/callback").is_err());
        assert!(validate_oauth_url("https://[fc00::1]/callback").is_err());
        // Link-local (fe80::/10)
        assert!(validate_oauth_url("https://[fe80::1]/callback").is_err());
        // Unspecified
        assert!(validate_oauth_url("https://[::]/callback").is_err());
        // IPv4-mapped private
        assert!(validate_oauth_url("https://[::ffff:10.0.0.1]/callback").is_err());
        assert!(validate_oauth_url("https://[::ffff:192.168.1.1]/callback").is_err());
        assert!(validate_oauth_url("https://[::ffff:127.0.0.1]/callback").is_err());
    }

    #[test]
    fn test_validate_oauth_url_allows_public_ipv6() {
        assert!(validate_oauth_url("https://[2607:f8b0:4004:800::200e]/callback").is_ok());
    }

    // ---- Deep-link validation ----

    #[test]
    fn test_deep_link_valid_paths() {
        assert!(validate_deep_link("mywallpaper://callback?code=abc").is_some());
        assert!(validate_deep_link("mywallpaper://auth/complete").is_some());
        assert!(validate_deep_link("mywallpaper://oauth/google?token=x").is_some());
        assert!(validate_deep_link("mywallpaper://login").is_some());
        assert!(validate_deep_link("mywallpaper://app/settings").is_some());
        assert!(validate_deep_link("mywallpaper://").is_some());
        assert!(validate_deep_link("mywallpaper:///").is_some());
    }

    #[test]
    fn test_deep_link_rejects_invalid() {
        // Wrong scheme
        assert!(validate_deep_link("https://evil.com").is_none());
        assert!(validate_deep_link("javascript:alert(1)").is_none());
        // Unknown path
        assert!(validate_deep_link("mywallpaper://evil/path").is_none());
        assert!(validate_deep_link("mywallpaper://admin/delete").is_none());
        // Garbage
        assert!(validate_deep_link("not a url").is_none());
    }

    // ---- Update version validation ----

    #[test]
    fn test_update_version_allows_upgrade() {
        assert!(validate_update_version("1.0.0", "1.0.1").is_ok());
        assert!(validate_update_version("1.0.0", "1.1.0").is_ok());
        assert!(validate_update_version("1.0.0", "2.0.0").is_ok());
        assert!(validate_update_version("1.0.223", "1.0.224").is_ok());
    }

    #[test]
    fn test_update_version_allows_same() {
        assert!(validate_update_version("1.0.0", "1.0.0").is_ok());
    }

    #[test]
    fn test_update_version_rejects_downgrade() {
        assert!(validate_update_version("1.0.224", "1.0.223").is_err());
        assert!(validate_update_version("2.0.0", "1.9.9").is_err());
        assert!(validate_update_version("1.1.0", "1.0.99").is_err());
    }

    #[test]
    fn test_update_version_handles_prefixes() {
        assert!(validate_update_version("1.0.0", "v1.0.1").is_ok());
        assert!(validate_update_version("v1.0.0", "1.0.1").is_ok());
        assert!(validate_update_version("1.0.1-dev", "1.0.1").is_ok());
    }
}
