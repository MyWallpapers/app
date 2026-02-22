//! Window Layer — Desktop WebView injection + mouse forwarding (Windows only).
//!
//! Injects WebView into Progman/WorkerW hierarchy. Low-level mouse hook
//! intercepts events over the desktop and forwards them to WebView2 via
//! SendMouseInput (composition mode).

#[cfg(target_os = "windows")]
use log::{debug, error, info, warn};
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(target_os = "windows")]
use std::sync::atomic::AtomicIsize;

static ICONS_RESTORED: AtomicBool = AtomicBool::new(false);
#[cfg(target_os = "windows")]
pub static HOOK_HANDLE_GLOBAL: AtomicIsize = AtomicIsize::new(0);

// ============================================================================
// Public API
// ============================================================================

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

pub fn restore_desktop_icons() {
    #[cfg(target_os = "windows")]
    {
        if ICONS_RESTORED.swap(true, Ordering::SeqCst) { return; }
        use windows::Win32::Foundation::HWND;
        use windows::Win32::UI::WindowsAndMessaging::{ShowWindow, SW_SHOW, UnhookWindowsHookEx, HHOOK};

        let slv = mouse_hook::get_syslistview_hwnd();
        if slv != 0 {
            unsafe { let _ = ShowWindow(HWND(slv as *mut _), SW_SHOW); }
            info!("[window_layer] Desktop icons successfully restored on exit.");
        }

        let hook_ptr = HOOK_HANDLE_GLOBAL.load(Ordering::SeqCst);
        if hook_ptr != 0 {
            unsafe { let _ = UnhookWindowsHookEx(HHOOK(hook_ptr as *mut _)); }
            info!("[window_layer] WH_MOUSE_LL hook successfully uninstalled.");
        }
    }
    #[cfg(not(target_os = "windows"))]
    let _ = ICONS_RESTORED.swap(true, Ordering::SeqCst);
}

// ============================================================================
// Windows: Desktop Detection (Universal WorkerW Standard Method)
// ============================================================================

#[cfg(target_os = "windows")]
struct DesktopDetection {
    target_parent: windows::Win32::Foundation::HWND,
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

        info!("[detect_desktop] Found Progman HWND: 0x{:X}", progman.0 as isize);

        // Force Windows to split the desktop and create the background WorkerW
        let mut msg_result: usize = 0;
        let _ = SendMessageTimeoutW(progman, 0x052C, WPARAM(0x0D), LPARAM(1), SMTO_NORMAL, 1000, Some(&mut msg_result));

        // Universal method: EnumWindows to find the WorkerW sibling of SHELLDLL_DefView's parent
        let mut target_parent = HWND::default();

        unsafe extern "system" fn enum_windows_cb(hwnd: HWND, lp: LPARAM) -> BOOL {
            let sv = FindWindowExW(hwnd, HWND::default(), windows::core::w!("SHELLDLL_DefView"), None).unwrap_or_default();
            if !sv.is_invalid() {
                let target = &mut *(lp.0 as *mut HWND);
                *target = FindWindowExW(HWND::default(), hwnd, windows::core::w!("WorkerW"), None).unwrap_or_default();
            }
            BOOL(1)
        }

        for attempt in 1..=40 {
            let _ = EnumWindows(Some(enum_windows_cb), LPARAM(&mut target_parent as *mut _ as isize));
            if !target_parent.is_invalid() {
                info!("[detect_desktop] Found WorkerW target on attempt {}: 0x{:X}", attempt, target_parent.0 as isize);
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }

        if target_parent.is_invalid() {
            return Err("Desktop detection failed. Could not locate WorkerW sibling.".to_string());
        }

        // Find SysListView32 (desktop icons) for hide/show support
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

        // Locate SHELLDLL_DefView to find icons inside it
        let mut active_sv = FindWindowExW(progman, HWND::default(), windows::core::w!("SHELLDLL_DefView"), None).unwrap_or_default();
        if active_sv.is_invalid() {
            unsafe extern "system" fn find_sv_cb(hwnd: HWND, lp: LPARAM) -> BOOL {
                let s = FindWindowExW(hwnd, HWND::default(), windows::core::w!("SHELLDLL_DefView"), None).unwrap_or_default();
                if !s.is_invalid() { *(lp.0 as *mut HWND) = s; return BOOL(0); }
                BOOL(1)
            }
            let _ = EnumWindows(Some(find_sv_cb), LPARAM(&mut active_sv as *mut _ as isize));
        }

        if !active_sv.is_invalid() {
            let _ = EnumChildWindows(active_sv, Some(find_slv), LPARAM(&mut syslistview as *mut _ as isize));
        }

        if syslistview.is_invalid() {
            warn!("[detect_desktop] SysListView32 not found!");
        } else {
            info!("[detect_desktop] Found SysListView32 HWND: 0x{:X}", syslistview.0 as isize);
        }

        // Absolute physical bounds via EnumDisplayMonitors
        struct MonitorRects { left: i32, top: i32, right: i32, bottom: i32 }
        let mut m_rects = MonitorRects { left: 0, top: 0, right: 0, bottom: 0 };
        unsafe extern "system" fn monitor_enum_cb(_hm: HMONITOR, _hdc: HDC, rect: *mut RECT, lp: LPARAM) -> BOOL {
            let data = &mut *(lp.0 as *mut MonitorRects);
            let r = *rect;
            if r.left < data.left { data.left = r.left; }
            if r.top < data.top { data.top = r.top; }
            if r.right > data.right { data.right = r.right; }
            if r.bottom > data.bottom { data.bottom = r.bottom; }
            BOOL(1)
        }
        let _ = EnumDisplayMonitors(HDC::default(), None, Some(monitor_enum_cb), LPARAM(&mut m_rects as *mut _ as isize));

        info!("[detect_desktop] Physical Screen Bounds: ({},{}) -> ({},{}) = {}x{}",
            m_rects.left, m_rects.top, m_rects.right, m_rects.bottom,
            m_rects.right - m_rects.left, m_rects.bottom - m_rects.top);

        Ok(DesktopDetection {
            target_parent,
            syslistview,
            v_x: m_rects.left,
            v_y: m_rects.top,
            v_width: m_rects.right - m_rects.left,
            v_height: m_rects.bottom - m_rects.top,
        })
    }
}

// ============================================================================
// Windows: Injection
// ============================================================================

#[cfg(target_os = "windows")]
fn apply_injection(our_hwnd: windows::Win32::Foundation::HWND, detection: &DesktopDetection) {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::*;

    unsafe {
        let current_parent = GetParent(our_hwnd).unwrap_or_default();
        if current_parent == detection.target_parent { return; }

        let mut style = GetWindowLongW(our_hwnd, GWL_STYLE) as u32;
        style &= !(WS_THICKFRAME.0 | WS_CAPTION.0 | WS_SYSMENU.0 | WS_MAXIMIZEBOX.0 | WS_MINIMIZEBOX.0 | WS_POPUP.0);
        style |= WS_CHILD.0 | WS_VISIBLE.0;
        let _ = SetWindowLongW(our_hwnd, GWL_STYLE, style as i32);

        // Strip WS_EX_LAYERED to force opaque WebView2 rendering (visibility fix)
        let mut ex_style = GetWindowLongW(our_hwnd, GWL_EXSTYLE) as u32;
        ex_style &= !WS_EX_LAYERED.0;
        ex_style |= WS_EX_NOACTIVATE.0;
        let _ = SetWindowLongW(our_hwnd, GWL_EXSTYLE, ex_style as i32);

        // Ensure WorkerW host is visible before injecting
        let _ = ShowWindow(detection.target_parent, SW_SHOW);

        let _ = SetParent(our_hwnd, detection.target_parent);

        let _ = SetWindowPos(
            our_hwnd, HWND_BOTTOM,
            detection.v_x, detection.v_y, detection.v_width, detection.v_height,
            SWP_NOACTIVATE | SWP_FRAMECHANGED | SWP_SHOWWINDOW,
        );

        let _ = ShowWindow(our_hwnd, SW_SHOW);

        let final_parent = GetParent(our_hwnd).unwrap_or_default();
        let is_visible = IsWindowVisible(our_hwnd).as_bool();

        info!(
            "[apply_injection] Injection Complete. Parent=0x{:X}, Visible={}, Rect=({}, {}, {}x{})",
            final_parent.0 as isize, is_visible, detection.v_x, detection.v_y, detection.v_width, detection.v_height
        );
    }
}

// ============================================================================
// Windows: Init
// ============================================================================

#[cfg(target_os = "windows")]
fn ensure_in_worker_w(window: &tauri::WebviewWindow) -> Result<(), String> {
    use windows::Win32::Foundation::HWND;
    let _ = window.set_ignore_cursor_events(true);

    let our_hwnd_raw = window.hwnd().map_err(|e| format!("{}", e))?;
    let our_hwnd = HWND(our_hwnd_raw.0 as *mut _);

    info!("[ensure_in_worker_w] Tauri Webview HWND: 0x{:X}", our_hwnd.0 as isize);

    let detection = detect_desktop()?;

    mouse_hook::set_webview_hwnd(our_hwnd.0 as isize);
    mouse_hook::set_target_parent_hwnd(detection.target_parent.0 as isize);
    if !detection.syslistview.is_invalid() {
        mouse_hook::set_syslistview_hwnd(detection.syslistview.0 as isize);
    }

    apply_injection(our_hwnd, &detection);

    mouse_hook::init_dispatch_window();

    let (w, h) = (detection.v_width, detection.v_height);

    std::thread::spawn(move || {
        use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
        use windows::Win32::UI::WindowsAndMessaging::PostMessageW;

        for attempt in 1..=200 {
            let ptr = wry::get_last_composition_controller_ptr();
            if ptr != 0 {
                info!("[wry_poll] Found Composition Controller at 0x{:X} on attempt {}", ptr, attempt);
                mouse_hook::set_comp_controller_ptr(ptr);
                let dh = mouse_hook::get_dispatch_hwnd();
                if dh != 0 {
                    unsafe {
                        let _ = PostMessageW(
                            HWND(dh as *mut _),
                            mouse_hook::WM_MWP_SETBOUNDS_PUB,
                            WPARAM(w as usize),
                            LPARAM(h as isize),
                        );
                    }
                }
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
    });

    mouse_hook::start_hook_thread();
    Ok(())
}

// ============================================================================
// Windows: Mouse Hook
// ============================================================================

#[cfg(target_os = "windows")]
pub mod mouse_hook {
    use log::{error, info};
    use std::sync::atomic::{AtomicBool, AtomicIsize, AtomicU8, Ordering};
    use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
    use windows::Win32::UI::WindowsAndMessaging::*;

    const MOUSE_MOVE: i32 = 0x0200;
    const MOUSE_LDOWN: i32 = 0x0201;
    const MOUSE_LUP: i32 = 0x0202;
    const MOUSE_RDOWN: i32 = 0x0204;
    const MOUSE_RUP: i32 = 0x0205;
    const MOUSE_MDOWN: i32 = 0x0207;
    const MOUSE_MUP: i32 = 0x0208;
    const MOUSE_WHEEL: i32 = 0x020A;
    const MOUSE_HWHEEL: i32 = 0x020E;

    const VK_NONE: i32 = 0x0;
    const VK_LBUTTON: i32 = 0x1;
    const VK_RBUTTON: i32 = 0x2;
    const VK_MBUTTON: i32 = 0x10;

    static WEBVIEW_HWND: AtomicIsize = AtomicIsize::new(0);
    static SYSLISTVIEW_HWND: AtomicIsize = AtomicIsize::new(0);
    static TARGET_PARENT_HWND: AtomicIsize = AtomicIsize::new(0);
    static COMP_CONTROLLER_PTR: AtomicIsize = AtomicIsize::new(0);
    static DRAG_VK: AtomicIsize = AtomicIsize::new(0);
    static DISPATCH_HWND: AtomicIsize = AtomicIsize::new(0);
    static CHROME_RWHH: AtomicIsize = AtomicIsize::new(0);

    const STATE_IDLE: u8 = 0;
    const STATE_DRAGGING: u8 = 1;
    const STATE_NATIVE: u8 = 2;
    static HOOK_STATE: AtomicU8 = AtomicU8::new(STATE_IDLE);

    static DIAG_POST_FAIL: AtomicBool = AtomicBool::new(true);

    const WM_MWP_MOUSE: u32 = 0x8000 + 42;
    pub const WM_MWP_SETBOUNDS_PUB: u32 = 0x8000 + 43;

    pub fn set_webview_hwnd(h: isize) { WEBVIEW_HWND.store(h, Ordering::SeqCst); }
    pub fn set_syslistview_hwnd(h: isize) { SYSLISTVIEW_HWND.store(h, Ordering::SeqCst); }
    pub fn set_target_parent_hwnd(h: isize) { TARGET_PARENT_HWND.store(h, Ordering::SeqCst); }
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
            if ptr != 0 {
                let _ = wry::set_controller_bounds_raw(ptr, wp.0 as i32, lp.0 as i32);
            }
            return LRESULT(0);
        }
        if msg == WM_MWP_MOUSE {
            let ptr = get_comp_controller_ptr();
            if ptr != 0 {
                let kind = (wp.0 & 0xFFFF) as i32;
                let _ = wry::send_mouse_input_raw(
                    ptr, kind,
                    ((wp.0 >> 16) & 0xFFFF) as i32,
                    ((wp.0 >> 32) & 0xFFFFFFFF) as u32,
                    (lp.0 & 0xFFFF) as i16 as i32,
                    ((lp.0 >> 16) & 0xFFFF) as i16 as i32,
                );
                if kind != MOUSE_MOVE {
                    info!("[dispatch_wnd_proc] Click injected to WebView2!");
                }
            }
            return LRESULT(0);
        }
        DefWindowProcW(hwnd, msg, wp, lp)
    }

    pub fn init_dispatch_window() {
        unsafe {
            let cls = windows::core::w!("MWP_MouseDispatch");
            let wc = WNDCLASSW {
                lpfnWndProc: Some(dispatch_wnd_proc),
                lpszClassName: cls,
                ..Default::default()
            };
            let _ = RegisterClassW(&wc);
            if let Ok(h) = CreateWindowExW(
                WINDOW_EX_STYLE(0), cls, windows::core::w!(""), WINDOW_STYLE(0),
                0, 0, 0, 0, HWND_MESSAGE, None, None, None,
            ) {
                DISPATCH_HWND.store(h.0 as isize, Ordering::SeqCst);
                info!("[init_dispatch_window] Proxy window at 0x{:X}", h.0 as isize);
            }
        }
    }

    #[inline]
    unsafe fn is_over_desktop(hwnd_under: HWND) -> bool {
        let tp = HWND(TARGET_PARENT_HWND.load(Ordering::Relaxed) as *mut _);
        let rwhh = HWND(CHROME_RWHH.load(Ordering::Relaxed) as *mut _);
        let wv = HWND(WEBVIEW_HWND.load(Ordering::Relaxed) as *mut _);

        // Fast path 1: Cached Chrome_RenderWidgetHostHWND
        if !rwhh.is_invalid() && hwnd_under == rwhh { return true; }
        // Fast path 2: Known roots
        if hwnd_under == tp || hwnd_under == wv { return true; }
        // Hierarchy check
        if IsChild(tp, hwnd_under).as_bool() || (!wv.is_invalid() && IsChild(wv, hwnd_under).as_bool()) {
            return true;
        }

        // Auto-discovery of Chrome_RenderWidgetHostHWND (string comparison, not hardcoded length)
        if rwhh.is_invalid() {
            let mut cls = [0u16; 64];
            let len = GetClassNameW(hwnd_under, &mut cls) as usize;
            let cls_name = String::from_utf16_lossy(&cls[..len]);

            if cls_name == "Chrome_RenderWidgetHostHWND" {
                CHROME_RWHH.store(hwnd_under.0 as isize, Ordering::Relaxed);
                info!("[is_over_desktop] WebView2 Auto-Discovered! Clicks will now work.");
                return true;
            }
        }
        false
    }

    #[inline]
    unsafe fn is_mouse_over_desktop_icon(x: i32, y: i32) -> bool {
        use windows::core::VARIANT;
        use windows::Win32::Foundation::POINT;
        use windows::Win32::System::Variant::{VT_DISPATCH, VT_I4};
        use windows::Win32::UI::Accessibility::{AccessibleObjectFromPoint, IAccessible};

        let pt = POINT { x, y };
        let mut p_acc: Option<IAccessible> = None;
        let mut var_child = VARIANT::default();

        if AccessibleObjectFromPoint(pt, &mut p_acc, &mut var_child).is_ok() {
            if let Some(acc) = p_acc {
                match acc.accHitTest(x, y) {
                    Ok(hit) => {
                        let vt = hit.as_raw().Anonymous.Anonymous.vt;
                        if vt == VT_I4.0 as u16 {
                            hit.as_raw().Anonymous.Anonymous.Anonymous.lVal > 0
                        } else {
                            vt == VT_DISPATCH.0 as u16
                        }
                    }
                    Err(_) => false,
                }
            } else { false }
        } else { false }
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
                let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
            }

            unsafe extern "system" fn hook_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
                let hook_h = HHOOK(crate::window_layer::HOOK_HANDLE_GLOBAL.load(Ordering::Relaxed) as *mut _);
                if code < 0 { return CallNextHookEx(hook_h, code, wparam, lparam); }

                let wv_raw = WEBVIEW_HWND.load(Ordering::Relaxed);
                if wv_raw == 0 { return CallNextHookEx(hook_h, code, wparam, lparam); }

                let info_hook = *(lparam.0 as *const MSLLHOOKSTRUCT);
                let msg = wparam.0 as u32;
                let is_down = msg == WM_LBUTTONDOWN || msg == WM_RBUTTONDOWN || msg == WM_MBUTTONDOWN;
                let is_up = msg == WM_LBUTTONUP || msg == WM_RBUTTONUP || msg == WM_MBUTTONUP;

                let hwnd_under = WindowFromPoint(info_hook.pt);

                if !is_over_desktop(hwnd_under) {
                    return CallNextHookEx(hook_h, code, wparam, lparam);
                }

                let is_icon = is_mouse_over_desktop_icon(info_hook.pt.x, info_hook.pt.y);
                let state = HOOK_STATE.load(Ordering::Relaxed);

                // STATE_NATIVE: Interacting with a real desktop icon
                if state == STATE_NATIVE {
                    if is_up { HOOK_STATE.store(STATE_IDLE, Ordering::Relaxed); }
                    return CallNextHookEx(hook_h, code, wparam, lparam);
                }

                // STATE_DRAGGING: Holding a click on the WebView
                if state == STATE_DRAGGING {
                    use windows::Win32::Graphics::Gdi::ScreenToClient;
                    let mut cp = info_hook.pt;
                    let _ = ScreenToClient(HWND(wv_raw as *mut _), &mut cp);
                    forward(msg, &info_hook, cp.x, cp.y);
                    if is_up { HOOK_STATE.store(STATE_IDLE, Ordering::Relaxed); }
                    if msg == WM_MOUSEMOVE { return CallNextHookEx(hook_h, code, wparam, lparam); }
                    return LRESULT(1);
                }

                // STATE_IDLE: Evaluate click target
                if is_down {
                    if is_icon {
                        HOOK_STATE.store(STATE_NATIVE, Ordering::Relaxed);
                        return CallNextHookEx(hook_h, code, wparam, lparam);
                    }
                    HOOK_STATE.store(STATE_DRAGGING, Ordering::Relaxed);
                }

                use windows::Win32::Graphics::Gdi::ScreenToClient;
                let mut cp = info_hook.pt;
                let _ = ScreenToClient(HWND(wv_raw as *mut _), &mut cp);
                forward(msg, &info_hook, cp.x, cp.y);

                if msg == WM_MOUSEMOVE { return CallNextHookEx(hook_h, code, wparam, lparam); }
                LRESULT(1)
            }

            unsafe {
                if let Ok(h) = SetWindowsHookExW(WH_MOUSE_LL, Some(hook_proc), None, 0) {
                    crate::window_layer::HOOK_HANDLE_GLOBAL.store(h.0 as isize, Ordering::SeqCst);
                    info!("[start_hook_thread] WH_MOUSE_LL Hook installed: 0x{:X}", h.0 as isize);
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
