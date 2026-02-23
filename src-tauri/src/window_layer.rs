//! Window Layer — Desktop WebView injection + mouse forwarding (Windows only).

#[cfg(target_os = "windows")]
use log::{error, info};
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(target_os = "windows")]
use std::sync::atomic::AtomicIsize;

static ICONS_RESTORED: AtomicBool = AtomicBool::new(false);
#[cfg(target_os = "windows")]
static HOOK_HANDLE_GLOBAL: AtomicIsize = AtomicIsize::new(0);

// ==============================================================================
// Public API
// ==============================================================================

pub fn setup_desktop_window(_window: &tauri::WebviewWindow) {
    #[cfg(target_os = "windows")]
    {
        info!("[window_layer] Starting desktop window setup phase...");
        if let Err(e) = ensure_in_worker_w(_window) {
            error!("[window_layer] CRITICAL: Failed to setup desktop layer: {}", e);
        } else {
            info!("[window_layer] Desktop layer setup completed successfully.");
        }
    }
}

#[tauri::command]
pub fn set_desktop_icons_visible(_visible: bool) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        use windows::Win32::Foundation::HWND;
        use windows::Win32::UI::WindowsAndMessaging::{ShowWindow, SW_HIDE, SW_SHOW};
        let slv = mouse_hook::get_syslistview_hwnd();
        if slv != 0 {
            unsafe {
                let _ = ShowWindow(HWND(slv as *mut _), if _visible { SW_SHOW } else { SW_HIDE });
            }
            info!("[window_layer] Desktop icons visibility set to {}", _visible);
        }
    }
    Ok(())
}

pub fn restore_desktop_icons_and_unhook() {
    if ICONS_RESTORED.swap(true, Ordering::SeqCst) { return; }
    #[cfg(target_os = "windows")]
    {
        use windows::Win32::Foundation::HWND;
        use windows::Win32::UI::WindowsAndMessaging::{ShowWindow, SW_SHOW, UnhookWindowsHookEx, HHOOK};

        let slv = mouse_hook::get_syslistview_hwnd();
        if slv != 0 {
            unsafe { let _ = ShowWindow(HWND(slv as *mut _), SW_SHOW); }
        }

        let hook_ptr = HOOK_HANDLE_GLOBAL.load(Ordering::SeqCst);
        if hook_ptr != 0 {
            unsafe { let _ = UnhookWindowsHookEx(HHOOK(hook_ptr as *mut _)); }
        }
    }
}

// ==============================================================================
// Windows: Desktop Detection (X-Ray Validated Logic)
// ==============================================================================

#[cfg(target_os = "windows")]
struct DesktopDetection {
    progman: windows::Win32::Foundation::HWND,
    explorer_pid: u32,
    target_parent: windows::Win32::Foundation::HWND,
    shell_view: windows::Win32::Foundation::HWND,
    syslistview: windows::Win32::Foundation::HWND,
    v_x: i32,
    v_y: i32,
    v_width: i32,
    v_height: i32,
}

#[cfg(target_os = "windows")]
fn detect_desktop() -> Result<DesktopDetection, String> {
    use windows::Win32::Foundation::{BOOL, HWND, LPARAM, RECT, WPARAM};
    use windows::Win32::Graphics::Gdi::{EnumDisplayMonitors, HDC, HMONITOR};
    use windows::Win32::UI::WindowsAndMessaging::*;

    unsafe {
        let progman = FindWindowW(windows::core::w!("Progman"), None)
            .map_err(|_| "Could not find Progman.".to_string())?;

        let mut explorer_pid: u32 = 0;
        GetWindowThreadProcessId(progman, Some(&mut explorer_pid));
        info!("[detect_desktop] Progman 0x{:X} belongs to explorer.exe pid={}", progman.0 as isize, explorer_pid);

        // Force Windows to spawn the wallpaper WorkerW layer
        let mut msg_result: usize = 0;
        let _ = SendMessageTimeoutW(progman, 0x052C, WPARAM(0x0D), LPARAM(1), SMTO_NORMAL, 1000, Some(&mut msg_result));

        // Let Windows breathe and spawn the window
        std::thread::sleep(std::time::Duration::from_millis(150));

        let mut target_parent = HWND::default();
        let mut shell_view = HWND::default();

        // 1. Detection Win11 24H2+ (Based on X-Ray logs)
        // SHELLDLL_DefView and WorkerW are direct children of Progman
        shell_view = FindWindowExW(progman, HWND::default(), windows::core::w!("SHELLDLL_DefView"), None).unwrap_or_default();

        if !shell_view.is_invalid() {
            target_parent = FindWindowExW(progman, HWND::default(), windows::core::w!("WorkerW"), None).unwrap_or_default();
            info!("[detect_desktop] 24H2+ architecture identified. Target WorkerW: 0x{:X}", target_parent.0 as isize);
        } else {
            // 2. Fallback to standard Win10/Win11
            struct SearchData { parent: HWND, sv: HWND }
            let mut data = SearchData { parent: HWND::default(), sv: HWND::default() };

            unsafe extern "system" fn enum_cb(hwnd: HWND, lp: LPARAM) -> BOOL {
                let sv = FindWindowExW(hwnd, HWND::default(), windows::core::w!("SHELLDLL_DefView"), None).unwrap_or_default();
                if !sv.is_invalid() {
                    let d = &mut *(lp.0 as *mut SearchData);
                    d.sv = sv;
                    d.parent = FindWindowExW(HWND::default(), hwnd, windows::core::w!("WorkerW"), None).unwrap_or_default();
                    return BOOL(0);
                }
                BOOL(1)
            }
            let _ = EnumWindows(Some(enum_cb), LPARAM(&mut data as *mut _ as isize));
            shell_view = data.sv;
            target_parent = data.parent;
            info!("[detect_desktop] Legacy architecture identified. Target WorkerW: 0x{:X}", target_parent.0 as isize);
        }

        if target_parent.is_invalid() {
            info!("[detect_desktop] WorkerW completely missing. Falling back to Progman root.");
            target_parent = progman;
        }

        let mut syslistview = HWND::default();
        unsafe extern "system" fn find_slv(hwnd: HWND, lp: LPARAM) -> BOOL {
            let mut buf = [0u16; 64];
            let len = GetClassNameW(hwnd, &mut buf);
            if String::from_utf16_lossy(&buf[..len as usize]) == "SysListView32" {
                *(lp.0 as *mut HWND) = hwnd;
                return BOOL(0);
            }
            BOOL(1)
        }
        let _ = EnumChildWindows(shell_view, Some(find_slv), LPARAM(&mut syslistview as *mut _ as isize));

        // Absolute Physical Bounds (Fixes the 120x0 size bug)
        struct MonitorRects { left: i32, top: i32, right: i32, bottom: i32 }
        let mut m_rects = MonitorRects { left: 0, top: 0, right: 0, bottom: 0 };
        unsafe extern "system" fn monitor_enum_cb(_hm: HMONITOR, _hdc: HDC, rect: *mut RECT, lparam: LPARAM) -> BOOL {
            let data = &mut *(lparam.0 as *mut MonitorRects);
            if rect.read().left < data.left { data.left = rect.read().left; }
            if rect.read().top < data.top { data.top = rect.read().top; }
            if rect.read().right > data.right { data.right = rect.read().right; }
            if rect.read().bottom > data.bottom { data.bottom = rect.read().bottom; }
            BOOL(1)
        }
        let _ = EnumDisplayMonitors(HDC::default(), None, Some(monitor_enum_cb), LPARAM(&mut m_rects as *mut _ as isize));

        let width = m_rects.right - m_rects.left;
        let height = m_rects.bottom - m_rects.top;
        info!("[detect_desktop] Enforced Physical Screen Bounds: {}x{} at {},{}", width, height, m_rects.left, m_rects.top);

        Ok(DesktopDetection {
            progman,
            explorer_pid,
            target_parent,
            shell_view,
            syslistview,
            v_x: m_rects.left,
            v_y: m_rects.top,
            v_width: width,
            v_height: height,
        })
    }
}

// ==============================================================================
// Windows: Injection Execution
// ==============================================================================

#[cfg(target_os = "windows")]
fn apply_injection(our_hwnd: windows::Win32::Foundation::HWND, detection: &DesktopDetection) {
    use windows::Win32::Foundation::{HWND, RECT};
    use windows::Win32::UI::WindowsAndMessaging::*;

    unsafe {
        let current_parent = GetParent(our_hwnd).unwrap_or_default();
        if current_parent == detection.target_parent { return; }

        let mut style = GetWindowLongW(our_hwnd, GWL_STYLE) as u32;
        style &= !(WS_THICKFRAME.0 | WS_CAPTION.0 | WS_SYSMENU.0 | WS_MAXIMIZEBOX.0 | WS_MINIMIZEBOX.0 | WS_POPUP.0);
        style |= WS_CHILD.0 | WS_VISIBLE.0;
        let _ = SetWindowLongW(our_hwnd, GWL_STYLE, style as i32);

        let mut ex_style = GetWindowLongW(our_hwnd, GWL_EXSTYLE) as u32;
        ex_style &= !WS_EX_LAYERED.0;
        ex_style &= !WS_EX_NOACTIVATE.0;
        // Remove all border-producing extended styles so no visible
        // gaps appear between the WebView content and the window edge.
        ex_style &= !WS_EX_CLIENTEDGE.0;
        ex_style &= !WS_EX_WINDOWEDGE.0;
        ex_style &= !WS_EX_DLGMODALFRAME.0;
        ex_style &= !WS_EX_STATICEDGE.0;
        let _ = SetWindowLongW(our_hwnd, GWL_EXSTYLE, ex_style as i32);

        // Disable the DWM thin border that Windows 11 adds to borderless windows
        use windows::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWA_BORDER_COLOR};
        let no_border: u32 = 0xFFFFFFFE; // DWMWA_COLOR_NONE
        let _ = DwmSetWindowAttribute(
            our_hwnd,
            DWMWA_BORDER_COLOR,
            &no_border as *const u32 as *const _,
            std::mem::size_of::<u32>() as u32,
        );

        let _ = ShowWindow(detection.target_parent, SW_SHOW);
        let _ = SetParent(our_hwnd, detection.target_parent);

        // Z-order is already correct: we're a child of WorkerW (behind
        // SHELLDLL_DefView in the Progman hierarchy). Use SWP_NOZORDER to
        // skip invalid cross-parent z-order, which caused SetWindowPos to
        // silently fail and leave the window at default size.
        let _ = SetWindowPos(
            our_hwnd, HWND::default(),
            0, 0, detection.v_width, detection.v_height,
            SWP_FRAMECHANGED | SWP_SHOWWINDOW | SWP_NOZORDER,
        );

        let _ = ShowWindow(our_hwnd, SW_SHOW);

        info!("[apply_injection] Injection Complete. Parent=0x{:X}, Size={}x{} at {},{}",
            detection.target_parent.0 as isize, detection.v_width, detection.v_height,
            detection.v_x, detection.v_y);

        // Verify actual window rects after injection
        let mut parent_rect = RECT::default();
        let mut our_rect = RECT::default();
        let _ = GetWindowRect(detection.target_parent, &mut parent_rect);
        let _ = GetWindowRect(our_hwnd, &mut our_rect);
        info!("[apply_injection] Parent RECT: ({},{})→({},{}) = {}x{}",
            parent_rect.left, parent_rect.top, parent_rect.right, parent_rect.bottom,
            parent_rect.right - parent_rect.left, parent_rect.bottom - parent_rect.top);
        info!("[apply_injection] Our RECT: ({},{})→({},{}) = {}x{}",
            our_rect.left, our_rect.top, our_rect.right, our_rect.bottom,
            our_rect.right - our_rect.left, our_rect.bottom - our_rect.top);
    }
}

// ==============================================================================
// Windows: Initialization
// ==============================================================================

#[cfg(target_os = "windows")]
fn ensure_in_worker_w(window: &tauri::WebviewWindow) -> Result<(), String> {
    use windows::Win32::Foundation::HWND;

    // false is safe: our window sits behind SHELLDLL_DefView in Z-order,
    // so the OS won't deliver native mouse events to it anyway.
    // All mouse input comes through the low-level hook → SendMouseInput.
    let _ = window.set_ignore_cursor_events(false);

    let our_hwnd_raw = window.hwnd().map_err(|e| format!("{}", e))?;
    let our_hwnd = HWND(our_hwnd_raw.0 as *mut _);

    let detection = detect_desktop()?;

    mouse_hook::set_webview_hwnd(our_hwnd.0 as isize);
    mouse_hook::set_target_parent_hwnd(detection.target_parent.0 as isize);
    mouse_hook::set_progman_hwnd(detection.progman.0 as isize);
    mouse_hook::set_explorer_pid(detection.explorer_pid);
    info!("[ensure_in_worker_w] Progman=0x{:X}, explorer_pid={}", detection.progman.0 as isize, detection.explorer_pid);
    if !detection.syslistview.is_invalid() {
        mouse_hook::set_syslistview_hwnd(detection.syslistview.0 as isize);
    }

    apply_injection(our_hwnd, &detection);

    mouse_hook::init_dispatch_window();
    info!("[ensure_in_worker_w] Dispatch window created. HWND: 0x{:X}", mouse_hook::get_dispatch_hwnd());

    let (w, h) = (detection.v_width, detection.v_height);

    let our_hwnd_isize = our_hwnd.0 as isize;
    std::thread::spawn(move || {
        use windows::Win32::Foundation::{HWND, LPARAM, RECT, WPARAM};
        use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, SetWindowPos, GetWindowRect, SWP_NOZORDER, SWP_SHOWWINDOW};

        for attempt in 1..=100 {
            let ptr = wry::get_last_composition_controller_ptr();
            if ptr != 0 {
                info!("[WRY_POLL] CompositionController acquired at 0x{:X} on attempt {}", ptr, attempt);
                mouse_hook::set_comp_controller_ptr(ptr);
                let dh = mouse_hook::get_dispatch_hwnd();
                if dh != 0 {
                    unsafe {
                        let _ = PostMessageW(HWND(dh as *mut _), mouse_hook::WM_MWP_SETBOUNDS_PUB, WPARAM(w as usize), LPARAM(h as isize));
                    }
                    info!("[WRY_POLL] Bounds set to {}x{}", w, h);
                }

                // Force outer window to full size — Tauri/WebView2 init may have resized it.
                unsafe {
                    let wv_h = HWND(our_hwnd_isize as *mut _);
                    let _ = SetWindowPos(wv_h, HWND::default(), 0, 0, w, h, SWP_NOZORDER | SWP_SHOWWINDOW);
                    let mut rect = RECT::default();
                    let _ = GetWindowRect(wv_h, &mut rect);
                    info!("[WRY_POLL] Post-resize RECT: ({},{})→({},{}) = {}x{}",
                        rect.left, rect.top, rect.right, rect.bottom,
                        rect.right - rect.left, rect.bottom - rect.top);
                }
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
    });

    mouse_hook::start_hook_thread();

    Ok(())
}

// ==============================================================================
// Windows: Mouse Hook
// ==============================================================================

#[cfg(target_os = "windows")]
pub mod mouse_hook {
    use log::{error, info};
    use std::sync::atomic::{AtomicIsize, AtomicU32, Ordering};
    use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
    use windows::Win32::UI::WindowsAndMessaging::*;

    const MOUSE_MOVE: i32 = 0x0200; const MOUSE_LDOWN: i32 = 0x0201; const MOUSE_LUP: i32 = 0x0202;
    const MOUSE_RDOWN: i32 = 0x0204; const MOUSE_RUP: i32 = 0x0205; const MOUSE_MDOWN: i32 = 0x0207;
    const MOUSE_MUP: i32 = 0x0208; const MOUSE_WHEEL: i32 = 0x020A; const MOUSE_HWHEEL: i32 = 0x020E;
    const VK_NONE: i32 = 0x0; const VK_LBUTTON: i32 = 0x1; const VK_RBUTTON: i32 = 0x2; const VK_MBUTTON: i32 = 0x10;

    static WEBVIEW_HWND: AtomicIsize = AtomicIsize::new(0);
    static SYSLISTVIEW_HWND: AtomicIsize = AtomicIsize::new(0);
    static TARGET_PARENT_HWND: AtomicIsize = AtomicIsize::new(0);
    static PROGMAN_HWND: AtomicIsize = AtomicIsize::new(0);
    static EXPLORER_PID: AtomicU32 = AtomicU32::new(0);
    static DESKTOP_CORE_HWND: AtomicIsize = AtomicIsize::new(0);
    static COMP_CONTROLLER_PTR: AtomicIsize = AtomicIsize::new(0);
    static DRAG_VK: AtomicIsize = AtomicIsize::new(0);
    static DISPATCH_HWND: AtomicIsize = AtomicIsize::new(0);
    static CHROME_RWHH: AtomicIsize = AtomicIsize::new(0);

    pub const WM_MWP_SETBOUNDS_PUB: u32 = 0x8000 + 43;
    const WM_MWP_MOUSE: u32 = 0x8000 + 42;

    pub fn set_webview_hwnd(h: isize) { WEBVIEW_HWND.store(h, Ordering::SeqCst); }
    pub fn set_syslistview_hwnd(h: isize) { SYSLISTVIEW_HWND.store(h, Ordering::SeqCst); }
    pub fn set_target_parent_hwnd(h: isize) { TARGET_PARENT_HWND.store(h, Ordering::SeqCst); }
    pub fn set_progman_hwnd(h: isize) { PROGMAN_HWND.store(h, Ordering::SeqCst); }
    pub fn set_explorer_pid(pid: u32) { EXPLORER_PID.store(pid, Ordering::SeqCst); }
    pub fn get_syslistview_hwnd() -> isize { SYSLISTVIEW_HWND.load(Ordering::SeqCst) }
    pub fn set_comp_controller_ptr(p: isize) { COMP_CONTROLLER_PTR.store(p, Ordering::SeqCst); }
    pub fn get_comp_controller_ptr() -> isize { COMP_CONTROLLER_PTR.load(Ordering::SeqCst) }
    pub fn get_dispatch_hwnd() -> isize { DISPATCH_HWND.load(Ordering::SeqCst) }

    #[inline]
    unsafe fn post_mouse(kind: i32, vk: i32, data: u32, x: i32, y: i32) {
        let dh = DISPATCH_HWND.load(Ordering::Relaxed);
        if dh == 0 { return; }
        let wp = WPARAM((kind as u16 as usize) | ((vk as u16 as usize) << 16) | ((data as usize) << 32));
        let lp = LPARAM(((x as i16 as u16 as u32) | ((y as i16 as u16 as u32) << 16)) as isize);
        let _ = PostMessageW(HWND(dh as *mut _), WM_MWP_MOUSE, wp, lp);
    }

    unsafe extern "system" fn dispatch_wnd_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
        if msg == WM_MWP_SETBOUNDS_PUB {
            let ptr = get_comp_controller_ptr();
            if ptr != 0 { let _ = wry::set_controller_bounds_raw(ptr, wp.0 as i32, lp.0 as i32); }
            return LRESULT(0);
        }
        if msg == WM_MWP_MOUSE {
            let ptr = get_comp_controller_ptr();
            if ptr != 0 {
                let kind = (wp.0 & 0xFFFF) as i32;
                let vk = ((wp.0 >> 16) & 0xFFFF) as i32;
                let data = ((wp.0 >> 32) & 0xFFFFFFFF) as u32;
                let x = (lp.0 & 0xFFFF) as i16 as i32;
                let y = ((lp.0 >> 16) & 0xFFFF) as i16 as i32;

                let is_click = kind == MOUSE_LDOWN || kind == MOUSE_LUP
                    || kind == MOUSE_RDOWN || kind == MOUSE_RUP
                    || kind == MOUSE_MDOWN || kind == MOUSE_MUP;

                // Log first 5 events + every 200th + ALL click events
                static FWD_N: AtomicIsize = AtomicIsize::new(0);
                let n = FWD_N.fetch_add(1, Ordering::Relaxed);
                if n < 5 || n % 200 == 0 || is_click {
                    info!("[dispatch] #{} kind=0x{:X} vk={} x={} y={} ptr=0x{:X}", n, kind, vk, x, y, ptr);
                }

                // For click-down events: send MOUSEMOVE to sync cursor
                if kind == MOUSE_LDOWN || kind == MOUSE_RDOWN || kind == MOUSE_MDOWN {
                    // We intentionally DO NOT call SetFocus(wv_h) here.
                    // Stealing focus from the desktop breaks native icon interactions (rename, drag, etc).
                    // Force cursor position update before click
                    let _ = wry::send_mouse_input_raw(ptr, MOUSE_MOVE, vk, 0, x, y);
                }

                if let Err(e) = wry::send_mouse_input_raw(ptr, kind, vk, data, x, y) {
                    static ERR_N: AtomicIsize = AtomicIsize::new(0);
                    let en = ERR_N.fetch_add(1, Ordering::Relaxed);
                    if en < 5 { error!("[dispatch] SendMouseInput FAILED #{}: {}", en, e); }
                }
            }
            return LRESULT(0);
        }
        DefWindowProcW(hwnd, msg, wp, lp)
    }

    pub fn init_dispatch_window() {
        unsafe {
            let cls = windows::core::w!("MWP_MouseDispatch");
            let wc = WNDCLASSW { lpfnWndProc: Some(dispatch_wnd_proc), lpszClassName: cls, ..Default::default() };
            let _ = RegisterClassW(&wc);
            if let Ok(h) = CreateWindowExW(WINDOW_EX_STYLE(0), cls, windows::core::w!(""), WINDOW_STYLE(0), 0, 0, 0, 0, HWND_MESSAGE, None, None, None) {
                DISPATCH_HWND.store(h.0 as isize, Ordering::SeqCst);
                info!("[init_dispatch_window] Message-only window created: 0x{:X}", h.0 as isize);
            } else {
                error!("[init_dispatch_window] Failed to create dispatch window!");
            }
        }
    }

    #[inline]
    unsafe fn get_parent_process_id(pid: u32) -> Option<u32> {
        use windows::Win32::System::Diagnostics::ToolHelp::{
            CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
        };
        use windows::Win32::Foundation::CloseHandle;

        let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if let Ok(snap) = snap {
            let mut entry = PROCESSENTRY32W {
                dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
                ..Default::default()
            };
            if Process32FirstW(snap, &mut entry).is_ok() {
                loop {
                    if entry.th32ProcessID == pid {
                        let _ = CloseHandle(snap);
                        return Some(entry.th32ParentProcessID);
                    }
                    if Process32NextW(snap, &mut entry).is_err() { break; }
                }
            }
            let _ = CloseHandle(snap);
        }
        None
    }

    #[inline]
    unsafe fn is_over_desktop(hwnd_under: HWND) -> bool {
        let tp = HWND(TARGET_PARENT_HWND.load(Ordering::Relaxed) as *mut _);
        let rwhh = HWND(CHROME_RWHH.load(Ordering::Relaxed) as *mut _);
        let wv = HWND(WEBVIEW_HWND.load(Ordering::Relaxed) as *mut _);
        let pm = HWND(PROGMAN_HWND.load(Ordering::Relaxed) as *mut _);
        let dc = HWND(DESKTOP_CORE_HWND.load(Ordering::Relaxed) as *mut _);

        // Fast path: known HWNDs (includes cached desktop CoreWindow)
        if !rwhh.is_invalid() && hwnd_under == rwhh { return true; }
        if !dc.is_invalid() && hwnd_under == dc { return true; }
        if hwnd_under == tp || hwnd_under == wv || hwnd_under == pm { return true; }

        // Progman contains SHELLDLL_DefView (on top in Z-order) + WorkerW (our container).
        if !pm.is_invalid() && IsChild(pm, hwnd_under).as_bool() { return true; }

        // Slow path: class name check for unknown windows
        let mut cls = [0u16; 64];
        let len = GetClassNameW(hwnd_under, &mut cls) as usize;
        let cls_name = String::from_utf16_lossy(&cls[..len]);

        // Win11 24H2: desktop background is a Windows.UI.Core.CoreWindow owned by explorer.exe.
        // It sits ON TOP of Progman/WorkerW in Z-order and intercepts WindowFromPoint.
        if cls_name == "Windows.UI.Core.CoreWindow" {
            let exp_pid = EXPLORER_PID.load(Ordering::Relaxed);
            if exp_pid != 0 {
                let mut pid: u32 = 0;
                GetWindowThreadProcessId(hwnd_under, Some(&mut pid));
                if pid == exp_pid {
                    info!("[is_over_desktop] Desktop CoreWindow detected: 0x{:X} (explorer.exe pid={})",
                        hwnd_under.0 as isize, pid);
                    DESKTOP_CORE_HWND.store(hwnd_under.0 as isize, Ordering::Relaxed);
                    return true;
                }
            }
        }

        // Auto-discover Chrome_RWHH — check if the PARENT's PID matches our app.
        // RWHH itself is in the renderer process (different PID), but its parent
        // Chrome_WidgetWin_1 is in the browser process. For OUR WebView2, that
        // browser process is a child of our app.
        if rwhh.is_invalid() && !wv.is_invalid() {
            if cls_name == "Chrome_RenderWidgetHostHWND" {
                let direct_parent = GetParent(hwnd_under).unwrap_or_default();
                if !direct_parent.is_invalid() {
                    let mut browser_pid: u32 = 0;
                    GetWindowThreadProcessId(direct_parent, Some(&mut browser_pid));
                    let our_pid = std::process::id();

                    let mut is_ours = browser_pid == our_pid;
                    if !is_ours {
                        if let Some(browser_parent_pid) = get_parent_process_id(browser_pid) {
                            if browser_parent_pid == our_pid {
                                is_ours = true;
                            }
                        }
                    }

                    if is_ours {
                        CHROME_RWHH.store(hwnd_under.0 as isize, Ordering::Relaxed);
                        info!("[is_over_desktop] OUR Chrome_RWHH at 0x{:X} (browser pid={} matches app via process tree)",
                            hwnd_under.0 as isize, browser_pid);
                        return true;
                    }
                }
            }
        }
        false
    }

    #[inline]
    unsafe fn forward(msg: u32, info_hook: &MSLLHOOKSTRUCT, cx: i32, cy: i32) {
        match msg {
            WM_MOUSEMOVE => post_mouse(MOUSE_MOVE, DRAG_VK.load(Ordering::Relaxed) as i32, 0, cx, cy),
            WM_LBUTTONDOWN => { DRAG_VK.store(VK_LBUTTON as isize, Ordering::Relaxed); post_mouse(MOUSE_LDOWN, VK_LBUTTON, 0, cx, cy); }
            WM_LBUTTONUP => { DRAG_VK.store(0, Ordering::Relaxed); post_mouse(MOUSE_LUP, VK_NONE, 0, cx, cy); }
            WM_RBUTTONDOWN => { DRAG_VK.store(VK_RBUTTON as isize, Ordering::Relaxed); post_mouse(MOUSE_RDOWN, VK_RBUTTON, 0, cx, cy); }
            WM_RBUTTONUP => { DRAG_VK.store(0, Ordering::Relaxed); post_mouse(MOUSE_RUP, VK_NONE, 0, cx, cy); }
            WM_MBUTTONDOWN => { DRAG_VK.store(VK_MBUTTON as isize, Ordering::Relaxed); post_mouse(MOUSE_MDOWN, VK_MBUTTON, 0, cx, cy); }
            WM_MBUTTONUP => { DRAG_VK.store(0, Ordering::Relaxed); post_mouse(MOUSE_MUP, VK_NONE, 0, cx, cy); }
            WM_MOUSEWHEEL | WM_MOUSEHWHEEL => {
                let kind = if msg == WM_MOUSEWHEEL { MOUSE_WHEEL } else { MOUSE_HWHEEL };
                post_mouse(kind, VK_NONE, (info_hook.mouseData >> 16) as i16 as i32 as u32, cx, cy);
            }
            _ => {}
        }
    }

    pub fn start_hook_thread() {
        std::thread::spawn(|| {
            unsafe {
                use windows::Win32::System::Com::{CoInitializeEx, COINIT_APARTMENTTHREADED};
                let hr = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
                if hr.is_err() {
                    error!("[start_hook_thread] COM Initialization Failed. HRESULT: {:?}", hr);
                }
            }

            unsafe extern "system" fn hook_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
                let hook_h = HHOOK(crate::window_layer::HOOK_HANDLE_GLOBAL.load(Ordering::Relaxed) as *mut _);
                if code < 0 { return CallNextHookEx(hook_h, code, wparam, lparam); }

                let wv_raw = WEBVIEW_HWND.load(Ordering::Relaxed);
                if wv_raw == 0 { return CallNextHookEx(hook_h, code, wparam, lparam); }

                let info_hook = *(lparam.0 as *const MSLLHOOKSTRUCT);
                let msg = wparam.0 as u32;

                let hwnd_under = WindowFromPoint(info_hook.pt);
                let wv = HWND(wv_raw as *mut _);

                // Smart log: boundary crossing + periodic (1/sec) always-log
                static LAST_HWND_UNDER: AtomicIsize = AtomicIsize::new(0);
                static LAST_TICK: AtomicIsize = AtomicIsize::new(0);
                let prev = LAST_HWND_UNDER.swap(hwnd_under.0 as isize, Ordering::Relaxed);
                let boundary = prev != hwnd_under.0 as isize;
                let tick = info_hook.time as isize;
                let periodic = tick.wrapping_sub(LAST_TICK.load(Ordering::Relaxed)) > 1000;
                if boundary || periodic {
                    if periodic { LAST_TICK.store(tick, Ordering::Relaxed); }
                    let mut cls = [0u16; 64];
                    let len = GetClassNameW(hwnd_under, &mut cls);
                    let cls_name = String::from_utf16_lossy(&cls[..len as usize]);
                    let over = is_over_desktop(hwnd_under);
                    let tag = if boundary { "cursor→" } else { "tick   " };
                    info!("[hook] {} 0x{:X} '{}' is_over_desktop={}", tag, hwnd_under.0 as isize, cls_name, over);
                }

                if !is_over_desktop(hwnd_under) { return CallNextHookEx(hook_h, code, wparam, lparam); }

                // Forward to WebView (composition mode receives input via SendMouseInput)
                use windows::Win32::Graphics::Gdi::ScreenToClient;
                let mut cp = info_hook.pt;
                let _ = ScreenToClient(wv, &mut cp);
                forward(msg, &info_hook, cp.x, cp.y);

                // Forward to SysListView32 for icon interactions + selection rectangle
                let slv = HWND(SYSLISTVIEW_HWND.load(Ordering::Relaxed) as *mut _);
                let icons_visible = if !slv.is_invalid() { IsWindowVisible(slv).as_bool() } else { false };

                if icons_visible && !slv.is_invalid() {
                    let lp;
                    if msg == 0x020A || msg == 0x020E { // WM_MOUSEWHEEL, WM_MOUSEHWHEEL
                        // Wheel events use screen coordinates in lParam
                        lp = ((info_hook.pt.x as i16 as u16 as u32) | ((info_hook.pt.y as i16 as u16 as u32) << 16)) as isize;
                    } else {
                        let mut slv_cp = info_hook.pt;
                        let _ = ScreenToClient(slv, &mut slv_cp);
                        lp = ((slv_cp.x as i16 as u16 as u32) | ((slv_cp.y as i16 as u16 as u32) << 16)) as isize;
                    }

                    let mut out_msg = msg;
                    if msg == 0x0201 { // WM_LBUTTONDOWN
                        // Manual double-click detection: low-level hooks only see
                        // WM_LBUTTONDOWN, never WM_LBUTTONDBLCLK. We must synthesize it.
                        static LAST_DOWN_TIME: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
                        static LAST_DOWN_X: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(0);
                        static LAST_DOWN_Y: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(0);

                        let now = info_hook.time;
                        let last_time = LAST_DOWN_TIME.load(Ordering::Relaxed);
                        let dt = now.saturating_sub(last_time);
                        let dx = (info_hook.pt.x - LAST_DOWN_X.load(Ordering::Relaxed)).abs();
                        let dy = (info_hook.pt.y - LAST_DOWN_Y.load(Ordering::Relaxed)).abs();

                        use windows::Win32::UI::Input::KeyboardAndMouse::GetDoubleClickTime;
                        let max_time = GetDoubleClickTime();
                        let max_dx = GetSystemMetrics(SM_CXDOUBLECLK) / 2;
                        let max_dy = GetSystemMetrics(SM_CYDOUBLECLK) / 2;

                        if dt > 0 && dt <= max_time && dx <= max_dx && dy <= max_dy {
                            out_msg = 0x0203; // WM_LBUTTONDBLCLK
                            LAST_DOWN_TIME.store(0, Ordering::Relaxed);
                        } else {
                            LAST_DOWN_TIME.store(now, Ordering::Relaxed);
                            LAST_DOWN_X.store(info_hook.pt.x, Ordering::Relaxed);
                            LAST_DOWN_Y.store(info_hook.pt.y, Ordering::Relaxed);
                        }
                    }

                    let _ = PostMessageW(slv, out_msg, wparam, LPARAM(lp));
                }

                // Always let OS propagate — needed for DoDragDrop native handling
                CallNextHookEx(hook_h, code, wparam, lparam)
            }

            unsafe {
                if let Ok(h) = SetWindowsHookExW(WH_MOUSE_LL, Some(hook_proc), None, 0) {
                    crate::window_layer::HOOK_HANDLE_GLOBAL.store(h.0 as isize, Ordering::SeqCst);
                    info!("[start_hook_thread] WH_MOUSE_LL hook installed: 0x{:X}", h.0 as isize);
                } else {
                    error!("[start_hook_thread] FAILED to install WH_MOUSE_LL hook!");
                }
                let mut msg = MSG::default();
                while GetMessageW(&mut msg, HWND::default(), 0, 0).into() {
                    let _ = TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
            }
        });
    }
}
