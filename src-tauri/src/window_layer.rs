//! Window Layer Mode — Desktop vs Interactive
//!
//! Desktop Mode: window placed BEHIND desktop icons (immune to Win+D / Cmd+F3).
//!   - Windows: reparent into WorkerW (behind SHELLDLL_DefView)
//!   - macOS: set window level to kCGDesktopWindowLevel, ignore mouse events
//!
//! Interactive Mode: window on top of everything (current behavior).
//!   - Windows: detach from WorkerW, fullscreen + WS_EX_TOOLWINDOW
//!   - macOS: normal window level, accept mouse events

use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use tauri::{Emitter, Manager};
use tracing::{info, warn};

// ============================================================================
// Types & State
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WindowLayerMode {
    Desktop,
    Interactive,
}

pub struct WindowLayerState {
    pub mode: Mutex<WindowLayerMode>,
}

impl WindowLayerState {
    pub fn new() -> Self {
        Self {
            mode: Mutex::new(WindowLayerMode::Interactive),
        }
    }
}

// ============================================================================
// Tauri Commands
// ============================================================================

/// Set the window layer mode
#[tauri::command]
pub fn set_window_layer(
    app: tauri::AppHandle,
    state: tauri::State<'_, WindowLayerState>,
    mode: WindowLayerMode,
) -> Result<(), String> {
    info!("Setting window layer mode to: {:?}", mode);

    if let Some(window) = app.get_webview_window("main") {
        apply_layer_mode(&window, mode)?;
    }

    let mut current = state.mode.lock().map_err(|e| e.to_string())?;
    *current = mode;

    Ok(())
}

/// Get the current window layer mode
#[tauri::command]
pub fn get_window_layer(
    state: tauri::State<'_, WindowLayerState>,
) -> Result<WindowLayerMode, String> {
    let mode = state.mode.lock().map_err(|e| e.to_string())?;
    Ok(*mode)
}

/// Toggle the window layer mode and return the new mode.
/// Emits "layer-mode-changed" event to the frontend.
#[tauri::command]
pub fn toggle_window_layer(
    app: tauri::AppHandle,
    state: tauri::State<'_, WindowLayerState>,
) -> Result<WindowLayerMode, String> {
    let new_mode = {
        let current = state.mode.lock().map_err(|e| e.to_string())?;
        match *current {
            WindowLayerMode::Desktop => WindowLayerMode::Interactive,
            WindowLayerMode::Interactive => WindowLayerMode::Desktop,
        }
    };

    info!("Toggling window layer mode to: {:?}", new_mode);

    if let Some(window) = app.get_webview_window("main") {
        apply_layer_mode(&window, new_mode)?;
    }

    let mut current = state.mode.lock().map_err(|e| e.to_string())?;
    *current = new_mode;

    // Emit event to frontend
    let _ = app.emit("layer-mode-changed", new_mode);

    Ok(new_mode)
}

// ============================================================================
// Global Shortcut Commands
// ============================================================================

/// Register a global shortcut that toggles the window layer mode
#[tauri::command]
pub fn register_layer_shortcut(app: tauri::AppHandle, shortcut: String) -> Result<(), String> {
    use tauri_plugin_global_shortcut::GlobalShortcutExt;

    info!("Registering layer toggle shortcut: {}", shortcut);

    let parsed: tauri_plugin_global_shortcut::Shortcut = shortcut
        .parse()
        .map_err(|e| format!("Invalid shortcut '{}': {}", shortcut, e))?;

    // Check if already registered
    if app.global_shortcut().is_registered(parsed) {
        info!("Shortcut {} already registered, skipping", shortcut);
        return Ok(());
    }

    app.global_shortcut()
        .on_shortcut(parsed, move |app, _shortcut, event| {
            // Only trigger on key press (not release)
            if event.state == tauri_plugin_global_shortcut::ShortcutState::Pressed {
                let state = app.state::<WindowLayerState>();
                let new_mode = {
                    let current = state.mode.lock().unwrap();
                    match *current {
                        WindowLayerMode::Desktop => WindowLayerMode::Interactive,
                        WindowLayerMode::Interactive => WindowLayerMode::Desktop,
                    }
                };

                info!("Global shortcut triggered, toggling to: {:?}", new_mode);

                if let Some(window) = app.get_webview_window("main") {
                    if let Err(e) = apply_layer_mode(&window, new_mode) {
                        warn!("Failed to apply layer mode: {}", e);
                        return;
                    }
                }

                let mut current = state.mode.lock().unwrap();
                *current = new_mode;

                let _ = app.emit("layer-mode-changed", new_mode);
            }
        })
        .map_err(|e| format!("Failed to register shortcut: {}", e))?;

    info!("Layer toggle shortcut registered: {}", shortcut);
    Ok(())
}

/// Unregister a previously registered layer shortcut
#[tauri::command]
pub fn unregister_layer_shortcut(app: tauri::AppHandle, shortcut: String) -> Result<(), String> {
    use tauri_plugin_global_shortcut::GlobalShortcutExt;

    info!("Unregistering layer shortcut: {}", shortcut);

    let parsed: tauri_plugin_global_shortcut::Shortcut = shortcut
        .parse()
        .map_err(|e| format!("Invalid shortcut '{}': {}", shortcut, e))?;

    app.global_shortcut()
        .unregister(parsed)
        .map_err(|e| format!("Failed to unregister shortcut: {}", e))?;

    Ok(())
}

// ============================================================================
// Platform-specific layer mode application
// ============================================================================

fn apply_layer_mode(window: &tauri::WebviewWindow, mode: WindowLayerMode) -> Result<(), String> {
    match mode {
        WindowLayerMode::Desktop => apply_desktop_mode(window),
        WindowLayerMode::Interactive => apply_interactive_mode(window),
    }
}

// ---- Windows ----------------------------------------------------------------

/// Enumerate ALL top-level WorkerW windows.
/// Returns a list of (HWND, has_shelldll_child) pairs.
#[cfg(target_os = "windows")]
unsafe fn enumerate_worker_ws() -> Vec<(windows::Win32::Foundation::HWND, bool)> {
    use windows::Win32::Foundation::{BOOL, HWND, LPARAM};
    use windows::Win32::UI::WindowsAndMessaging::*;

    struct EnumData {
        workers: Vec<(HWND, bool)>,
    }

    let mut data = EnumData {
        workers: Vec::new(),
    };

    unsafe extern "system" fn enum_callback(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let data = &mut *(lparam.0 as *mut EnumData);

        let mut class_name = [0u16; 256];
        let len = GetClassNameW(hwnd, &mut class_name);
        if len > 0 {
            let name = String::from_utf16_lossy(&class_name[..len as usize]);
            if name == "WorkerW" {
                let has_shelldll = FindWindowExW(
                    hwnd,
                    HWND::default(),
                    windows::core::w!("SHELLDLL_DefView"),
                    None,
                )
                .map_or(false, |h| !h.is_invalid());

                data.workers.push((hwnd, has_shelldll));
            }
        }

        BOOL(1)
    }

    let _ = EnumWindows(
        Some(enum_callback),
        LPARAM(&mut data as *mut EnumData as isize),
    );

    data.workers
}

/// Find the WorkerW window suitable for desktop wallpaper reparenting.
/// This is the WorkerW that does NOT contain SHELLDLL_DefView.
#[cfg(target_os = "windows")]
unsafe fn find_target_worker_w() -> Option<windows::Win32::Foundation::HWND> {
    let workers = enumerate_worker_ws();

    info!("WorkerW scan: found {} WorkerW windows", workers.len());
    for (i, (hwnd, has_shelldll)) in workers.iter().enumerate() {
        info!(
            "  WorkerW[{}]: {:?}, has_shelldll: {}",
            i, hwnd, has_shelldll
        );
    }

    // Target = WorkerW without SHELLDLL_DefView
    workers
        .iter()
        .find(|(_, has_shelldll)| !has_shelldll)
        .map(|(hwnd, _)| *hwnd)
}

/// Send the magic 0x052C message to Progman to spawn WorkerW.
#[cfg(target_os = "windows")]
unsafe fn send_spawn_messages(progman: windows::Win32::Foundation::HWND) {
    use windows::Win32::Foundation::{LPARAM, WPARAM};
    use windows::Win32::UI::WindowsAndMessaging::*;

    let mut result: usize = 0;

    // Method 1: wParam=0xD (standard Win10+)
    let _ = SendMessageTimeoutW(
        progman,
        0x052C,
        WPARAM(0x0000_000D),
        LPARAM(0),
        SMTO_NORMAL,
        1000,
        Some(&mut result),
    );
    let _ = SendMessageTimeoutW(
        progman,
        0x052C,
        WPARAM(0x0000_000D),
        LPARAM(1),
        SMTO_NORMAL,
        1000,
        Some(&mut result),
    );

    // Method 2: wParam=0 (Win11 / alternate shells)
    let _ = SendMessageTimeoutW(
        progman,
        0x052C,
        WPARAM(0),
        LPARAM(0),
        SMTO_NORMAL,
        1000,
        Some(&mut result),
    );
}

/// Reparent our window into the given container (WorkerW or Progman).
#[cfg(target_os = "windows")]
unsafe fn reparent_into(
    our_hwnd: windows::Win32::Foundation::HWND,
    container: windows::Win32::Foundation::HWND,
    container_name: &str,
) -> Result<(), String> {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::*;

    let _ = SetParent(our_hwnd, container);

    // Resize to cover the container
    let mut rect = windows::Win32::Foundation::RECT::default();
    let _ = GetClientRect(container, &mut rect);
    let _ = SetWindowPos(
        our_hwnd,
        HWND::default(),
        0,
        0,
        rect.right - rect.left,
        rect.bottom - rect.top,
        SWP_NOZORDER | SWP_NOACTIVATE | SWP_SHOWWINDOW,
    );

    // Remove WS_EX_TOOLWINDOW since we're behind icons now
    let style = GetWindowLongPtrW(our_hwnd, GWL_EXSTYLE);
    SetWindowLongPtrW(
        our_hwnd,
        GWL_EXSTYLE,
        style & !(WS_EX_TOOLWINDOW.0 as isize),
    );

    info!(
        "Windows: Desktop Mode applied (reparented into {})",
        container_name
    );
    Ok(())
}

#[cfg(target_os = "windows")]
fn apply_desktop_mode(window: &tauri::WebviewWindow) -> Result<(), String> {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::*;

    let our_hwnd = window
        .hwnd()
        .map_err(|e| format!("Failed to get HWND: {}", e))?;
    let our_hwnd = HWND(our_hwnd.0 as *mut core::ffi::c_void);

    // Remove fullscreen BEFORE reparenting to avoid Tauri state conflicts
    let _ = window.set_fullscreen(false);

    unsafe {
        // Phase 1: Check if a suitable WorkerW already exists
        if let Some(worker_w) = find_target_worker_w() {
            info!("Found existing target WorkerW: {:?}", worker_w);
            return reparent_into(our_hwnd, worker_w, "WorkerW");
        }

        // Phase 2: Spawn WorkerW via Progman magic messages, with retries
        let progman = FindWindowW(windows::core::w!("Progman"), None)
            .map_err(|_| "Could not find Progman window".to_string())?;
        if progman.is_invalid() {
            let _ = window.set_fullscreen(true);
            return Err("Could not find Progman window".to_string());
        }
        info!("Found Progman: {:?}", progman);

        for attempt in 1..=5 {
            send_spawn_messages(progman);

            let delay = 200 * attempt;
            info!(
                "WorkerW spawn attempt {}/5, waiting {}ms...",
                attempt, delay
            );
            std::thread::sleep(std::time::Duration::from_millis(delay));

            if let Some(worker_w) = find_target_worker_w() {
                info!(
                    "WorkerW found on attempt {}: {:?}",
                    attempt, worker_w
                );
                return reparent_into(our_hwnd, worker_w, "WorkerW");
            }
        }

        // Phase 3: Progman fallback — reparent directly into Progman.
        // This places our window inside Progman, behind SHELLDLL_DefView (desktop icons).
        warn!("WorkerW spawn failed after 5 attempts, using Progman fallback");
        reparent_into(our_hwnd, progman, "Progman (fallback)")
    }
}

#[cfg(target_os = "windows")]
fn apply_interactive_mode(window: &tauri::WebviewWindow) -> Result<(), String> {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::*;

    let our_hwnd = window
        .hwnd()
        .map_err(|e| format!("Failed to get HWND: {}", e))?;
    let our_hwnd = HWND(our_hwnd.0 as *mut core::ffi::c_void);

    unsafe {
        // 1. Detach from WorkerW/Progman (reparent to desktop/null)
        let _ = SetParent(our_hwnd, HWND::default());

        // 2. Set WS_EX_TOOLWINDOW to hide from taskbar
        let style = GetWindowLongPtrW(our_hwnd, GWL_EXSTYLE);
        SetWindowLongPtrW(our_hwnd, GWL_EXSTYLE, style | WS_EX_TOOLWINDOW.0 as isize);
    }

    // 3. Restore window position to cover primary monitor (same as startup in lib.rs)
    if let Some(monitor) = window.primary_monitor().ok().flatten() {
        let size = monitor.size();
        let position = monitor.position();
        info!(
            "Restoring to primary monitor: {}x{} at ({}, {})",
            size.width, size.height, position.x, position.y
        );
        let _ = window.set_position(tauri::Position::Physical(
            tauri::PhysicalPosition::new(position.x, position.y),
        ));
        let _ = window.set_size(tauri::Size::Physical(tauri::PhysicalSize::new(
            size.width,
            size.height,
        )));
    }

    // 4. Show, focus, then fullscreen
    let _ = window.show();
    let _ = window.set_focus();
    let _ = window.set_fullscreen(true);

    info!("Windows: Interactive Mode applied (detached from WorkerW)");
    Ok(())
}

// ---- macOS ------------------------------------------------------------------

#[cfg(target_os = "macos")]
fn apply_desktop_mode(window: &tauri::WebviewWindow) -> Result<(), String> {
    let ns_window = window
        .ns_window()
        .map_err(|e| format!("Failed to get NSWindow: {}", e))?;

    set_macos_desktop_mode(ns_window);
    info!("macOS: Desktop Mode applied");
    Ok(())
}

#[cfg(target_os = "macos")]
fn apply_interactive_mode(window: &tauri::WebviewWindow) -> Result<(), String> {
    let ns_window = window
        .ns_window()
        .map_err(|e| format!("Failed to get NSWindow: {}", e))?;

    set_macos_interactive_mode(ns_window);
    info!("macOS: Interactive Mode applied");
    Ok(())
}

#[cfg(target_os = "macos")]
fn set_macos_desktop_mode(ns_window_ptr: *mut std::ffi::c_void) {
    use objc::{msg_send, sel, sel_impl};

    unsafe {
        let obj = ns_window_ptr as *mut objc::runtime::Object;
        // kCGDesktopWindowLevel = kCGMinimumWindowLevel + 20 = -2147483628 + 20 = -2147483608
        // But the commonly used value is CGWindowLevelForKey(kCGDesktopWindowLevelKey) which is -2147483623
        let _: () = msg_send![obj, setLevel: -2147483623_i64];
        // canJoinAllSpaces (1) | stationary (16) | ignoresCycle (64) = 81
        let _: () = msg_send![obj, setCollectionBehavior: 81_u64];
        let _: () = msg_send![obj, setIgnoresMouseEvents: true];
    }
}

/// Public helper used by lib.rs during initial setup
#[cfg(target_os = "macos")]
pub fn set_macos_interactive_mode(ns_window_ptr: *mut std::ffi::c_void) {
    use objc::{msg_send, sel, sel_impl};

    unsafe {
        let obj = ns_window_ptr as *mut objc::runtime::Object;
        // NSNormalWindowLevel = 0
        let _: () = msg_send![obj, setLevel: 0_i64];
        // canJoinAllSpaces (1) | stationary (16) | ignoresCycle (64) = 81
        let _: () = msg_send![obj, setCollectionBehavior: 81_u64];
        let _: () = msg_send![obj, setIgnoresMouseEvents: false];
    }

    info!("macOS: Interactive Mode configured");
}

// ---- Linux (paused) ---------------------------------------------------------

#[cfg(target_os = "linux")]
fn apply_desktop_mode(_window: &tauri::WebviewWindow) -> Result<(), String> {
    Err("Window layer mode is not yet supported on Linux".to_string())
}

#[cfg(target_os = "linux")]
fn apply_interactive_mode(_window: &tauri::WebviewWindow) -> Result<(), String> {
    Err("Window layer mode is not yet supported on Linux".to_string())
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mode_serialization() {
        let desktop = serde_json::to_string(&WindowLayerMode::Desktop).unwrap();
        assert_eq!(desktop, "\"desktop\"");

        let interactive = serde_json::to_string(&WindowLayerMode::Interactive).unwrap();
        assert_eq!(interactive, "\"interactive\"");

        let parsed: WindowLayerMode = serde_json::from_str("\"desktop\"").unwrap();
        assert_eq!(parsed, WindowLayerMode::Desktop);
    }

    #[test]
    fn test_state_default() {
        let state = WindowLayerState::new();
        let mode = state.mode.lock().unwrap();
        assert_eq!(*mode, WindowLayerMode::Interactive);
    }
}
