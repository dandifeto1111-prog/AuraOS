//! AuraOS v4.4 Shared Native Windows 11 Window Manager & Aero Snap Engine
//! - Enables TRUE edge & corner resizing (WS_THICKFRAME), native resize cursors, Windows 11 Aero Snap
//!   (Left 50%, Right 50%, 4 Quarter Corners 25%, Top Maximize), Win+Arrow tiling & Snap Assist
//! - Strips 100% of native Win32 titlebar / white border via WM_NCCALCSIZE = 0 + DWMWA_COLOR_NONE
//! - Respects AuraTopBar's 34px Work Area cleanly when maximized or tiled!

use windows::core::PCWSTR;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Dwm::{
    DwmSetWindowAttribute, DWMWA_BORDER_COLOR, DWMWA_WINDOW_CORNER_PREFERENCE,
    DWMWCP_DONOTROUND,
};
use windows::Win32::Graphics::Gdi::{
    GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTONEAREST,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    GetWindowRect, IsZoomed, LoadImageW, SendMessageW, SetWindowPos, ShowWindow, HTBOTTOM,
    HTBOTTOMLEFT, HTBOTTOMRIGHT, HTCAPTION, HTCLIENT, HTLEFT, HTRIGHT, HTTOP, HTTOPLEFT,
    HTTOPRIGHT, ICON_BIG, ICON_SMALL, IMAGE_ICON, LR_DEFAULTSIZE, LR_SHARED, MINMAXINFO,
    NCCALCSIZE_PARAMS, SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER,
    SW_MAXIMIZE, SW_RESTORE, WINDOW_STYLE, WM_SETICON, WS_CAPTION, WS_MAXIMIZEBOX, WS_MINIMIZEBOX,
    WS_POPUP, WS_SYSMENU, WS_THICKFRAME,
};

/// Standard frameless yet 100% Windows-11-resizable & Aero-Snappable style for all AuraOS apps!
pub const AURA_APP_STYLE: WINDOW_STYLE = WINDOW_STYLE(
    WS_POPUP.0
        | WS_THICKFRAME.0
        | WS_CAPTION.0
        | WS_MINIMIZEBOX.0
        | WS_MAXIMIZEBOX.0
        | WS_SYSMENU.0,
);

/// DWMWA_COLOR_NONE (0xFFFFFFFE): hides Windows 11's native 1px frame border so Direct2D renders cleanly!
const DWMWA_COLOR_NONE: u32 = 0xFFFF_FFFE;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SnapZone {
    LeftHalf,
    RightHalf,
    TopHalf,
    BottomHalf,
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
    Maximize,
    Center,
}

impl SnapZone {
    pub fn next_cycle(cur: Option<SnapZone>) -> SnapZone {
        match cur {
            None => SnapZone::LeftHalf,
            Some(SnapZone::LeftHalf) => SnapZone::RightHalf,
            Some(SnapZone::RightHalf) => SnapZone::TopLeft,
            Some(SnapZone::TopLeft) => SnapZone::TopRight,
            Some(SnapZone::TopRight) => SnapZone::BottomLeft,
            Some(SnapZone::BottomLeft) => SnapZone::BottomRight,
            Some(SnapZone::BottomRight) => SnapZone::Center,
            _ => SnapZone::LeftHalf,
        }
    }
}

/// Binds the executable's embedded resource icon (ID = 1) to the window so Alt+Tab, Taskbar,
/// and AuraDock ALWAYS display the crisp native AuraOS squircle icon instead of a blank icon!
pub fn apply_app_icon(hwnd: HWND) {
    unsafe {
        if let Ok(hinst) = GetModuleHandleW(None) {
            let res_id = PCWSTR(1usize as *const u16);
            if let Ok(big) = LoadImageW(hinst, res_id, IMAGE_ICON, 48, 48, LR_SHARED) {
                if !big.is_invalid() {
                    let _ = SendMessageW(hwnd, WM_SETICON, WPARAM(ICON_BIG as usize), LPARAM(big.0 as isize));
                }
            }
            if let Ok(small) = LoadImageW(hinst, res_id, IMAGE_ICON, 16, 16, LR_SHARED | LR_DEFAULTSIZE) {
                if !small.is_invalid() {
                    let _ = SendMessageW(hwnd, WM_SETICON, WPARAM(ICON_SMALL as usize), LPARAM(small.0 as isize));
                }
            }
        }
    }
}

/// Disables DWM's grey border & native rounding so our Direct2D rounded canvas has ZERO artifacts!
pub fn configure_frameless_dwm(hwnd: HWND) {
    apply_app_icon(hwnd);
    unsafe {
        let none_col = DWMWA_COLOR_NONE;
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_BORDER_COLOR,
            &none_col as *const u32 as *const _,
            std::mem::size_of::<u32>() as u32,
        );
        let pref = DWMWCP_DONOTROUND;
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_WINDOW_CORNER_PREFERENCE,
            &pref as *const _ as *const _,
            std::mem::size_of_val(&pref) as u32,
        );
        let _ = SetWindowPos(
            hwnd,
            HWND::default(),
            0,
            0,
            0,
            0,
            SWP_FRAMECHANGED | SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
        );
    }
}

pub fn monitor_work_rect(hwnd: HWND) -> RECT {
    unsafe {
        let hmon = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
        let mut mi = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if GetMonitorInfoW(hmon, &mut mi).as_bool() {
            return mi.rcWork;
        }
    }
    RECT {
        left: 0,
        top: 34,
        right: 1920,
        bottom: 1080,
    }
}

/// Strips the standard Win32 non-client frame completely while keeping exact rcWork bounds when maximized!
pub unsafe fn handle_nccalcsize(hwnd: HWND, wparam: WPARAM, lparam: LPARAM) -> Option<LRESULT> {
    if wparam.0 == 0 {
        return Some(LRESULT(0));
    }
    if IsZoomed(hwnd).as_bool() && lparam.0 != 0 {
        let params = &mut *(lparam.0 as *mut NCCALCSIZE_PARAMS);
        params.rgrc[0] = monitor_work_rect(hwnd);
    }
    Some(LRESULT(0))
}

/// Enforces clean minimum resize bounds and exact monitor Work Area when maximizing (respects AuraTopBar!)
pub unsafe fn handle_getminmaxinfo(
    hwnd: HWND,
    lparam: LPARAM,
    min_w: i32,
    min_h: i32,
) -> LRESULT {
    if lparam.0 != 0 {
        let mmi = &mut *(lparam.0 as *mut MINMAXINFO);
        mmi.ptMinTrackSize = POINT { x: min_w, y: min_h };
        let hmon = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
        let mut mi = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if GetMonitorInfoW(hmon, &mut mi).as_bool() {
            mmi.ptMaxPosition = POINT {
                x: mi.rcWork.left - mi.rcMonitor.left,
                y: mi.rcWork.top - mi.rcMonitor.top,
            };
            mmi.ptMaxSize = POINT {
                x: mi.rcWork.right - mi.rcWork.left,
                y: mi.rcWork.bottom - mi.rcWork.top,
            };
        }
    }
    LRESULT(0)
}

/// Authentic Windows 11 WM_NCHITTEST:
/// - 8px edges & 14px corners return true HT* resize codes -> native cursors & smooth resize!
/// - Empty titlebar returns HTCAPTION -> native Windows 11 Aero Snap (edges, corners, Snap Assist, dblclick max)!
pub unsafe fn handle_nchittest<F>(
    hwnd: HWND,
    lparam: LPARAM,
    titlebar_h: f32,
    over_interactive: F,
) -> LRESULT
where
    F: FnOnce(f32, f32) -> bool,
{
    let mut wr = RECT::default();
    if GetWindowRect(hwnd, &mut wr).is_err() {
        return LRESULT(HTCLIENT as isize);
    }
    let sx = (lparam.0 & 0xFFFF) as i16 as i32;
    let sy = ((lparam.0 >> 16) & 0xFFFF) as i16 as i32;
    let zoomed = IsZoomed(hwnd).as_bool();

    if !zoomed {
        let b = 8;
        let c = 14;
        let on_l = sx < wr.left + b;
        let on_r = sx >= wr.right - b;
        let on_t = sy < wr.top + b;
        let on_b = sy >= wr.bottom - b;

        let near_l = sx < wr.left + c;
        let near_r = sx >= wr.right - c;
        let near_t = sy < wr.top + c;
        let near_b = sy >= wr.bottom - c;

        if (on_t && near_l) || (on_l && near_t) {
            return LRESULT(HTTOPLEFT as isize);
        }
        if (on_t && near_r) || (on_r && near_t) {
            return LRESULT(HTTOPRIGHT as isize);
        }
        if (on_b && near_l) || (on_l && near_b) {
            return LRESULT(HTBOTTOMLEFT as isize);
        }
        if (on_b && near_r) || (on_r && near_b) {
            return LRESULT(HTBOTTOMRIGHT as isize);
        }
        if on_l {
            return LRESULT(HTLEFT as isize);
        }
        if on_r {
            return LRESULT(HTRIGHT as isize);
        }
        if on_t {
            return LRESULT(HTTOP as isize);
        }
        if on_b {
            return LRESULT(HTBOTTOM as isize);
        }
    }

    let local_x = (sx - wr.left) as f32;
    let local_y = (sy - wr.top) as f32;
    if local_y >= 0.0 && local_y < titlebar_h && !over_interactive(local_x, local_y) {
        return LRESULT(HTCAPTION as isize);
    }

    LRESULT(HTCLIENT as isize)
}

/// Snaps any AuraOS window instantaneously to halves, quarters, maximize, or centered default!
pub unsafe fn snap_window(hwnd: HWND, zone: SnapZone) {
    if zone == SnapZone::Maximize {
        let _ = ShowWindow(hwnd, SW_MAXIMIZE);
        return;
    }
    if IsZoomed(hwnd).as_bool() {
        let _ = ShowWindow(hwnd, SW_RESTORE);
    }
    let wa = monitor_work_rect(hwnd);
    let ww = wa.right - wa.left;
    let wh = wa.bottom - wa.top;
    let half_w = ww / 2;
    let half_h = wh / 2;

    let (x, y, w, h) = match zone {
        SnapZone::LeftHalf => (wa.left, wa.top, half_w, wh),
        SnapZone::RightHalf => (wa.left + half_w, wa.top, ww - half_w, wh),
        SnapZone::TopHalf => (wa.left, wa.top, ww, half_h),
        SnapZone::BottomHalf => (wa.left, wa.top + half_h, ww, wh - half_h),
        SnapZone::TopLeft => (wa.left, wa.top, half_w, half_h),
        SnapZone::TopRight => (wa.left + half_w, wa.top, ww - half_w, half_h),
        SnapZone::BottomLeft => (wa.left, wa.top + half_h, half_w, wh - half_h),
        SnapZone::BottomRight => (wa.left + half_w, wa.top + half_h, ww - half_w, wh - half_h),
        SnapZone::Center => {
            let cw = ((ww as f32 * 0.72) as i32).clamp(840, 1280);
            let ch = ((wh as f32 * 0.78) as i32).clamp(580, 820);
            (wa.left + (ww - cw) / 2, wa.top + (wh - ch) / 2, cw, ch)
        }
        SnapZone::Maximize => unreachable!(),
    };

    let _ = SetWindowPos(hwnd, HWND::default(), x, y, w, h, SWP_NOZORDER);
}

/// Right-click helper on the green '+' traffic light: smoothly cycles halves, 4 corners, and center!
pub unsafe fn cycle_snap_window(hwnd: HWND) {
    use std::cell::Cell;
    thread_local! {
        static LAST_SNAP: Cell<Option<SnapZone>> = const { Cell::new(None) };
    }
    let next = LAST_SNAP.with(|c| {
        let n = SnapZone::next_cycle(c.get());
        c.set(Some(n));
        n
    });
    snap_window(hwnd, next);
}
