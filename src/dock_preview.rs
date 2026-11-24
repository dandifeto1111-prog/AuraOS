//! AuraOS v4.5 Live DWM Window Preview Popup for AuraDock (60 FPS GPU-Composited, 0% CPU!)
//! Uses native Windows Desktop Window Manager `DwmRegisterThumbnail` to stream LIVE real-time
//! previews of any running window (videos, browsers, games, Aura apps) right next to the Dock!

use crate::win32_shell::to_wide;
use std::cell::RefCell;
use windows::core::PCWSTR;
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, RECT, SIZE, WPARAM};
use windows::Win32::Graphics::Dwm::{
    DwmQueryThumbnailSourceSize, DwmRegisterThumbnail, DwmSetWindowAttribute,
    DwmUnregisterThumbnail, DwmUpdateThumbnailProperties, DWMWCP_ROUND, DWMWA_BORDER_COLOR,
    DWMWA_WINDOW_CORNER_PREFERENCE, DWM_THUMBNAIL_PROPERTIES, DWM_TNP_OPACITY,
    DWM_TNP_RECTDESTINATION, DWM_TNP_SOURCECLIENTAREAONLY, DWM_TNP_VISIBLE,
};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, CreateFontW, CreateSolidBrush, DeleteObject, DrawTextW, EndPaint, FillRect,
    InvalidateRect, SelectObject, SetBkMode, SetTextColor, DT_END_ELLIPSIS, DT_LEFT,
    DT_SINGLELINE, DT_VCENTER, PAINTSTRUCT, TRANSPARENT,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, IsWindow, LoadCursorW, PostMessageW,
    RegisterClassW, SetWindowPos, ShowWindow, HWND_TOPMOST, IDC_HAND, SWP_NOACTIVATE, SW_HIDE,
    SW_SHOWNOACTIVATE, WM_LBUTTONDOWN, WM_MOUSEMOVE, WM_PAINT, WNDCLASSW, WS_EX_NOACTIVATE,
    WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
};

pub const PREVIEW_W: i32 = 276;
pub const PREVIEW_H: i32 = 196;
const HEADER_H: i32 = 34;

#[derive(Default)]
struct PreviewPaintState {
    title: String,
    target_hwnd: isize,
    hover_close: bool,
    owner_hwnd: isize,
    click_msg: u32,
}

thread_local! {
    static PREVIEW_STATE: RefCell<PreviewPaintState> = RefCell::new(PreviewPaintState::default());
}

fn close_btn_rect() -> RECT {
    RECT {
        left: PREVIEW_W - 28,
        top: 8,
        right: PREVIEW_W - 10,
        bottom: 26,
    }
}

fn pt_in_rect(r: RECT, x: i32, y: i32) -> bool {
    x >= r.left && x <= r.right && y >= r.top && y <= r.bottom
}

unsafe extern "system" fn preview_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_PAINT => {
            let mut ps = PAINTSTRUCT::default();
            let hdc = BeginPaint(hwnd, &mut ps);
            if !hdc.is_invalid() {
                let full = RECT { left: 0, top: 0, right: PREVIEW_W, bottom: PREVIEW_H };
                let bg = CreateSolidBrush(COLORREF(0x001E1717)); // #17171E matte obsidian
                FillRect(hdc, &full, bg);
                let _ = DeleteObject(bg);

                let well = RECT { left: 10, top: HEADER_H, right: PREVIEW_W - 10, bottom: PREVIEW_H - 10 };
                let well_br = CreateSolidBrush(COLORREF(0x00120E0E));
                FillRect(hdc, &well, well_br);
                let _ = DeleteObject(well_br);

                let (title, hov_close) = PREVIEW_STATE.with(|s| {
                    let g = s.borrow();
                    (g.title.clone(), g.hover_close)
                });

                // Header title text
                let font_face = to_wide("Segoe UI");
                let hfont = CreateFontW(
                    -13,
                    0,
                    0,
                    0,
                    600,
                    0,
                    0,
                    0,
                    1,
                    0,
                    0,
                    5,
                    0,
                    PCWSTR(font_face.as_ptr()),
                );
                let old_f = SelectObject(hdc, hfont);
                SetBkMode(hdc, TRANSPARENT);
                SetTextColor(hdc, COLORREF(0x00FAF5F5));
                let mut tr = RECT { left: 12, top: 4, right: PREVIEW_W - 34, bottom: HEADER_H - 2 };
                let mut wtitle = to_wide(&title);
                let wlen = wtitle.len().saturating_sub(1);
                let _ = DrawTextW(
                    hdc,
                    &mut wtitle[..wlen],
                    &mut tr,
                    DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS,
                );

                // Close '×' pill
                let cr = close_btn_rect();
                let cbr = CreateSolidBrush(COLORREF(if hov_close { 0x005252E8 } else { 0x0032282B }));
                FillRect(hdc, &cr, cbr);
                let _ = DeleteObject(cbr);
                SetTextColor(hdc, COLORREF(0x00FFFFFF));
                let mut x_txt = to_wide("×");
                let mut xr = cr;
                xr.left += 5;
                let _ = DrawTextW(hdc, &mut x_txt[..1], &mut xr, DT_LEFT | DT_VCENTER | DT_SINGLELINE);

                SelectObject(hdc, old_f);
                let _ = DeleteObject(hfont);
            }
            let _ = EndPaint(hwnd, &ps);
            LRESULT(0)
        }
        WM_MOUSEMOVE => {
            let x = (lparam.0 & 0xFFFF) as i16 as i32;
            let y = ((lparam.0 >> 16) & 0xFFFF) as i16 as i32;
            let hc = pt_in_rect(close_btn_rect(), x, y);
            let changed = PREVIEW_STATE.with(|s| {
                let mut g = s.borrow_mut();
                if g.hover_close != hc {
                    g.hover_close = hc;
                    true
                } else {
                    false
                }
            });
            if changed {
                let _ = InvalidateRect(hwnd, None, false);
            }
            LRESULT(0)
        }
        WM_LBUTTONDOWN => {
            let x = (lparam.0 & 0xFFFF) as i16 as i32;
            let y = ((lparam.0 >> 16) & 0xFFFF) as i16 as i32;
            let is_close = pt_in_rect(close_btn_rect(), x, y);
            let (owner, cmsg, target) = PREVIEW_STATE.with(|s| {
                let g = s.borrow();
                (g.owner_hwnd, g.click_msg, g.target_hwnd)
            });
            if owner != 0 && cmsg != 0 && target != 0 {
                // wparam = 1 (close) or 0 (focus/restore), lparam = target_hwnd
                let _ = PostMessageW(
                    HWND(owner as *mut _),
                    cmsg,
                    WPARAM(if is_close { 1 } else { 0 }),
                    LPARAM(target),
                );
            }
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

pub struct DockPreviewPopup {
    pub hwnd: HWND,
    thumb: isize,
    pub active_target: isize,
    pub visible: bool,
    pub bounds: RECT,
}

impl DockPreviewPopup {
    pub fn new(owner_hwnd: HWND, click_msg: u32) -> Self {
        PREVIEW_STATE.with(|s| {
            let mut g = s.borrow_mut();
            g.owner_hwnd = owner_hwnd.0 as isize;
            g.click_msg = click_msg;
        });

        let hwnd = unsafe {
            let hinst = GetModuleHandleW(None).unwrap_or_default();
            let cls = to_wide("AuraDockPreview_v45");
            let wc = WNDCLASSW {
                lpfnWndProc: Some(preview_wnd_proc),
                hInstance: hinst.into(),
                hCursor: LoadCursorW(None, IDC_HAND).unwrap_or_default(),
                lpszClassName: PCWSTR(cls.as_ptr()),
                ..Default::default()
            };
            RegisterClassW(&wc);
            let title = to_wide("AuraDockPreview");
            let hw = CreateWindowExW(
                WS_EX_TOOLWINDOW | WS_EX_TOPMOST | WS_EX_NOACTIVATE,
                PCWSTR(cls.as_ptr()),
                PCWSTR(title.as_ptr()),
                WS_POPUP,
                -32000,
                -32000,
                PREVIEW_W,
                PREVIEW_H,
                None,
                None,
                hinst,
                None,
            )
            .unwrap_or_default();

            if !hw.is_invalid() {
                let pref = DWMWCP_ROUND;
                let _ = DwmSetWindowAttribute(
                    hw,
                    DWMWA_WINDOW_CORNER_PREFERENCE,
                    &pref as *const _ as *const _,
                    std::mem::size_of_val(&pref) as u32,
                );
                let border: u32 = 0x0060424A; // subtle violet-slate border
                let _ = DwmSetWindowAttribute(
                    hw,
                    DWMWA_BORDER_COLOR,
                    &border as *const u32 as *const _,
                    std::mem::size_of::<u32>() as u32,
                );
            }
            hw
        };

        Self {
            hwnd,
            thumb: 0,
            active_target: 0,
            visible: false,
            bounds: RECT::default(),
        }
    }

    pub fn show_for_window(
        &mut self,
        target_hwnd: isize,
        title: &str,
        x: i32,
        y: i32,
        screen_w: i32,
        screen_h: i32,
    ) {
        if self.hwnd.is_invalid() || target_hwnd == 0 {
            return;
        }
        let target = HWND(target_hwnd as *mut _);
        unsafe {
            if !IsWindow(target).as_bool() {
                self.hide();
                return;
            }
        }

        let px = x.clamp(8, (screen_w - PREVIEW_W - 8).max(8));
        let py = y.clamp(40, (screen_h - PREVIEW_H - 8).max(40));
        self.bounds = RECT {
            left: px,
            top: py,
            right: px + PREVIEW_W,
            bottom: py + PREVIEW_H,
        };

        let target_changed = self.active_target != target_hwnd || self.thumb == 0;
        if target_changed {
            self.unregister_thumb();
            unsafe {
                if let Ok(th) = DwmRegisterThumbnail(self.hwnd, target) {
                    self.thumb = th;
                    let src_sz = DwmQueryThumbnailSourceSize(th).unwrap_or(SIZE { cx: 1280, cy: 720 });
                    let avail_w = (PREVIEW_W - 20) as f32;
                    let avail_h = (PREVIEW_H - HEADER_H - 10) as f32;
                    let sw = (src_sz.cx.max(1)) as f32;
                    let sh = (src_sz.cy.max(1)) as f32;
                    let scale = (avail_w / sw).min(avail_h / sh);
                    let dw = (sw * scale).round() as i32;
                    let dh = (sh * scale).round() as i32;
                    let ox = 10 + ((avail_w as i32 - dw) / 2).max(0);
                    let oy = HEADER_H + ((avail_h as i32 - dh) / 2).max(0);
                    let props = DWM_THUMBNAIL_PROPERTIES {
                        dwFlags: DWM_TNP_RECTDESTINATION
                            | DWM_TNP_VISIBLE
                            | DWM_TNP_OPACITY
                            | DWM_TNP_SOURCECLIENTAREAONLY,
                        rcDestination: RECT {
                            left: ox,
                            top: oy,
                            right: ox + dw.max(16),
                            bottom: oy + dh.max(16),
                        },
                        rcSource: RECT::default(),
                        opacity: 255,
                        fVisible: true.into(),
                        fSourceClientAreaOnly: false.into(),
                    };
                    let _ = DwmUpdateThumbnailProperties(th, &props);
                }
            }
            self.active_target = target_hwnd;
        }

        PREVIEW_STATE.with(|s| {
            let mut g = s.borrow_mut();
            g.title = title.to_string();
            g.target_hwnd = target_hwnd;
            g.hover_close = false;
        });

        unsafe {
            let _ = SetWindowPos(
                self.hwnd,
                HWND_TOPMOST,
                px,
                py,
                PREVIEW_W,
                PREVIEW_H,
                SWP_NOACTIVATE,
            );
            let _ = ShowWindow(self.hwnd, SW_SHOWNOACTIVATE);
            let _ = InvalidateRect(self.hwnd, None, false);
        }
        self.visible = true;
    }

    pub fn contains_point(&self, cx: i32, cy: i32) -> bool {
        self.visible
            && cx >= self.bounds.left - 8
            && cx <= self.bounds.right + 8
            && cy >= self.bounds.top - 8
            && cy <= self.bounds.bottom + 8
    }

    fn unregister_thumb(&mut self) {
        if self.thumb != 0 {
            unsafe {
                let _ = DwmUnregisterThumbnail(self.thumb);
            }
            self.thumb = 0;
        }
    }

    pub fn hide(&mut self) {
        if !self.visible && self.thumb == 0 {
            return;
        }
        self.unregister_thumb();
        self.active_target = 0;
        self.visible = false;
        unsafe {
            if !self.hwnd.is_invalid() {
                let _ = ShowWindow(self.hwnd, SW_HIDE);
            }
        }
    }

    pub fn shutdown(&mut self) {
        self.hide();
        unsafe {
            if !self.hwnd.is_invalid() {
                let _ = DestroyWindow(self.hwnd);
                self.hwnd = HWND::default();
            }
        }
    }
}
