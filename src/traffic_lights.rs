//! AuraOS v5.1 Unified macOS Traffic-Light Overlay for Standard Third-Party Windows!
//! - 100% Pure Win32 + Direct2D, ZERO DLL injection, 100% Riot Vanguard-safe & 100% reversible!
//! - Queries the exact `DWMWA_CAPTION_BUTTON_BOUNDS` rect of the active standard Windows app
//!   (Edge, Explorer, Terminal, Notepad, etc.) and covers its traditional `— □ ×` box with a
//!   seamlessly color-matched Material You capsule containing interactive macOS semáforos:
//!   • Verde  -> Maximizar / Restaurar (Clic derecho = ciclo Aero Snap!)
//!   • Amarillo -> Minimizar con Animación Genie directa al Dock!
//!   • Rojo   -> Cerrar ventana (`WM_CLOSE`)!
//! - Automatically hides on AuraOS own apps (which already draw native semáforos), fullscreen apps,
//!   minimized windows, frameless/custom-chrome popups, or when no standard window is focused!

use crate::renderer::{GpuEngine, Rect, TextStyle, WindowSurface};
use crate::theme;
use crate::win32_shell::{self, to_wide};
use crate::window_styler;
use std::cell::RefCell;
use windows::core::PCWSTR;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_CAPTION_BUTTON_BOUNDS};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Controls::WM_MOUSELEAVE;
use windows::Win32::UI::Input::KeyboardAndMouse::{TrackMouseEvent, TME_LEAVE, TRACKMOUSEEVENT};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, GetForegroundWindow, GetWindowLongW,
    GetWindowRect, GetWindowThreadProcessId, IsIconic, IsWindow, IsWindowVisible, IsZoomed,
    LoadCursorW, PostMessageW, RegisterClassW, SetWindowPos, ShowWindow, GWL_EXSTYLE, GWL_STYLE,
    HWND_TOPMOST, IDC_HAND, MA_NOACTIVATE, SWP_NOACTIVATE, SW_HIDE, SW_SHOWNOACTIVATE,
    WM_LBUTTONDOWN, WM_MOUSEACTIVATE, WM_MOUSEMOVE, WM_RBUTTONDOWN, WNDCLASSW, WS_CAPTION,
    WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_NOREDIRECTIONBITMAP, WS_EX_TOOLWINDOW, WS_EX_TOPMOST,
    WS_EX_TRANSPARENT, WS_POPUP, WS_THICKFRAME,
};

struct TlShared {
    parent_hwnd: HWND,
    wm_genie_min: u32,
    target_hwnd: isize,
    hovered_idx: i32, // -1 none, 0 green(max), 1 yellow(min), 2 red(close)
    width: i32,
    height: i32,
    dirty: bool,
}

thread_local! {
    static TL_SHARED: RefCell<Option<TlShared>> = const { RefCell::new(None) };
}

fn hit_button(px: f32, w: f32) -> i32 {
    // Three circles neatly spaced inside the capsule on the right side
    let cx_red = w - 22.0;
    let cx_yel = w - 46.0;
    let cx_grn = w - 70.0;
    if (px - cx_red).abs() <= 11.0 {
        2
    } else if (px - cx_yel).abs() <= 11.0 {
        1
    } else if (px - cx_grn).abs() <= 11.0 {
        0
    } else {
        -1
    }
}

unsafe extern "system" fn tl_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_MOUSEACTIVATE => return LRESULT(MA_NOACTIVATE as isize),
        WM_MOUSEMOVE => {
            let px = (lparam.0 & 0xFFFF) as i16 as f32;
            let mut tme = TRACKMOUSEEVENT {
                cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                dwFlags: TME_LEAVE,
                hwndTrack: hwnd,
                dwHoverTime: 0,
            };
            let _ = TrackMouseEvent(&mut tme);
            TL_SHARED.with(|cell| {
                if let Ok(mut g) = cell.try_borrow_mut() {
                    if let Some(sh) = g.as_mut() {
                        let idx = hit_button(px, sh.width as f32);
                        let new_h = if idx >= 0 { idx } else { 99 }; // 99 = hovering capsule shows glyphs!
                        if sh.hovered_idx != new_h {
                            sh.hovered_idx = new_h;
                            sh.dirty = true;
                        }
                    }
                }
            });
            return LRESULT(0);
        }
        WM_MOUSELEAVE => {
            TL_SHARED.with(|cell| {
                if let Ok(mut g) = cell.try_borrow_mut() {
                    if let Some(sh) = g.as_mut() {
                        if sh.hovered_idx != -1 {
                            sh.hovered_idx = -1;
                            sh.dirty = true;
                        }
                    }
                }
            });
            return LRESULT(0);
        }
        WM_LBUTTONDOWN => {
            let px = (lparam.0 & 0xFFFF) as i16 as f32;
            let act = TL_SHARED.with(|cell| {
                cell.borrow().as_ref().map(|sh| {
                    (hit_button(px, sh.width as f32), sh.target_hwnd, sh.parent_hwnd, sh.wm_genie_min)
                })
            });
            if let Some((idx, target, parent, wm_genie)) = act {
                if target != 0 {
                    match idx {
                        2 => window_styler::close_active_window(target),
                        1 => {
                            if !parent.is_invalid() && wm_genie != 0 {
                                let _ = PostMessageW(parent, wm_genie, WPARAM(target as usize), LPARAM(0));
                            } else {
                                window_styler::minimize_active_window(target);
                            }
                        }
                        0 => window_styler::toggle_maximize_active_window(target),
                        _ => {}
                    }
                }
            }
            return LRESULT(0);
        }
        WM_RBUTTONDOWN => {
            let px = (lparam.0 & 0xFFFF) as i16 as f32;
            let act = TL_SHARED.with(|cell| {
                cell.borrow()
                    .as_ref()
                    .map(|sh| (hit_button(px, sh.width as f32), sh.target_hwnd))
            });
            if let Some((0, target)) = act {
                if target != 0 {
                    crate::aura_window::cycle_snap_window(HWND(target as *mut _));
                }
            }
            return LRESULT(0);
        }
        _ => {}
    }
    DefWindowProcW(hwnd, msg, wparam, lparam)
}

fn is_aura_or_excluded_proc(hwnd: HWND) -> bool {
    let mut pid = 0u32;
    unsafe {
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
    }
    if pid == 0 || pid == std::process::id() {
        return true;
    }
    let (pn, _) = win32_shell::get_process_info(pid);
    let low = pn.to_lowercase();
    low.starts_with("aura")
        || matches!(
            low.as_str(),
            "wallpaper64"
                | "wallpaper32"
                | "searchhost"
                | "startmenuexperiencehost"
                | "shellexperiencehost"
                | "lockapp"
                | "taskmgr"
                | "steamwebhelper"
        )
}

/// Computes exact screen rect covering the standard Windows `— □ ×` caption buttons via DWM!
fn compute_caption_buttons_rect(hwnd: HWND) -> Option<RECT> {
    if hwnd.is_invalid() || is_aura_or_excluded_proc(hwnd) {
        return None;
    }
    unsafe {
        if !IsWindow(hwnd).as_bool()
            || !IsWindowVisible(hwnd).as_bool()
            || IsIconic(hwnd).as_bool()
        {
            return None;
        }
        let style = GetWindowLongW(hwnd, GWL_STYLE) as u32;
        let ex_style = GetWindowLongW(hwnd, GWL_EXSTYLE) as u32;
        if (style & WS_CAPTION.0) != WS_CAPTION.0 || (style & WS_THICKFRAME.0) == 0 {
            return None;
        }
        let bad_ex = WS_EX_LAYERED.0
            | WS_EX_NOREDIRECTIONBITMAP.0
            | WS_EX_TOOLWINDOW.0
            | WS_EX_TRANSPARENT.0;
        if (ex_style & bad_ex) != 0 {
            return None;
        }

        let mut wr = RECT::default();
        if GetWindowRect(hwnd, &mut wr).is_err() {
            return None;
        }
        let win_w = wr.right - wr.left;
        let win_h = wr.bottom - wr.top;
        if win_w < 300 || win_h < 180 {
            return None;
        }

        // Exact DWM caption button bounds (relative to window top-left)!
        let mut btn_rc = RECT::default();
        if DwmGetWindowAttribute(
            hwnd,
            DWMWA_CAPTION_BUTTON_BOUNDS,
            &mut btn_rc as *mut RECT as *mut _,
            std::mem::size_of::<RECT>() as u32,
        )
        .is_ok()
        {
            let bw = btn_rc.right - btn_rc.left;
            let bh = btn_rc.bottom - btn_rc.top;
            if (84..=240).contains(&bw) && (18..=48).contains(&bh) {
                let zoomed = IsZoomed(hwnd).as_bool();
                let top_adj = if zoomed { 6 } else { 1 };
                let right_adj = if zoomed { 8 } else { 6 };
                let left = (wr.left + btn_rc.left - 2).max(wr.left + 80);
                let right = (wr.left + btn_rc.right + 2).min(wr.right - right_adj);
                let top = (wr.top + btn_rc.top + top_adj).max(34); // never overlap TopBar!
                let bottom = top + (bh.clamp(24, 34));
                if right - left >= 84 {
                    return Some(RECT { left, top, right, bottom });
                }
            }
        }
    }
    None
}

pub struct TrafficLightOverlay {
    hwnd: HWND,
    surf: Option<WindowSurface>,
    visible: bool,
    last_rect: RECT,
    last_target: isize,
}

impl TrafficLightOverlay {
    pub fn new(gpu: &GpuEngine, parent_hwnd: HWND, wm_genie_min: u32) -> Self {
        TL_SHARED.with(|cell| {
            *cell.borrow_mut() = Some(TlShared {
                parent_hwnd,
                wm_genie_min,
                target_hwnd: 0,
                hovered_idx: -1,
                width: 138,
                height: 30,
                dirty: false,
            });
        });

        let hwnd = unsafe {
            let hinst = GetModuleHandleW(None).unwrap_or_default();
            let cls = to_wide("AuraTrafficLightsOverlay_v51");
            let wc = WNDCLASSW {
                lpfnWndProc: Some(tl_wnd_proc),
                hInstance: hinst.into(),
                hCursor: LoadCursorW(None, IDC_HAND).unwrap_or_default(),
                lpszClassName: PCWSTR(cls.as_ptr()),
                ..Default::default()
            };
            RegisterClassW(&wc);
            let title = to_wide("AuraTrafficLightsOverlay");
            CreateWindowExW(
                WS_EX_NOREDIRECTIONBITMAP | WS_EX_TOOLWINDOW | WS_EX_TOPMOST | WS_EX_NOACTIVATE,
                PCWSTR(cls.as_ptr()),
                PCWSTR(title.as_ptr()),
                WS_POPUP,
                0,
                0,
                138,
                30,
                None,
                None,
                hinst,
                None,
            )
            .unwrap_or_default()
        };
        let surf = if !hwnd.is_invalid() {
            WindowSurface::new(gpu, hwnd, 2, 2).ok()
        } else {
            None
        };
        Self {
            hwnd,
            surf,
            visible: false,
            last_rect: RECT::default(),
            last_target: 0,
        }
    }

    pub fn hide(&mut self, gpu: &GpuEngine) {
        if self.visible {
            unsafe {
                let _ = ShowWindow(self.hwnd, SW_HIDE);
            }
            self.visible = false;
            self.last_target = 0;
            if let Some(s) = &mut self.surf {
                s.trim_hidden(gpu);
            }
            TL_SHARED.with(|cell| {
                if let Ok(mut g) = cell.try_borrow_mut() {
                    if let Some(sh) = g.as_mut() {
                        sh.target_hwnd = 0;
                        sh.hovered_idx = -1;
                        sh.dirty = false;
                    }
                }
            });
        }
    }

    pub fn sync(&mut self, gpu: &mut GpuEngine, enabled: bool, suppressed: bool) {
        if !enabled || suppressed || self.hwnd.is_invalid() {
            self.hide(gpu);
            return;
        }
        let fg = unsafe { GetForegroundWindow() };
        if fg == self.hwnd {
            return; // keep visible while clicking!
        }
        let Some(rc) = compute_caption_buttons_rect(fg) else {
            self.hide(gpu);
            return;
        };

        let w = (rc.right - rc.left).clamp(88, 220);
        let h = (rc.bottom - rc.top).clamp(24, 38);
        let target_raw = fg.0 as isize;

        let mut need_draw = !self.visible
            || target_raw != self.last_target
            || rc.left != self.last_rect.left
            || rc.top != self.last_rect.top
            || rc.right != self.last_rect.right
            || rc.bottom != self.last_rect.bottom;

        let hovered_idx = TL_SHARED.with(|cell| {
            let mut g = cell.borrow_mut();
            if let Some(sh) = g.as_mut() {
                sh.target_hwnd = target_raw;
                sh.width = w;
                sh.height = h;
                if sh.dirty {
                    sh.dirty = false;
                    need_draw = true;
                }
                sh.hovered_idx
            } else {
                -1
            }
        });

        if !need_draw {
            return;
        }

        self.last_target = target_raw;
        self.last_rect = rc;
        if let Some(surf) = &mut self.surf {
            surf.ensure_size(gpu, w as u32, h as u32);
            let wf = w as f32;
            let hf = h as f32;
            let show_glyphs = hovered_idx >= 0;
            let p = theme::pal();

            surf.draw(gpu, |ctx| {
                // Opaque Material You surface seamlessly matching styled DWM caption color
                ctx.fill_rrect(Rect::new(0.0, 0.0, wf, hf), 6.0, p.surface);

                // Sleek floating pill around the 3 macOS semáforos
                let pill_w = 78.0f32.min(wf - 6.0);
                let pill_x = wf - pill_w - 6.0;
                let pill_r = Rect::new(pill_x, (hf - 22.0) * 0.5, pill_w, 22.0);
                if show_glyphs {
                    ctx.fill_card(pill_r, 11.0, p.elevated, p.border_strong);
                } else {
                    ctx.fill_rrect(pill_r, 11.0, p.pill_subtle);
                }

                let cy = hf * 0.5;
                let btns = [
                    (0, wf - 70.0, theme::EMERALD, "+"),
                    (1, wf - 46.0, theme::AMBER, "−"),
                    (2, wf - 22.0, theme::CORAL, "×"),
                ];
                for (idx, cx, col, glyph) in btns {
                    let r = if hovered_idx == idx { 6.6 } else { 5.8 };
                    ctx.fill_circle(cx, cy, r, col);
                    if show_glyphs {
                        ctx.draw_text_center(
                            glyph,
                            Rect::new(cx - 6.0, cy - 6.5, 12.0, 12.0),
                            TextStyle::Small9Reg,
                            theme::DARK_INK,
                        );
                    }
                }
            });
        }

        unsafe {
            let _ = SetWindowPos(self.hwnd, HWND_TOPMOST, rc.left, rc.top, w, h, SWP_NOACTIVATE);
            if !self.visible {
                let _ = ShowWindow(self.hwnd, SW_SHOWNOACTIVATE);
                self.visible = true;
            }
        }
    }

    pub fn shutdown(&mut self) {
        unsafe {
            if !self.hwnd.is_invalid() {
                let _ = ShowWindow(self.hwnd, SW_HIDE);
                let _ = DestroyWindow(self.hwnd);
                self.hwnd = HWND::default();
            }
        }
        self.visible = false;
    }
}
