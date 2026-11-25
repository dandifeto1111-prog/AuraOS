//! Native Win32 OS Shell Layer for AuraOS v3
//! - Real Windows AppBar (34px Top Bar) + SPI_SETWORKAREA (zero 48px bottom gap)
//! - 100% Reversible Windows Taskbar suppression (never kills explorer.exe)
//! - Bulletproof Fullscreen detector (zero QUNS=2 / Wallpaper Engine false positives)
//! - Open Windows tracker, native File Picker, .lnk resolver, Power actions

use crate::models::{KeyboardLayoutEntry, RunningWindow};
use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use windows::core::{PCWSTR, PWSTR};
use windows::Win32::Foundation::{BOOL, HWND, LPARAM, MAX_PATH, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_CLOAKED};
use windows::Win32::Graphics::Gdi::{
    GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTONEAREST,
};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, IPersistFile, CLSCTX_ALL, CLSCTX_INPROC_SERVER,
    COINIT_APARTMENTTHREADED, STGM,
};
use windows::Win32::System::ProcessStatus::GetModuleFileNameExW;
use windows::Win32::System::Threading::{
    CreateMutexW, OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
    PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_VM_READ,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    ActivateKeyboardLayout, EnableWindow, GetKeyboardLayout, GetKeyboardLayoutList, keybd_event,
    HKL, KEYBD_EVENT_FLAGS, KEYEVENTF_KEYUP, KLF_ACTIVATE,
};
use windows::Win32::UI::Shell::Common::COMDLG_FILTERSPEC;
use windows::Win32::UI::Shell::{
    FileOpenDialog, FileSaveDialog, IFileOpenDialog, IFileSaveDialog, IShellLinkW, SHAppBarMessage,
    ShellExecuteW, ShellLink, ABM_GETSTATE, ABM_NEW, ABM_QUERYPOS, ABM_REMOVE, ABM_SETPOS,
    ABM_SETSTATE, ABS_AUTOHIDE, ABE_TOP, APPBARDATA, FOS_FILEMUSTEXIST, FOS_NOCHANGEDIR,
    FOS_OVERWRITEPROMPT, SIGDN_FILESYSPATH,
};
use windows::Win32::System::Shutdown::LockWorkStation;
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, FindWindowExW, FindWindowW, GetClassNameW, GetCursorPos, GetForegroundWindow,
    GetSystemMetrics, GetWindowLongW, GetWindowRect, GetWindowTextLengthW, GetWindowTextW,
    GetWindowThreadProcessId, IsIconic, IsWindowVisible, IsZoomed, PostMessageW,
    RegisterWindowMessageW, SetForegroundWindow, SetWindowLongW, SetWindowPos, ShowWindow,
    SystemParametersInfoW, GWL_EXSTYLE, HWND_BOTTOM, HWND_TOPMOST, SM_CXSCREEN, SM_CYSCREEN,
    SPIF_SENDCHANGE, SPI_SETWORKAREA, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER,
    SW_HIDE, SW_MAXIMIZE, SW_RESTORE, SW_SHOW, SW_SHOWNORMAL, WM_INPUTLANGCHANGEREQUEST,
    WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
};

static REGISTERED_APPBAR: AtomicIsize = AtomicIsize::new(0);
static TASKBAR_HIDDEN_BY_US: AtomicBool = AtomicBool::new(false);
static SAVED_TASKBAR_STATE: AtomicIsize = AtomicIsize::new(-1);

pub fn to_wide(s: &str) -> Vec<u16> {
    OsStr::new(s).encode_wide().chain(std::iter::once(0)).collect()
}

pub fn from_wide(buf: &[u16]) -> String {
    let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..end])
}

pub fn screen_size() -> (i32, i32) {
    unsafe { (GetSystemMetrics(SM_CXSCREEN), GetSystemMetrics(SM_CYSCREEN)) }
}

pub fn cursor_pos() -> (i32, i32) {
    unsafe {
        let mut pt = POINT::default();
        if GetCursorPos(&mut pt).is_ok() {
            (pt.x, pt.y)
        } else {
            (0, 0)
        }
    }
}

pub fn acquire_single_instance_mutex(name: &str) -> bool {
    unsafe {
        let w = to_wide(name);
        let h = CreateMutexW(None, true, PCWSTR(w.as_ptr()));
        if h.is_err() {
            return false;
        }
        let last_err = windows::Win32::Foundation::GetLastError();
        if last_err == windows::Win32::Foundation::ERROR_ALREADY_EXISTS {
            // ONLY activate AuraTopBar Overview when this mutex belongs to AuraShell itself!
            // Other apps (AuraLock, AuraSettings, etc.) must NEVER trigger WM_AURASHELL_V3_ACTIVATE!
            if name.contains("AuraShell") {
                let msg_name = to_wide("WM_AURASHELL_V3_ACTIVATE");
                let msg = RegisterWindowMessageW(PCWSTR(msg_name.as_ptr()));
                let top_title = to_wide("AuraTopBar");
                if let Ok(existing) = FindWindowW(None, PCWSTR(top_title.as_ptr())) {
                    if !existing.is_invalid() {
                        let _ = PostMessageW(existing, msg, WPARAM(0), LPARAM(0));
                        return false;
                    }
                }
            } else if name.contains("AuraLock") {
                let lock_title = to_wide("AuraLock");
                if let Ok(existing) = FindWindowW(None, PCWSTR(lock_title.as_ptr())) {
                    if !existing.is_invalid() && IsWindowVisible(existing).as_bool() {
                        bring_to_front(existing);
                        return false;
                    }
                }
            }
            return false;
        }
        true
    }
}

pub fn make_tool_window(hwnd: HWND, no_activate: bool) {
    unsafe {
        let mut ex = GetWindowLongW(hwnd, GWL_EXSTYLE);
        ex |= WS_EX_TOOLWINDOW.0 as i32;
        if no_activate {
            ex |= WS_EX_NOACTIVATE.0 as i32;
        }
        SetWindowLongW(hwnd, GWL_EXSTYLE, ex);
    }
}

pub fn ensure_topmost(hwnd: HWND) {
    unsafe {
        let _ = SetWindowPos(
            hwnd,
            HWND_TOPMOST,
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
        );
    }
}

pub fn send_to_desktop_bottom(hwnd: HWND) {
    unsafe {
        let _ = SetWindowPos(
            hwnd,
            HWND_BOTTOM,
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
        );
    }
}

pub fn bring_to_front(hwnd: HWND) {
    use windows::Win32::System::Threading::{AttachThreadInput, GetCurrentThreadId};
    use windows::Win32::UI::Input::KeyboardAndMouse::SetFocus;
    use windows::Win32::UI::WindowsAndMessaging::{
        AllowSetForegroundWindow, BringWindowToTop, IsHungAppWindow, ASFW_ANY,
    };
    unsafe {
        if IsIconic(hwnd).as_bool() {
            let _ = ShowWindow(hwnd, SW_RESTORE);
        } else {
            let _ = ShowWindow(hwnd, SW_SHOW);
        }
        let _ = AllowSetForegroundWindow(ASFW_ANY);
        let _ = BringWindowToTop(hwnd);
        if SetForegroundWindow(hwnd).as_bool() {
            let _ = SetFocus(hwnd);
            return;
        }
        // Fallback ONLY if foreground window is NOT hung (prevents cross-monitor / hung-app input queue deadlock!)
        let fg = GetForegroundWindow();
        if !fg.is_invalid() && !IsHungAppWindow(fg).as_bool() {
            let fg_tid = GetWindowThreadProcessId(fg, None);
            let cur_tid = GetCurrentThreadId();
            let attached = fg_tid != 0 && fg_tid != cur_tid && AttachThreadInput(cur_tid, fg_tid, true).as_bool();
            let _ = BringWindowToTop(hwnd);
            let _ = SetForegroundWindow(hwnd);
            let _ = SetFocus(hwnd);
            if attached {
                let _ = AttachThreadInput(cur_tid, fg_tid, false);
            }
        }
    }
}

pub fn toggle_or_focus_window(hwnd_raw: isize) {
    let hwnd = HWND(hwnd_raw as *mut _);
    if hwnd.is_invalid() {
        return;
    }
    unsafe {
        if IsIconic(hwnd).as_bool() {
            let _ = ShowWindow(hwnd, SW_RESTORE);
            let _ = SetForegroundWindow(hwnd);
        } else if GetForegroundWindow() == hwnd {
            let _ = ShowWindow(hwnd, windows::Win32::UI::WindowsAndMessaging::SW_MINIMIZE);
        } else {
            let _ = ShowWindow(hwnd, SW_SHOW);
            let _ = SetForegroundWindow(hwnd);
        }
    }
}

// ==================== APPBAR & WORKAREA ====================

pub fn register_top_appbar(hwnd: HWND, screen_w: i32, bar_h: i32) -> RECT {
    unsafe {
        let mut abd = APPBARDATA {
            cbSize: std::mem::size_of::<APPBARDATA>() as u32,
            hWnd: hwnd,
            uEdge: ABE_TOP,
            rc: RECT { left: 0, top: 0, right: screen_w, bottom: bar_h },
            ..Default::default()
        };
        if REGISTERED_APPBAR.load(Ordering::Relaxed) != hwnd.0 as isize {
            SHAppBarMessage(ABM_NEW, &mut abd);
            REGISTERED_APPBAR.store(hwnd.0 as isize, Ordering::Relaxed);
        }
        SHAppBarMessage(ABM_QUERYPOS, &mut abd);
        abd.rc.bottom = abd.rc.top + bar_h;
        SHAppBarMessage(ABM_SETPOS, &mut abd);
        abd.rc
    }
}

pub fn unregister_top_appbar() {
    let raw = REGISTERED_APPBAR.swap(0, Ordering::Relaxed);
    if raw == 0 {
        return;
    }
    unsafe {
        let mut abd = APPBARDATA {
            cbSize: std::mem::size_of::<APPBARDATA>() as u32,
            hWnd: HWND(raw as *mut _),
            ..Default::default()
        };
        SHAppBarMessage(ABM_REMOVE, &mut abd);
    }
}

pub fn set_work_area(left: i32, top: i32, right: i32, bottom: i32) {
    unsafe {
        let mut rc = RECT { left, top, right, bottom };
        let _ = SystemParametersInfoW(
            SPI_SETWORKAREA,
            0,
            Some(&mut rc as *mut RECT as *mut _),
            SPIF_SENDCHANGE,
        );
    }
}

pub fn apply_full_work_area(top_bar_px: i32, taskbar_hidden: bool) {
    let (sw, sh) = screen_size();
    let bottom = if taskbar_hidden { sh } else { sh - 48 };
    set_work_area(0, top_bar_px, sw, bottom);
}

pub fn refresh_maximized_windows() {
    let open = get_open_user_windows();
    unsafe {
        for w in open {
            let hwnd = HWND(w.hwnd as *mut _);
            if IsZoomed(hwnd).as_bool() {
                let _ = ShowWindow(hwnd, SW_RESTORE);
                let _ = ShowWindow(hwnd, SW_MAXIMIZE);
            }
        }
    }
}

// ==================== REVERSIBLE WINDOWS TASKBAR SUPPRESSION ====================

fn set_windows_taskbar_autohide(enable: bool) {
    unsafe {
        let cls = to_wide("Shell_TrayWnd");
        let Ok(tray) = FindWindowW(PCWSTR(cls.as_ptr()), None) else { return };
        if tray.is_invalid() {
            return;
        }
        let mut abd = APPBARDATA {
            cbSize: std::mem::size_of::<APPBARDATA>() as u32,
            hWnd: tray,
            ..Default::default()
        };
        if enable && SAVED_TASKBAR_STATE.load(Ordering::Relaxed) < 0 {
            let st = SHAppBarMessage(ABM_GETSTATE, &mut abd) as isize;
            SAVED_TASKBAR_STATE.store(st, Ordering::Relaxed);
        }
        if enable {
            abd.lParam = LPARAM(ABS_AUTOHIDE as isize);
            SHAppBarMessage(ABM_SETSTATE, &mut abd);
        } else {
            let saved = SAVED_TASKBAR_STATE.swap(-1, Ordering::Relaxed);
            let restore = if saved >= 0 { saved & !(ABS_AUTOHIDE as isize) } else { 0 };
            abd.lParam = LPARAM(restore);
            SHAppBarMessage(ABM_SETSTATE, &mut abd);
        }
    }
}

pub fn set_taskbar_visibility(visible: bool) {
    unsafe {
        let (sw, sh) = screen_size();
        if !visible {
            set_windows_taskbar_autohide(true);
        }

        let cls = to_wide("Shell_TrayWnd");
        if let Ok(primary) = FindWindowW(PCWSTR(cls.as_ptr()), None) {
            if !primary.is_invalid() {
                if !visible {
                    let _ = EnableWindow(primary, false);
                    let _ = ShowWindow(primary, SW_HIDE);
                    let _ = SetWindowPos(primary, HWND::default(), 0, sh + 240, sw, 48, SWP_NOACTIVATE | SWP_NOZORDER);
                } else {
                    let _ = EnableWindow(primary, true);
                    let _ = SetWindowPos(primary, HWND::default(), 0, sh - 48, sw, 48, SWP_NOACTIVATE | SWP_NOZORDER);
                    let _ = ShowWindow(primary, SW_SHOW);
                }
            }
        }

        let sec_cls = to_wide("Shell_SecondaryTrayWnd");
        let mut sec = HWND::default();
        while let Ok(next) = FindWindowExW(HWND::default(), sec, PCWSTR(sec_cls.as_ptr()), None) {
            if next.is_invalid() {
                break;
            }
            let _ = EnableWindow(next, visible);
            let _ = ShowWindow(next, if visible { SW_SHOW } else { SW_HIDE });
            sec = next;
        }

        if visible {
            set_windows_taskbar_autohide(false);
        }
        TASKBAR_HIDDEN_BY_US.store(!visible, Ordering::Relaxed);
    }
}

pub fn enforce_taskbar_suppressed() {
    unsafe {
        let cls = to_wide("Shell_TrayWnd");
        let Ok(primary) = FindWindowW(PCWSTR(cls.as_ptr()), None) else { return };
        if primary.is_invalid() {
            return;
        }
        let (sw, sh) = screen_size();
        let mut r = RECT::default();
        let vis = IsWindowVisible(primary).as_bool();
        let on_screen = GetWindowRect(primary, &mut r).is_ok() && r.top < sh + 50;
        if vis || on_screen {
            let _ = EnableWindow(primary, false);
            let _ = ShowWindow(primary, SW_HIDE);
            let _ = SetWindowPos(primary, HWND::default(), 0, sh + 240, sw, 48, SWP_NOACTIVATE | SWP_NOZORDER);
        }
    }
}

pub fn restore_windows_safely() {
    apply_display_gamma(100, false);
    set_keep_awake(false);
    crate::window_styler::restore_all_styled_windows();
    unregister_top_appbar();
    if TASKBAR_HIDDEN_BY_US.load(Ordering::Relaxed) {
        set_taskbar_visibility(true);
        let (sw, sh) = screen_size();
        set_work_area(0, 0, sw, sh - 48);
    }
}

// ==================== v5.1 LAPTOP BATTERY, BRIGHTNESS/NIGHT LIGHT, KEEP AWAKE & RAM TRIM ====================

/// Returns `Some((battery_pct, is_charging))` on laptops/UPS, or `None` on desktop PCs without battery!
pub fn get_battery_status() -> Option<(u8, bool)> {
    use windows::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};
    unsafe {
        let mut sps = SYSTEM_POWER_STATUS::default();
        if GetSystemPowerStatus(&mut sps).is_ok() {
            // 128 = No system battery, 255 = Unknown
            if (sps.BatteryFlag & 128) != 0 || sps.BatteryLifePercent > 100 {
                return None;
            }
            let charging = sps.ACLineStatus == 1 || (sps.BatteryFlag & 8) != 0;
            return Some((sps.BatteryLifePercent, charging));
        }
    }
    None
}

/// Real smooth hardware display brightness (35..=100%) + warm 3800K Night Light via GDI Gamma Ramp!
pub fn apply_display_gamma(brightness_pct: i32, night_light: bool) {
    use windows::Win32::Graphics::Gdi::{GetDC, ReleaseDC};
    use windows::Win32::UI::ColorSystem::SetDeviceGammaRamp;
    let b_scale = (brightness_pct.clamp(35, 100) as f32) / 100.0;
    let r_mul = b_scale;
    let g_mul = if night_light { b_scale * 0.84 } else { b_scale };
    let bl_mul = if night_light { b_scale * 0.62 } else { b_scale };

    let mut ramp = [[0u16; 256]; 3];
    for i in 0..256usize {
        let base = (i * 256) as f32;
        ramp[0][i] = (base * r_mul).clamp(0.0, 65535.0) as u16;
        ramp[1][i] = (base * g_mul).clamp(0.0, 65535.0) as u16;
        ramp[2][i] = (base * bl_mul).clamp(0.0, 65535.0) as u16;
    }
    unsafe {
        let hdc = GetDC(HWND::default());
        if !hdc.is_invalid() {
            let _ = SetDeviceGammaRamp(hdc, ramp.as_ptr() as *const _);
            let _ = ReleaseDC(HWND::default(), hdc);
        }
    }
}

/// Prevents monitor sleep & screensaver while active ("Café / Keep Awake" mode)!
pub fn set_keep_awake(enabled: bool) {
    use windows::Win32::System::Power::{
        SetThreadExecutionState, ES_CONTINUOUS, ES_DISPLAY_REQUIRED, ES_SYSTEM_REQUIRED,
    };
    unsafe {
        let flags = if enabled {
            ES_CONTINUOUS | ES_DISPLAY_REQUIRED | ES_SYSTEM_REQUIRED
        } else {
            ES_CONTINUOUS
        };
        let _ = SetThreadExecutionState(flags);
    }
}

pub fn get_uptime_short() -> String {
    use windows::Win32::System::SystemInformation::GetTickCount64;
    let secs = unsafe { GetTickCount64() } / 1000;
    let mins = (secs / 60) % 60;
    let hours = (secs / 3600) % 24;
    let days = secs / 86400;
    if days > 0 {
        format!("Activo {}d {}h", days, hours)
    } else if hours > 0 {
        format!("Activo {}h {}m", hours, mins)
    } else {
        format!("Activo {}m", mins.max(1))
    }
}

/// Gently returns unused working-set pages to Windows after closing heavy overlays!
pub fn trim_own_working_set() {
    use windows::Win32::System::ProcessStatus::EmptyWorkingSet;
    use windows::Win32::System::Threading::GetCurrentProcess;
    unsafe {
        let _ = EmptyWorkingSet(GetCurrentProcess());
    }
}

// ==================== FULLSCREEN & DESKTOP DETECTION (ZERO QUNS=2 BUG!) ====================

pub fn get_process_info(pid: u32) -> (String, String) {
    if pid == 0 {
        return (String::new(), String::new());
    }
    unsafe {
        // Primary: PROCESS_QUERY_LIMITED_INFORMATION + QueryFullProcessImageNameW works on sandboxed
        // Edge/Chrome audio processes, Store UWP apps (Spotify/WhatsApp), and anti-cheat games!
        if let Ok(h) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) {
            let mut buf = [0u16; MAX_PATH as usize * 2];
            let mut len = buf.len() as u32;
            let ok = QueryFullProcessImageNameW(h, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut len).is_ok();
            let _ = windows::Win32::Foundation::CloseHandle(h);
            if ok && len > 0 {
                let path = String::from_utf16_lossy(&buf[..len as usize]);
                let name = Path::new(&path)
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("")
                    .to_string();
                return (name, path);
            }
        }
        // Fallback: GetModuleFileNameExW
        if let Ok(h) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_VM_READ, false, pid) {
            let mut buf = [0u16; MAX_PATH as usize * 2];
            let len = GetModuleFileNameExW(h, None, &mut buf);
            let _ = windows::Win32::Foundation::CloseHandle(h);
            if len > 0 {
                let path = String::from_utf16_lossy(&buf[..len as usize]);
                let name = Path::new(&path)
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("")
                    .to_string();
                return (name, path);
            }
        }
    }
    (String::new(), String::new())
}

pub fn is_desktop_or_shell_window(hwnd: HWND) -> bool {
    if hwnd.is_invalid() {
        return true;
    }
    unsafe {
        if !IsWindowVisible(hwnd).as_bool() || IsIconic(hwnd).as_bool() {
            return true;
        }
        let mut cls_buf = [0u16; 128];
        let clen = GetClassNameW(hwnd, &mut cls_buf);
        let cls = String::from_utf16_lossy(&cls_buf[..clen as usize]);
        if matches!(
            cls.as_str(),
            "Progman"
                | "WorkerW"
                | "SHELLDLL_DefView"
                | "SysListView32"
                | "Shell_TrayWnd"
                | "Shell_SecondaryTrayWnd"
                | "ForegroundStaging"
                | "ApplicationManager_DesktopShellWindow"
                | "MultitaskingViewFrame"
                | "Windows.UI.Core.CoreWindow"
        ) {
            return true;
        }

        let tlen = GetWindowTextLengthW(hwnd);
        if tlen > 0 {
            let mut tbuf = vec![0u16; (tlen + 1) as usize];
            let got = GetWindowTextW(hwnd, &mut tbuf);
            let title = String::from_utf16_lossy(&tbuf[..got as usize]);
            if title.contains("AURA_WEB_WP_HOST")
                || matches!(
                    title.trim(),
                    "Program Manager"
                        | "WorkerW"
                        | "AuraTopBar"
                        | "AuraDock"
                        | "AuraDockNotch"
                        | "AuraOverview"
                        | "AuraControlCenter"
                        | "AuraPowerMenu"
                        | "AuraLaunchpad"
                        | "AuraDesktopClock"
                        | "AuraWallpaperLayer"
                        | "AuraWallpaperVideo"
                        | "AuraWidgetClock"
                        | "AuraWidgetMusic"
                        | "AuraWidgetWeather"
                        | "AuraWidgetMonitor"
                )
            {
                return true;
            }
        }

        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        if pid != 0 {
            if pid == std::process::id() {
                return true;
            }
            let (pn, _) = get_process_info(pid);
            let low = pn.to_lowercase();
            if matches!(
                low.as_str(),
                "aurashell"
                    | "wallpaper64"
                    | "wallpaper32"
                    | "webwallpaper32"
                    | "ui32"
                    | "searchhost"
                    | "startmenuexperiencehost"
                    | "shellexperiencehost"
                    | "shellhost"
                    | "lockapp"
            ) {
                return true;
            }
            if low == "explorer" && cls != "CabinetWClass" && cls != "ExploreWClass" {
                return true;
            }
        }
    }
    false
}

pub fn is_fullscreen_app_active() -> bool {
    unsafe {
        let fg = GetForegroundWindow();
        if is_desktop_or_shell_window(fg) {
            return false;
        }
        let mut cloaked: i32 = 0;
        let _ = DwmGetWindowAttribute(
            fg,
            DWMWA_CLOAKED,
            &mut cloaked as *mut i32 as *mut _,
            std::mem::size_of::<i32>() as u32,
        );
        if cloaked != 0 {
            return false;
        }
        let ex = GetWindowLongW(fg, GWL_EXSTYLE);
        if (ex & WS_EX_TOOLWINDOW.0 as i32) != 0 {
            return false;
        }
        let mut r = RECT::default();
        if GetWindowRect(fg, &mut r).is_err() {
            return false;
        }
        let mon = MonitorFromWindow(fg, MONITOR_DEFAULTTONEAREST);
        let mut mi = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if !GetMonitorInfoW(mon, &mut mi).as_bool() {
            return false;
        }
        // Multi-monitor fix: ONLY hide Primary Monitor AuraTopBar/AuraDock when the fullscreen app
        // is actually on the PRIMARY monitor (MONITORINFOF_PRIMARY = 1)!
        if (mi.dwFlags & 1) == 0 {
            return false;
        }
        let m = mi.rcMonitor;
        r.left <= m.left + 1 && r.top <= m.top + 1 && r.right >= m.right - 1 && r.bottom >= m.bottom - 1
    }
}

pub fn is_foreground_overlapping(dock_rect: RECT) -> bool {
    unsafe {
        let fg = GetForegroundWindow();
        if is_desktop_or_shell_window(fg) {
            return false;
        }
        if IsZoomed(fg).as_bool() {
            return true;
        }
        let mut r = RECT::default();
        if GetWindowRect(fg, &mut r).is_err() {
            return false;
        }
        !(r.right <= dock_rect.left - 14
            || r.left >= dock_rect.right + 14
            || r.bottom <= dock_rect.top - 14
            || r.top >= dock_rect.bottom + 14)
    }
}

// ==================== OPEN USER WINDOWS TRACKER ====================

struct EnumState {
    fg: HWND,
    my_pid: u32,
    out: Vec<RunningWindow>,
}

unsafe extern "system" fn enum_windows_cb(hwnd: HWND, lparam: LPARAM) -> BOOL {
    let state = &mut *(lparam.0 as *mut EnumState);
    if !IsWindowVisible(hwnd).as_bool() {
        return BOOL(1);
    }
    let tlen = GetWindowTextLengthW(hwnd);
    if tlen <= 0 {
        return BOOL(1);
    }
    let ex = GetWindowLongW(hwnd, GWL_EXSTYLE);
    if (ex & WS_EX_TOOLWINDOW.0 as i32) != 0 {
        return BOOL(1);
    }
    let mut cloaked: i32 = 0;
    let _ = DwmGetWindowAttribute(
        hwnd,
        DWMWA_CLOAKED,
        &mut cloaked as *mut i32 as *mut _,
        std::mem::size_of::<i32>() as u32,
    );
    if cloaked != 0 {
        return BOOL(1);
    }

    let mut pid = 0u32;
    GetWindowThreadProcessId(hwnd, Some(&mut pid));
    if pid == 0 || pid == state.my_pid {
        return BOOL(1);
    }

    let mut tbuf = vec![0u16; (tlen + 1) as usize];
    let got = GetWindowTextW(hwnd, &mut tbuf);
    let title = String::from_utf16_lossy(&tbuf[..got as usize]).trim().to_string();
    if title.is_empty()
        || title == "Program Manager"
        || title.contains("AURA_WEB_WP_HOST")
        || (title == "Settings" && cloaked != 0)
    {
        return BOOL(1);
    }

    let (proc_name, exe_path) = get_process_info(pid);
    let low = proc_name.to_lowercase();
    if matches!(
        low.as_str(),
        "aurashell"
            | "wallpaper64"
            | "wallpaper32"
            | "textinputhost"
            | "searchhost"
            | "startmenuexperiencehost"
            | "shellexperiencehost"
            | "applicationframehost"
    ) && title == "Program Manager"
    {
        return BOOL(1);
    }
    if matches!(
        low.as_str(),
        "aurashell" | "wallpaper64" | "wallpaper32" | "textinputhost" | "searchhost" | "startmenuexperiencehost"
    ) {
        return BOOL(1);
    }

    state.out.push(RunningWindow {
        hwnd: hwnd.0 as isize,
        pid,
        title,
        process_name: if proc_name.is_empty() { "App".to_string() } else { proc_name },
        exe_path,
        is_active: hwnd == state.fg,
        is_minimized: IsIconic(hwnd).as_bool(),
    });
    BOOL(1)
}

pub fn get_open_user_windows() -> Vec<RunningWindow> {
    use std::collections::{HashMap, HashSet};
    use std::sync::Mutex;

    static WINDOW_ORDER: Mutex<Option<(u64, HashMap<isize, u64>)>> = Mutex::new(None);

    let mut out = unsafe {
        let mut state = EnumState {
            fg: GetForegroundWindow(),
            my_pid: std::process::id(),
            out: Vec::with_capacity(16),
        };
        let _ = EnumWindows(Some(enum_windows_cb), LPARAM(&mut state as *mut EnumState as isize));
        state.out
    };

    // STABLE INSERTION ORDER: EnumWindows enumerates in Z-order (focused first), which used to
    // jump clicked apps to slot #1 in AuraDock! Assign each HWND a permanent monotonic order ID
    // the first time it appears so icons stay ROCK-SOLID in their exact slot forever!
    let mut guard = WINDOW_ORDER.lock().unwrap_or_else(|e| e.into_inner());
    let (next_seq, map) = guard.get_or_insert_with(|| (1u64, HashMap::new()));
    let live_set: HashSet<isize> = out.iter().map(|w| w.hwnd).collect();
    map.retain(|h, _| live_set.contains(h));

    for w in &out {
        map.entry(w.hwnd).or_insert_with(|| {
            let id = *next_seq;
            *next_seq += 1;
            id
        });
    }
    out.sort_by_key(|w| map.get(&w.hwnd).copied().unwrap_or(u64::MAX));
    out
}

pub fn get_active_window_label() -> String {
    unsafe {
        let fg = GetForegroundWindow();
        if is_desktop_or_shell_window(fg) {
            return "Escritorio".to_string();
        }
        let mut pid = 0u32;
        GetWindowThreadProcessId(fg, Some(&mut pid));
        let (pn, _) = get_process_info(pid);
        match pn.to_lowercase().as_str() {
            "msedge" => "Microsoft Edge".to_string(),
            "chrome" => "Google Chrome".to_string(),
            "discord" => "Discord".to_string(),
            "spotify" => "Spotify".to_string(),
            "antigravity" => "Antigravity".to_string(),
            "aurafiles" => "AuraFiles".to_string(),
            "code" => "VS Code".to_string(),
            "windowsterminal" => "Terminal".to_string(),
            "" => "Escritorio".to_string(),
            _ => pn,
        }
    }
}

// ==================== SYSTEM CONTROLS, POWER & LAUNCHING ====================

pub fn is_any_maximized_window_visible() -> bool {
    unsafe {
        let fg = GetForegroundWindow();
        if !is_desktop_or_shell_window(fg) && IsZoomed(fg).as_bool() {
            let mon = MonitorFromWindow(fg, MONITOR_DEFAULTTONEAREST);
            let mut mi = MONITORINFO {
                cbSize: std::mem::size_of::<MONITORINFO>() as u32,
                ..Default::default()
            };
            if GetMonitorInfoW(mon, &mut mi).as_bool() && (mi.dwFlags & 1) != 0 {
                return true;
            }
        }
    }
    is_fullscreen_app_active()
}

pub fn get_active_user_hwnd() -> isize {
    unsafe {
        let fg = GetForegroundWindow();
        if is_desktop_or_shell_window(fg) {
            0
        } else {
            fg.0 as isize
        }
    }
}

pub fn get_active_keyboard_hkl() -> isize {
    unsafe {
        let fg = GetForegroundWindow();
        let tid = if !fg.is_invalid() { GetWindowThreadProcessId(fg, None) } else { 0 };
        GetKeyboardLayout(tid).0 as isize
    }
}

pub fn get_installed_keyboard_layouts() -> Vec<KeyboardLayoutEntry> {
    unsafe {
        let mut list = [HKL::default(); 16];
        let count = GetKeyboardLayoutList(Some(&mut list)) as usize;
        let mut out = Vec::new();
        for hkl in list.into_iter().take(count) {
            let lang_id = (hkl.0 as usize) & 0xFFFF;
            let primary = lang_id & 0x3FF;
            let (short_code, label) = match lang_id {
                0x240A => ("ES-CO", "Español (Colombia)"),
                0x080A => ("ES-LA", "Español (Latinoamérica)"),
                0x0C0A => ("ES-ES", "Español (España)"),
                0x0409 => ("EN-US", "English (United States)"),
                0x0809 => ("EN-UK", "English (UK)"),
                0x0416 => ("PT-BR", "Português (Brasil)"),
                0x040C => ("FR", "Français"),
                0x0407 => ("DE", "Deutsch"),
                _ => match primary {
                    0x0A => ("ESP", "Español"),
                    0x09 => ("ENG", "English"),
                    0x16 => ("POR", "Português"),
                    0x0C => ("FRA", "Français"),
                    0x07 => ("DEU", "Deutsch"),
                    _ => ("KBD", "Teclado"),
                },
            };
            out.push(KeyboardLayoutEntry {
                hkl: hkl.0 as isize,
                short_code: short_code.to_string(),
                label: label.to_string(),
            });
        }
        out
    }
}

pub fn select_keyboard_layout(hkl_raw: isize) {
    unsafe {
        let hkl = HKL(hkl_raw as *mut _);
        let _ = ActivateKeyboardLayout(hkl, KLF_ACTIVATE);
        let fg = GetForegroundWindow();
        if !fg.is_invalid() {
            let _ = PostMessageW(fg, WM_INPUTLANGCHANGEREQUEST, WPARAM(0), LPARAM(hkl_raw));
        }
    }
}

pub fn get_keyboard_layout_short() -> String {
    unsafe {
        let fg = GetForegroundWindow();
        let tid = if !fg.is_invalid() {
            GetWindowThreadProcessId(fg, None)
        } else {
            0
        };
        let hkl = GetKeyboardLayout(tid);
        let lang_id = (hkl.0 as usize) & 0x3FF;
        match lang_id {
            0x0A => "ESP".to_string(),
            0x09 => "ENG".to_string(),
            0x16 => "POR".to_string(),
            0x0C => "FRA".to_string(),
            0x07 => "DEU".to_string(),
            _ => "ESP".to_string(),
        }
    }
}

pub fn cycle_keyboard_layout() {
    let layouts = get_installed_keyboard_layouts();
    if layouts.len() >= 2 {
        unsafe {
            let fg = GetForegroundWindow();
            let tid = if !fg.is_invalid() { GetWindowThreadProcessId(fg, None) } else { 0 };
            let cur = GetKeyboardLayout(tid).0 as isize;
            let idx = layouts.iter().position(|l| l.hkl == cur).unwrap_or(0);
            let next = &layouts[(idx + 1) % layouts.len()];
            select_keyboard_layout(next.hkl);
            return;
        }
    }
    unsafe {
        keybd_event(0x12, 0, KEYBD_EVENT_FLAGS(0), 0);
        keybd_event(0x10, 0, KEYBD_EVENT_FLAGS(0), 0);
        keybd_event(0x10, 0, KEYEVENTF_KEYUP, 0);
        keybd_event(0x12, 0, KEYEVENTF_KEYUP, 0);
    }
}

pub fn get_network_status() -> String {
    // Fast native check using Windows IP Helper / Tcp check fallback
    use windows::Win32::NetworkManagement::IpHelper::{
        GetAdaptersAddresses, GAA_FLAG_SKIP_ANYCAST, GAA_FLAG_SKIP_MULTICAST, IP_ADAPTER_ADDRESSES_LH,
    };
    use windows::Win32::NetworkManagement::Ndis::IfOperStatusUp;
    unsafe {
        let mut size = 16384u32;
        let mut buf = vec![0u8; size as usize];
        let ret = GetAdaptersAddresses(
            0,
            GAA_FLAG_SKIP_ANYCAST | GAA_FLAG_SKIP_MULTICAST,
            None,
            Some(buf.as_mut_ptr() as *mut IP_ADAPTER_ADDRESSES_LH),
            &mut size,
        );
        if ret == 0 {
            let mut ptr = buf.as_ptr() as *const IP_ADAPTER_ADDRESSES_LH;
            let mut has_eth = false;
            while !ptr.is_null() {
                let a = &*ptr;
                if a.OperStatus == IfOperStatusUp {
                    if a.IfType == 71 {
                        return "Wi-Fi".to_string();
                    }
                    if a.IfType == 6 {
                        has_eth = true;
                    }
                }
                ptr = a.Next;
            }
            if has_eth {
                return "Ethernet".to_string();
            }
        }
    }
    "Red".to_string()
}

/// 100% Non-Blocking Target Launcher:
/// NEVER blocks the calling UI thread or `WH_KEYBOARD_LL` hook!
/// Uses direct `Command::new` for plain `.exe` files without args, and detached `ShellExecuteExW`
/// with `SEE_MASK_FLAG_NO_UI | SEE_MASK_ASYNCOK` for `.lnk`, URLs, `ms-settings:`, and documents so
/// a broken manifest / UAC / DDE hang can NEVER freeze AuraShell!
pub fn launch_target(target: &str, args: &str) {
    let target_s = target.trim().to_string();
    if target_s.is_empty() {
        return;
    }
    let args_s = args.to_string();

    // Fast path for plain local .exe with no arguments (instant spawn, 0 ShellExecute modal risk!)
    if args_s.is_empty() && target_s.to_lowercase().ends_with(".exe") && Path::new(&target_s).is_file() {
        let mut cmd = std::process::Command::new(&target_s);
        if let Some(parent) = Path::new(&target_s).parent() {
            cmd.current_dir(parent);
        }
        if cmd.spawn().is_ok() {
            return;
        }
    }

    std::thread::spawn(move || {
        use windows::Win32::UI::Shell::{
            ShellExecuteExW, SEE_MASK_ASYNCOK, SEE_MASK_FLAG_NO_UI, SHELLEXECUTEINFOW,
        };
        unsafe {
            let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
            let op = to_wide("open");
            let file = to_wide(&target_s);
            let params = to_wide(&args_s);
            let dir_str = Path::new(&target_s)
                .parent()
                .and_then(|p| p.to_str())
                .filter(|_| Path::new(&target_s).is_file())
                .unwrap_or("")
                .to_string();
            let dir = to_wide(&dir_str);

            let mut sei = SHELLEXECUTEINFOW {
                cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
                fMask: SEE_MASK_FLAG_NO_UI | SEE_MASK_ASYNCOK,
                hwnd: HWND::default(),
                lpVerb: PCWSTR(op.as_ptr()),
                lpFile: PCWSTR(file.as_ptr()),
                lpParameters: if args_s.is_empty() { PCWSTR::null() } else { PCWSTR(params.as_ptr()) },
                lpDirectory: if dir_str.is_empty() { PCWSTR::null() } else { PCWSTR(dir.as_ptr()) },
                nShow: SW_SHOWNORMAL.0,
                ..Default::default()
            };
            if ShellExecuteExW(&mut sei).is_err() {
                // Retry without SEE_MASK_FLAG_NO_UI ONLY for UAC elevation prompts if error was elevation required (740)
                let err = windows::Win32::Foundation::GetLastError().0;
                if err == 740 {
                    let runas = to_wide("runas");
                    let _ = ShellExecuteW(
                        HWND::default(),
                        PCWSTR(runas.as_ptr()),
                        PCWSTR(file.as_ptr()),
                        if args_s.is_empty() { PCWSTR::null() } else { PCWSTR(params.as_ptr()) },
                        if dir_str.is_empty() { PCWSTR::null() } else { PCWSTR(dir.as_ptr()) },
                        SW_SHOWNORMAL,
                    );
                }
            }
            windows::Win32::System::Com::CoUninitialize();
        }
    });
}

pub fn launch_aurafiles(folder: Option<&str>) {
    if let Some(exe) = resolve_aura_app_path("AuraFiles.exe") {
        let mut cmd = std::process::Command::new(&exe);
        if let Some(f) = folder {
            if !f.trim().is_empty() {
                cmd.arg(f);
            }
        }
        if cmd.spawn().is_ok() {
            return;
        }
    }
    let arg = folder.map(|f| format!("\"{}\"", f)).unwrap_or_default();
    launch_target("explorer.exe", &arg);
}

pub fn launch_aura_settings(tab: Option<&str>) {
    // If AuraSettings is already open & no specific tab override requested, restore & bring to front!
    if tab.is_none() {
        unsafe {
            let cls = to_wide("AuraSettings_GPU_v43_Wnd");
            if let Ok(existing) = FindWindowW(PCWSTR(cls.as_ptr()), None) {
                if !existing.is_invalid() && IsWindowVisible(existing).as_bool() {
                    bring_to_front(existing);
                    return;
                }
            }
        }
    }
    if let Some(exe) = resolve_aura_app_path("AuraSettings.exe") {
        let mut cmd = std::process::Command::new(&exe);
        if let Some(t) = tab {
            if !t.trim().is_empty() {
                cmd.args(["--tab", t]);
            }
        }
        if cmd.spawn().is_ok() {
            return;
        }
    }
    launch_target("ms-settings:", "");
}

/// Cleans up any stale/hidden zombie `AuraLock.exe` process before launching a fresh lock screen.
fn cleanup_stale_auralock_if_hidden() {
    use windows::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
    };
    use windows::Win32::System::Threading::{TerminateProcess, PROCESS_TERMINATE};

    unsafe {
        let lock_title = to_wide("AuraLock");
        if let Ok(existing) = FindWindowW(None, PCWSTR(lock_title.as_ptr())) {
            if !existing.is_invalid() && IsWindowVisible(existing).as_bool() {
                // Already genuinely visible on screen — do not kill!
                return;
            }
        }
        let Ok(snap) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else { return };
        let mut pe = PROCESSENTRY32W {
            dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
            ..Default::default()
        };
        if Process32FirstW(snap, &mut pe).is_ok() {
            loop {
                let exe = from_wide(&pe.szExeFile);
                if exe.eq_ignore_ascii_case("AuraLock.exe") {
                    if let Ok(hp) = OpenProcess(PROCESS_TERMINATE, false, pe.th32ProcessID) {
                        let _ = TerminateProcess(hp, 0);
                        let _ = windows::Win32::Foundation::CloseHandle(hp);
                    }
                }
                if Process32NextW(snap, &mut pe).is_err() {
                    break;
                }
            }
        }
        let _ = windows::Win32::Foundation::CloseHandle(snap);
    }
}

pub fn launch_aura_lock() {
    unsafe {
        let lock_title = to_wide("AuraLock");
        if let Ok(existing) = FindWindowW(None, PCWSTR(lock_title.as_ptr())) {
            if !existing.is_invalid() && IsWindowVisible(existing).as_bool() {
                bring_to_front(existing);
                return;
            }
        }
    }
    cleanup_stale_auralock_if_hidden();
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()));
    let candidates = [
        exe_dir.as_ref().map(|d| d.join("AuraLock.exe")),
        Some(crate::models::app_data_dir().join("AuraLock.exe")),
    ];
    for c in candidates.iter().flatten() {
        if c.exists() {
            let _ = std::process::Command::new(c).spawn();
            return;
        }
    }
    power_lock();
}

pub fn launch_aura_tasks() {
    unsafe {
        let cls = to_wide("AuraTasks_Rust_v45");
        if let Ok(existing) = FindWindowW(PCWSTR(cls.as_ptr()), None) {
            if !existing.is_invalid() && IsWindowVisible(existing).as_bool() {
                bring_to_front(existing);
                return;
            }
        }
    }
    if let Some(exe) = resolve_aura_app_path("AuraTasks.exe") {
        if std::process::Command::new(&exe).spawn().is_ok() {
            return;
        }
    }
    launch_target("taskmgr.exe", "");
}

/// Asks running AuraShell.exe to play the macOS Genie Suction effect when minimizing an Aura app!
/// Returns true if AuraShell handled it (caller should NOT immediately ShowWindow(SW_MINIMIZE)),
/// or false if AuraShell isn't running so caller should fall back to normal ShowWindow(SW_MINIMIZE).
pub fn request_genie_minimize(hwnd: HWND) -> bool {
    use windows::Win32::UI::WindowsAndMessaging::{
        FindWindowW, PostMessageW, RegisterWindowMessageW,
    };
    unsafe {
        let cls = to_wide("AuraOS_Rust_v3_Wnd");
        let title = to_wide("AuraTopBar");
        if let Ok(shell_hw) = FindWindowW(PCWSTR(cls.as_ptr()), PCWSTR(title.as_ptr())) {
            if !shell_hw.is_invalid() {
                let m = to_wide("WM_AURAOS_GENIE_MINIMIZE");
                let msg = RegisterWindowMessageW(PCWSTR(m.as_ptr()));
                if msg != 0 && PostMessageW(shell_hw, msg, WPARAM(hwnd.0 as usize), LPARAM(0)).is_ok() {
                    return true;
                }
            }
        }
    }
    false
}

/// Broadcasts live settings reload signal to running AuraShell.exe (zero restart!)
pub fn notify_shell_settings_changed() {
    use windows::Win32::UI::WindowsAndMessaging::{RegisterWindowMessageW, SendNotifyMessageW, HWND_BROADCAST};
    unsafe {
        let m = to_wide("WM_AURAOS_SETTINGS_CHANGED");
        let msg = RegisterWindowMessageW(PCWSTR(m.as_ptr()));
        if msg != 0 {
            let _ = SendNotifyMessageW(HWND_BROADCAST, msg, WPARAM(0), LPARAM(0));
        }
    }
}

// Native Windows Power Actions
pub fn power_lock() {
    unsafe {
        let _ = LockWorkStation();
    }
}

pub fn power_sleep() {
    let _ = std::process::Command::new("rundll32.exe")
        .args(["powrprof.dll,SetSuspendState", "0,1,0"])
        .spawn();
}

pub fn power_restart() {
    restore_windows_safely();
    let _ = std::process::Command::new("shutdown.exe")
        .args(["/r", "/t", "0"])
        .spawn();
}

pub fn power_shutdown() {
    restore_windows_safely();
    let _ = std::process::Command::new("shutdown.exe")
        .args(["/s", "/t", "0"])
        .spawn();
}

// Resolve Windows .lnk shortcut target cleanly via COM IShellLinkW
pub fn resolve_shortcut_target(lnk_path: &str) -> Option<String> {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER).ok()?;
        let persist: IPersistFile = windows::core::Interface::cast(&link).ok()?;
        let w = to_wide(lnk_path);
        persist.Load(PCWSTR(w.as_ptr()), STGM(0)).ok()?;
        let mut target = [0u16; MAX_PATH as usize * 2];
        link.GetPath(&mut target, std::ptr::null_mut(), 0).ok()?;
        let out = from_wide(&target);
        if !out.is_empty() && Path::new(&out).exists() {
            Some(out)
        } else {
            None
        }
    }
}

// Native Windows 11 File Open Dialog (for "+ Agregar IA / App" and "+ Agregar juego")
pub fn pick_file_dialog(title: &str, filter_label: &str, filter_spec: &str) -> Option<String> {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let dialog: IFileOpenDialog = CoCreateInstance(&FileOpenDialog, None, CLSCTX_ALL).ok()?;
        let w_title = to_wide(title);
        let _ = dialog.SetTitle(PCWSTR(w_title.as_ptr()));
        let w_label = to_wide(filter_label);
        let w_spec = to_wide(filter_spec);
        let specs = [COMDLG_FILTERSPEC {
            pszName: PCWSTR(w_label.as_ptr()),
            pszSpec: PCWSTR(w_spec.as_ptr()),
        }];
        let _ = dialog.SetFileTypes(&specs);
        if let Ok(opts) = dialog.GetOptions() {
            let _ = dialog.SetOptions(opts | FOS_FILEMUSTEXIST | FOS_NOCHANGEDIR);
        }
        dialog.Show(HWND::default()).ok()?;
        let item = dialog.GetResult().ok()?;
        let pwstr: PWSTR = item.GetDisplayName(SIGDN_FILESYSPATH).ok()?;
        let path = pwstr.to_string().ok();
        windows::Win32::System::Com::CoTaskMemFree(Some(pwstr.0 as *const _));
        path
    }
}

/// Native Windows 11 File Save Dialog (used by AuraPDF, AuraPhotos Studio & AuraShot!)
pub fn save_file_dialog(
    title: &str,
    filter_label: &str,
    filter_spec: &str,
    default_ext: &str,
    default_name: &str,
) -> Option<String> {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let dialog: IFileSaveDialog = CoCreateInstance(&FileSaveDialog, None, CLSCTX_ALL).ok()?;
        let w_title = to_wide(title);
        let _ = dialog.SetTitle(PCWSTR(w_title.as_ptr()));
        let w_label = to_wide(filter_label);
        let w_spec = to_wide(filter_spec);
        let specs = [COMDLG_FILTERSPEC {
            pszName: PCWSTR(w_label.as_ptr()),
            pszSpec: PCWSTR(w_spec.as_ptr()),
        }];
        let _ = dialog.SetFileTypes(&specs);
        let w_ext = to_wide(default_ext);
        let _ = dialog.SetDefaultExtension(PCWSTR(w_ext.as_ptr()));
        let w_name = to_wide(default_name);
        let _ = dialog.SetFileName(PCWSTR(w_name.as_ptr()));
        if let Ok(opts) = dialog.GetOptions() {
            let _ = dialog.SetOptions(opts | FOS_OVERWRITEPROMPT | FOS_NOCHANGEDIR);
        }
        dialog.Show(HWND::default()).ok()?;
        let item = dialog.GetResult().ok()?;
        let pwstr: PWSTR = item.GetDisplayName(SIGDN_FILESYSPATH).ok()?;
        let path = pwstr.to_string().ok();
        windows::Win32::System::Com::CoTaskMemFree(Some(pwstr.0 as *const _));
        path
    }
}

/// Copies UTF-8 text into the native Win32 Clipboard (`CF_UNICODETEXT`).
pub fn copy_text_to_clipboard(text: &str) -> bool {
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::System::DataExchange::{
        CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData,
    };
    #[link(name = "kernel32")]
    extern "system" {
        fn GlobalAlloc(uFlags: u32, dwBytes: usize) -> *mut core::ffi::c_void;
        fn GlobalLock(hMem: *mut core::ffi::c_void) -> *mut core::ffi::c_void;
        fn GlobalUnlock(hMem: *mut core::ffi::c_void) -> i32;
    }
    const GMEM_MOVEABLE: u32 = 0x0002;
    const CF_UNICODETEXT: u32 = 13;

    let wide = to_wide(text);
    let byte_len = wide.len() * 2;
    unsafe {
        if OpenClipboard(HWND::default()).is_err() {
            return false;
        }
        let _ = EmptyClipboard();
        let hmem = GlobalAlloc(GMEM_MOVEABLE, byte_len);
        if hmem.is_null() {
            let _ = CloseClipboard();
            return false;
        }
        let dst = GlobalLock(hmem);
        if dst.is_null() {
            let _ = CloseClipboard();
            return false;
        }
        std::ptr::copy_nonoverlapping(wide.as_ptr() as *const u8, dst as *mut u8, byte_len);
        GlobalUnlock(hmem);
        let ok = SetClipboardData(CF_UNICODETEXT, HANDLE(hmem)).is_ok();
        let _ = CloseClipboard();
        ok
    }
}

/// Reads current UTF-8 text from Win32 Clipboard (`CF_UNICODETEXT`), if present.
pub fn read_clipboard_text() -> Option<String> {
    use windows::Win32::System::DataExchange::{
        CloseClipboard, GetClipboardData, IsClipboardFormatAvailable, OpenClipboard,
    };
    #[link(name = "kernel32")]
    extern "system" {
        fn GlobalLock(hMem: *mut core::ffi::c_void) -> *mut core::ffi::c_void;
        fn GlobalUnlock(hMem: *mut core::ffi::c_void) -> i32;
    }
    const CF_UNICODETEXT: u32 = 13;
    unsafe {
        if IsClipboardFormatAvailable(CF_UNICODETEXT).is_err() {
            return None;
        }
        if OpenClipboard(HWND::default()).is_err() {
            return None;
        }
        let res = if let Ok(h) = GetClipboardData(CF_UNICODETEXT) {
            let ptr = GlobalLock(h.0) as *const u16;
            if !ptr.is_null() {
                let mut len = 0usize;
                while len < 65536 && *ptr.add(len) != 0 {
                    len += 1;
                }
                let s = String::from_utf16_lossy(std::slice::from_raw_parts(ptr, len));
                GlobalUnlock(h.0);
                Some(s)
            } else {
                None
            }
        } else {
            None
        };
        let _ = CloseClipboard();
        res.filter(|s| !s.trim().is_empty())
    }
}

/// Copies a 32-bit BGRA image buffer directly into the Win32 Clipboard (`CF_DIB`) for 1-click paste!
pub fn copy_bgra_to_clipboard(width: u32, height: u32, bgra: &[u8]) -> bool {
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::Graphics::Gdi::{BITMAPINFOHEADER, BI_RGB};
    use windows::Win32::System::DataExchange::{
        CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData,
    };
    #[link(name = "kernel32")]
    extern "system" {
        fn GlobalAlloc(uFlags: u32, dwBytes: usize) -> *mut core::ffi::c_void;
        fn GlobalLock(hMem: *mut core::ffi::c_void) -> *mut core::ffi::c_void;
        fn GlobalUnlock(hMem: *mut core::ffi::c_void) -> i32;
    }
    const GMEM_MOVEABLE: u32 = 0x0002;
    const CF_DIB: u32 = 8;

    if width == 0 || height == 0 || bgra.len() < (width as usize * height as usize * 4) {
        return false;
    }
    let hdr_sz = std::mem::size_of::<BITMAPINFOHEADER>();
    let px_bytes = width as usize * height as usize * 4;
    let total = hdr_sz + px_bytes;

    let hdr = BITMAPINFOHEADER {
        biSize: hdr_sz as u32,
        biWidth: width as i32,
        biHeight: height as i32, // bottom-up DIB for maximum compatibility across Discord/Paint/Browsers!
        biPlanes: 1,
        biBitCount: 32,
        biCompression: BI_RGB.0,
        biSizeImage: px_bytes as u32,
        ..Default::default()
    };

    unsafe {
        if OpenClipboard(HWND::default()).is_err() {
            return false;
        }
        let _ = EmptyClipboard();
        let hmem = GlobalAlloc(GMEM_MOVEABLE, total);
        if hmem.is_null() {
            let _ = CloseClipboard();
            return false;
        }
        let dst = GlobalLock(hmem) as *mut u8;
        if dst.is_null() {
            let _ = CloseClipboard();
            return false;
        }
        std::ptr::copy_nonoverlapping(&hdr as *const _ as *const u8, dst, hdr_sz);
        let row_bytes = width as usize * 4;
        for y in 0..height as usize {
            let src_row = &bgra[(height as usize - 1 - y) * row_bytes..(height as usize - y) * row_bytes];
            std::ptr::copy_nonoverlapping(src_row.as_ptr(), dst.add(hdr_sz + y * row_bytes), row_bytes);
        }
        GlobalUnlock(hmem);
        let ok = SetClipboardData(CF_DIB, HANDLE(hmem)).is_ok();
        let _ = CloseClipboard();
        ok
    }
}

pub fn is_pdf_ext(ext: &str) -> bool {
    ext.eq_ignore_ascii_case("pdf")
}

pub fn launch_aura_pdf(file: Option<&str>) {
    if !launch_aura_app("AuraPDF.exe", file) {
        if let Some(f) = file {
            launch_target(f, "");
        }
    }
}

pub fn launch_aura_shot() {
    let _ = launch_aura_app("AuraShot.exe", None);
}

// ==================== AURAOS v4.0 NATIVE SUITE ROUTER ====================

pub fn is_image_ext(ext: &str) -> bool {
    matches!(
        ext.to_lowercase().as_str(),
        "png" | "jpg" | "jpeg" | "webp" | "bmp" | "gif" | "ico" | "tif" | "tiff"
    )
}

pub fn is_video_ext(ext: &str) -> bool {
    matches!(
        ext.to_lowercase().as_str(),
        "mp4" | "mkv" | "webm" | "mov" | "avi" | "wmv" | "flv" | "m4v"
    )
}

pub fn is_audio_ext(ext: &str) -> bool {
    matches!(
        ext.to_lowercase().as_str(),
        "mp3" | "wav" | "flac" | "ogg" | "m4a" | "aac"
    )
}

pub fn is_text_code_ext(ext: &str) -> bool {
    matches!(
        ext.to_lowercase().as_str(),
        "txt" | "md" | "json" | "log" | "ini" | "cfg" | "toml" | "yaml" | "yml" | "xml"
            | "csv" | "ps1" | "bat" | "cmd" | "sh" | "rs" | "py" | "js" | "ts" | "jsx"
            | "tsx" | "cs" | "c" | "cpp" | "h" | "hpp" | "html" | "css" | "sql" | "env"
    )
}

pub fn resolve_aura_app_path(exe_name: &str) -> Option<std::path::PathBuf> {
    if let Ok(cur) = std::env::current_exe() {
        if let Some(dir) = cur.parent() {
            let p = dir.join(exe_name);
            if p.exists() {
                return Some(p);
            }
        }
    }
    let p = crate::models::app_data_dir().join(exe_name);
    if p.exists() {
        return Some(p);
    }
    None
}

pub fn launch_aura_app(exe_name: &str, file_arg: Option<&str>) -> bool {
    let Some(exe) = resolve_aura_app_path(exe_name) else {
        return false;
    };
    let mut cmd = std::process::Command::new(&exe);
    if let Some(a) = file_arg {
        if !a.trim().is_empty() {
            cmd.arg(a);
        }
    }
    cmd.spawn().is_ok()
}

pub fn is_daw_project_ext(ext: &str) -> bool {
    matches!(ext.to_lowercase().as_str(), "flp" | "lsp")
}

pub fn resolve_aurabeats_path() -> Option<std::path::PathBuf> {
    if let Ok(local) = std::env::var("LOCALAPPDATA") {
        let p = Path::new(&local).join("AuraBeats").join("AuraBeats.exe");
        if p.exists() {
            return Some(p);
        }
    }
    if let Some(p) = resolve_aura_app_path("AuraBeats.exe") {
        return Some(p);
    }
    let dev = std::path::PathBuf::from(
        r"C:\Users\danil\.gemini\antigravity\scratch\AuraBeats\target\release\AuraBeats.exe",
    );
    if dev.exists() {
        return Some(dev);
    }
    None
}

pub fn resolve_aurabeats_icon_path() -> Option<std::path::PathBuf> {
    if let Ok(local) = std::env::var("LOCALAPPDATA") {
        let p1 = Path::new(&local).join("AuraBeats").join("AuraBeats_256.png");
        if p1.exists() {
            return Some(p1);
        }
        let p2 = Path::new(&local)
            .join("AuraShell")
            .join("assets")
            .join("icons")
            .join("AuraBeats_256.png");
        if p2.exists() {
            return Some(p2);
        }
    }
    if let Ok(cur) = std::env::current_exe() {
        if let Some(dir) = cur.parent() {
            let p1 = dir.join("AuraBeats_256.png");
            if p1.exists() {
                return Some(p1);
            }
            let p2 = dir.join("assets").join("icons").join("AuraBeats_256.png");
            if p2.exists() {
                return Some(p2);
            }
        }
    }
    let dev = std::path::PathBuf::from(
        r"C:\Users\danil\.gemini\antigravity\scratch\AuraOS-Rust\assets\icons\AuraBeats_256.png",
    );
    if dev.exists() {
        return Some(dev);
    }
    None
}

pub fn launch_aurabeats(project_file: Option<&str>) -> bool {
    let Some(exe) = resolve_aurabeats_path() else {
        return false;
    };
    let mut cmd = std::process::Command::new(&exe);
    if let Some(f) = project_file {
        if !f.trim().is_empty() {
            cmd.arg(f);
        }
    }
    cmd.spawn().is_ok()
}

pub fn count_aurabeats_plugins() -> usize {
    let mut dirs = Vec::new();
    if let Ok(local) = std::env::var("LOCALAPPDATA") {
        dirs.push(Path::new(&local).join(r"Programs\Common\CLAP"));
        dirs.push(Path::new(&local).join(r"Programs\Common\VST3"));
    }
    dirs.push(std::path::PathBuf::from(r"C:\Program Files\Common Files\CLAP"));
    dirs.push(std::path::PathBuf::from(r"C:\Program Files\Common Files\VST3"));

    let mut seen = std::collections::HashSet::new();
    for dir in dirs {
        let Ok(rd) = std::fs::read_dir(&dir) else { continue };
        for entry in rd.flatten() {
            let p = entry.path();
            let ext = p
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_lowercase();
            if ext == "clap" || ext == "vst3" {
                if let Some(stem) = p.file_stem().and_then(|s| s.to_str()) {
                    seen.insert(stem.to_lowercase());
                }
            }
        }
    }
    seen.len()
}

/// Smart AuraOS File Router: opens Images in AuraPhotos, Videos/Audio in AuraPlayer,
/// DAW Projects (.flp/.lsp) in AuraBeats, Text/Code in AuraNotes, and everything else via Windows ShellExecuteW!
pub fn open_path_with_aura_suite(path: &Path) {
    let path_str = path.to_string_lossy().to_string();
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    if is_daw_project_ext(ext) && launch_aurabeats(Some(&path_str)) {
        return;
    }
    if is_pdf_ext(ext) && launch_aura_app("AuraPDF.exe", Some(&path_str)) {
        return;
    }
    if is_image_ext(ext) && launch_aura_app("AuraPhotos.exe", Some(&path_str)) {
        return;
    }
    if (is_video_ext(ext) || is_audio_ext(ext)) && launch_aura_app("AuraPlayer.exe", Some(&path_str)) {
        return;
    }
    if is_text_code_ext(ext) && launch_aura_app("AuraNotes.exe", Some(&path_str)) {
        return;
    }
    launch_target(&path_str, "");
}

pub fn find_ffmpeg_tool(name: &str) -> Option<std::path::PathBuf> {
    if let Ok(cur) = std::env::current_exe() {
        if let Some(d) = cur.parent() {
            let p = d.join(name);
            if p.exists() {
                return Some(p);
            }
        }
    }
    if let Ok(path_env) = std::env::var("PATH") {
        for dir in path_env.split(';') {
            let p = Path::new(dir).join(name);
            if p.exists() {
                return Some(p);
            }
        }
    }
    if let Ok(local) = std::env::var("LOCALAPPDATA") {
        let winget_links = Path::new(&local).join(r"Microsoft\WinGet\Links").join(name);
        if winget_links.exists() {
            return Some(winget_links);
        }
        let pkgs = Path::new(&local).join(r"Microsoft\WinGet\Packages");
        if let Ok(entries) = std::fs::read_dir(pkgs) {
            for e in entries.flatten() {
                let n = e.file_name().to_string_lossy().to_lowercase();
                if n.contains("ffmpeg") {
                    for sub in walk_find_exe(&e.path(), name, 3) {
                        return Some(sub);
                    }
                }
            }
        }
    }
    for cand in [
        format!(r"C:\ffmpeg\bin\{}", name),
        format!(r"C:\Program Files\ffmpeg\bin\{}", name),
    ] {
        let p = std::path::PathBuf::from(cand);
        if p.exists() {
            return Some(p);
        }
    }
    None
}

fn walk_find_exe(dir: &Path, name: &str, depth: usize) -> Vec<std::path::PathBuf> {
    let mut found = Vec::new();
    if depth == 0 {
        return found;
    }
    if let Ok(rd) = std::fs::read_dir(dir) {
        for e in rd.flatten() {
            let p = e.path();
            if p.is_file() && p.file_name().map(|f| f.to_string_lossy().eq_ignore_ascii_case(name)).unwrap_or(false) {
                found.push(p);
                return found;
            } else if p.is_dir() {
                found.extend(walk_find_exe(&p, name, depth - 1));
                if !found.is_empty() {
                    return found;
                }
            }
        }
    }
    found
}

pub fn move_to_recycle_bin(path: &str) -> bool {
    use windows::Win32::UI::Shell::{SHFileOperationW, FOF_ALLOWUNDO, FOF_NOCONFIRMATION, FOF_SILENT, FO_DELETE, SHFILEOPSTRUCTW};
    let mut wide: Vec<u16> = path.encode_utf16().collect();
    wide.push(0);
    wide.push(0); // double-null terminated required by SHFileOperationW
    let mut op = SHFILEOPSTRUCTW {
        hwnd: HWND::default(),
        wFunc: FO_DELETE,
        pFrom: PCWSTR(wide.as_ptr()),
        pTo: PCWSTR::null(),
        fFlags: (FOF_ALLOWUNDO | FOF_NOCONFIRMATION | FOF_SILENT).0 as u16,
        fAnyOperationsAborted: BOOL(0),
        hNameMappings: std::ptr::null_mut(),
        lpszProgressTitle: PCWSTR::null(),
    };
    unsafe { SHFileOperationW(&mut op) == 0 && !op.fAnyOperationsAborted.as_bool() }
}

// ==================== v5.3 EXPLORER SHELL DIALOGS & ELEVATED PROCESS TERMINATION ====================

/// Opens the REAL native Windows Properties dialog for any file or folder (non-blocking!).
pub fn show_file_properties(path: &str) {
    let p_owned = path.to_string();
    std::thread::spawn(move || {
        use windows::Win32::UI::Shell::{ShellExecuteExW, SEE_MASK_INVOKEIDLIST, SHELLEXECUTEINFOW};
        unsafe {
            let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
            let verb = to_wide("properties");
            let file = to_wide(&p_owned);
            let mut sei = SHELLEXECUTEINFOW {
                cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
                fMask: SEE_MASK_INVOKEIDLIST,
                hwnd: HWND::default(),
                lpVerb: PCWSTR(verb.as_ptr()),
                lpFile: PCWSTR(file.as_ptr()),
                nShow: SW_SHOW.0,
                ..Default::default()
            };
            let _ = ShellExecuteExW(&mut sei);
            // Keep thread alive briefly so the shell properties modal has time to display
            std::thread::sleep(std::time::Duration::from_millis(600));
            windows::Win32::System::Com::CoUninitialize();
        }
    });
}

/// Opens Windows' native "Abrir con..." (Open With) application chooser dialog!
pub fn open_with_dialog(path: &str) {
    let _ = std::process::Command::new("rundll32.exe")
        .args(["shell32.dll,OpenAs_RunDLL", path])
        .spawn();
}

/// Opens Windows File Explorer and highlights the specified file or folder!
pub fn reveal_in_windows_explorer(path: &str) {
    let arg = format!("/select,\"{}\"", path);
    let _ = std::process::Command::new("explorer.exe").arg(arg).spawn();
}

/// Opens Windows Terminal (`wt.exe`) if installed, or PowerShell cleanly at `dir`!
pub fn open_terminal_at(dir: &str) {
    let clean_dir = if Path::new(dir).is_dir() {
        dir.to_string()
    } else {
        Path::new(dir)
            .parent()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| "C:\\".to_string())
    };
    if std::process::Command::new("wt.exe")
        .args(["-d", &clean_dir])
        .spawn()
        .is_ok()
    {
        return;
    }
    let ps_cmd = format!("Set-Location -LiteralPath '{}'", clean_dir.replace('\'', "''"));
    let _ = std::process::Command::new("powershell.exe")
        .args(["-NoExit", "-Command", &ps_cmd])
        .spawn();
}

/// Runs an executable elevated via UAC (`ShellExecuteW("runas")`) in a detached thread!
pub fn run_as_admin(exe: &str, args: &str) {
    let exe_s = exe.to_string();
    let args_s = args.to_string();
    std::thread::spawn(move || unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let verb = to_wide("runas");
        let file = to_wide(&exe_s);
        let params = to_wide(&args_s);
        let _ = ShellExecuteW(
            HWND::default(),
            PCWSTR(verb.as_ptr()),
            PCWSTR(file.as_ptr()),
            if args_s.is_empty() { PCWSTR::null() } else { PCWSTR(params.as_ptr()) },
            PCWSTR::null(),
            SW_SHOWNORMAL,
        );
        windows::Win32::System::Com::CoUninitialize();
    });
}

/// Returns `true` if the current process is running with Administrator (elevated UAC) privileges!
pub fn is_process_elevated() -> bool {
    use windows::Win32::Foundation::{CloseHandle, HANDLE};
    use windows::Win32::Security::{GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY};
    use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
    unsafe {
        let mut token = HANDLE::default();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token).is_err() {
            return false;
        }
        let mut elev = TOKEN_ELEVATION::default();
        let mut ret_len = 0u32;
        let ok = GetTokenInformation(
            token,
            TokenElevation,
            Some(&mut elev as *mut TOKEN_ELEVATION as *mut _),
            std::mem::size_of::<TOKEN_ELEVATION>() as u32,
            &mut ret_len,
        )
        .is_ok();
        let _ = CloseHandle(token);
        ok && elev.TokenIsElevated != 0
    }
}

/// Enables `SeDebugPrivilege` on the current process token (succeeds when elevated),
/// granting maximum Windows permission to inspect, suspend, and terminate stubborn processes!
pub fn enable_debug_privilege() -> bool {
    use windows::Win32::Foundation::{CloseHandle, HANDLE, LUID};
    use windows::Win32::Security::{
        AdjustTokenPrivileges, LookupPrivilegeValueW, LUID_AND_ATTRIBUTES, SE_PRIVILEGE_ENABLED,
        TOKEN_ADJUST_PRIVILEGES, TOKEN_PRIVILEGES, TOKEN_QUERY,
    };
    use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
    unsafe {
        let mut token = HANDLE::default();
        if OpenProcessToken(
            GetCurrentProcess(),
            TOKEN_ADJUST_PRIVILEGES | TOKEN_QUERY,
            &mut token,
        )
        .is_err()
        {
            return false;
        }
        let mut luid = LUID::default();
        let priv_name = to_wide("SeDebugPrivilege");
        let ok = if LookupPrivilegeValueW(PCWSTR::null(), PCWSTR(priv_name.as_ptr()), &mut luid).is_ok() {
            let tp = TOKEN_PRIVILEGES {
                PrivilegeCount: 1,
                Privileges: [LUID_AND_ATTRIBUTES {
                    Luid: luid,
                    Attributes: SE_PRIVILEGE_ENABLED,
                }],
            };
            AdjustTokenPrivileges(token, false, Some(&tp), 0, None, None).is_ok()
                && windows::Win32::Foundation::GetLastError().0 == 0
        } else {
            false
        };
        let _ = CloseHandle(token);
        ok
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KillOutcome {
    Killed,
    KilledElevated,
    CancelledByUser,
    ProtectedOrDenied,
    NotFound,
}

fn collect_descendant_pids(root_pid: u32) -> Vec<u32> {
    use windows::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
    };
    let mut edges: Vec<(u32, u32)> = Vec::new();
    unsafe {
        let Ok(snap) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else { return Vec::new() };
        let mut pe = PROCESSENTRY32W {
            dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
            ..Default::default()
        };
        if Process32FirstW(snap, &mut pe).is_ok() {
            loop {
                edges.push((pe.th32ParentProcessID, pe.th32ProcessID));
                if Process32NextW(snap, &mut pe).is_err() {
                    break;
                }
            }
        }
        let _ = windows::Win32::Foundation::CloseHandle(snap);
    }
    let mut visited = std::collections::HashSet::new();
    let mut stack = vec![root_pid];
    let mut out = Vec::new();
    visited.insert(root_pid);
    while let Some(cur) = stack.pop() {
        for &(ppid, pid) in &edges {
            if ppid == cur && pid != 0 && visited.insert(pid) {
                out.push(pid);
                stack.push(pid);
            }
        }
    }
    out
}

fn is_pid_alive(pid: u32) -> bool {
    use windows::Win32::Foundation::STILL_ACTIVE;
    use windows::Win32::System::Threading::GetExitCodeProcess;
    unsafe {
        if let Ok(h) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) {
            let mut code = 0u32;
            let alive = GetExitCodeProcess(h, &mut code).is_ok() && code == STILL_ACTIVE.0 as u32;
            let _ = windows::Win32::Foundation::CloseHandle(h);
            alive
        } else {
            false
        }
    }
}

/// Terminates a process (and optionally its entire child tree) using:
/// 1. Native `OpenProcess(PROCESS_TERMINATE)` + `TerminateProcess` (instant, 0ms)
/// 2. If `ERROR_ACCESS_DENIED` and not elevated: automatic UAC-elevated `taskkill /F [/T] /PID <pid>`
///    with `SW_HIDE`, waiting cleanly for the result!
/// 3. Distinguishes user-cancelled UAC vs kernel PPL / anti-cheat protected processes!
pub fn force_kill_pid(pid: u32, kill_tree: bool) -> KillOutcome {
    use windows::Win32::System::Threading::{
        GetExitCodeProcess, TerminateProcess, WaitForSingleObject, PROCESS_TERMINATE,
    };
    use windows::Win32::UI::Shell::{ShellExecuteExW, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW};

    if pid == 0 || pid == 4 {
        return KillOutcome::ProtectedOrDenied;
    }

    // 1. Direct native termination first (kills children first if kill_tree)
    let mut targets = if kill_tree {
        let mut c = collect_descendant_pids(pid);
        c.reverse();
        c.push(pid);
        c
    } else {
        vec![pid]
    };
    targets.dedup();

    let mut direct_root_ok = false;
    let mut saw_access_denied = false;
    unsafe {
        for &tpid in &targets {
            match OpenProcess(PROCESS_TERMINATE, false, tpid) {
                Ok(h) => {
                    let ok = TerminateProcess(h, 1).is_ok();
                    let _ = windows::Win32::Foundation::CloseHandle(h);
                    if tpid == pid && ok {
                        direct_root_ok = true;
                    }
                }
                Err(_) => {
                    let err = windows::Win32::Foundation::GetLastError().0;
                    if err == 5 {
                        saw_access_denied = true;
                    }
                }
            }
        }
    }

    if direct_root_ok && !saw_access_denied {
        return KillOutcome::Killed;
    }
    if !is_pid_alive(pid) {
        return if direct_root_ok { KillOutcome::Killed } else { KillOutcome::NotFound };
    }

    // 2. If already elevated, try `taskkill /F /T /PID` directly (in case SeDebugPrivilege or session helps)
    if is_process_elevated() {
        use std::os::windows::process::CommandExt;
        let args: Vec<String> = if kill_tree {
            vec!["/F".into(), "/T".into(), "/PID".into(), pid.to_string()]
        } else {
            vec!["/F".into(), "/PID".into(), pid.to_string()]
        };
        let ok = std::process::Command::new("taskkill.exe")
            .args(&args)
            .creation_flags(0x0800_0000)
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        return if ok || !is_pid_alive(pid) {
            KillOutcome::Killed
        } else {
            KillOutcome::ProtectedOrDenied
        };
    }

    // 3. Automatic UAC-elevated `taskkill` fallback when standard permissions are denied!
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let verb = to_wide("runas");
        let file = to_wide("taskkill.exe");
        let params_s = if kill_tree {
            format!("/F /T /PID {}", pid)
        } else {
            format!("/F /PID {}", pid)
        };
        let params = to_wide(&params_s);
        let mut sei = SHELLEXECUTEINFOW {
            cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
            fMask: SEE_MASK_NOCLOSEPROCESS,
            hwnd: HWND::default(),
            lpVerb: PCWSTR(verb.as_ptr()),
            lpFile: PCWSTR(file.as_ptr()),
            lpParameters: PCWSTR(params.as_ptr()),
            nShow: SW_HIDE.0,
            ..Default::default()
        };
        if ShellExecuteExW(&mut sei).is_err() {
            let err = windows::Win32::Foundation::GetLastError().0;
            windows::Win32::System::Com::CoUninitialize();
            return if err == 1223 {
                KillOutcome::CancelledByUser
            } else {
                KillOutcome::ProtectedOrDenied
            };
        }
        let mut exit_code = 1u32;
        if !sei.hProcess.is_invalid() {
            let _ = WaitForSingleObject(sei.hProcess, 8000);
            let _ = GetExitCodeProcess(sei.hProcess, &mut exit_code);
            let _ = windows::Win32::Foundation::CloseHandle(sei.hProcess);
        }
        windows::Win32::System::Com::CoUninitialize();
        if exit_code == 0 || !is_pid_alive(pid) {
            KillOutcome::KilledElevated
        } else {
            KillOutcome::ProtectedOrDenied
        }
    }
}

/// Elevated helper to start/stop a Windows Service via UAC `sc.exe` when SCM denies standard access!
pub fn control_service_elevated(svc_name: &str, stop: bool) -> bool {
    use windows::Win32::System::Threading::{GetExitCodeProcess, WaitForSingleObject};
    use windows::Win32::UI::Shell::{ShellExecuteExW, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW};
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let verb = to_wide("runas");
        let file = to_wide("sc.exe");
        let params_s = format!("{} \"{}\"", if stop { "stop" } else { "start" }, svc_name);
        let params = to_wide(&params_s);
        let mut sei = SHELLEXECUTEINFOW {
            cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
            fMask: SEE_MASK_NOCLOSEPROCESS,
            hwnd: HWND::default(),
            lpVerb: PCWSTR(verb.as_ptr()),
            lpFile: PCWSTR(file.as_ptr()),
            lpParameters: PCWSTR(params.as_ptr()),
            nShow: SW_HIDE.0,
            ..Default::default()
        };
        if ShellExecuteExW(&mut sei).is_err() {
            windows::Win32::System::Com::CoUninitialize();
            return false;
        }
        let mut code = 1u32;
        if !sei.hProcess.is_invalid() {
            let _ = WaitForSingleObject(sei.hProcess, 8000);
            let _ = GetExitCodeProcess(sei.hProcess, &mut code);
            let _ = windows::Win32::Foundation::CloseHandle(sei.hProcess);
        }
        windows::Win32::System::Com::CoUninitialize();
        code == 0
    }
}
