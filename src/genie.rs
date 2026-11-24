//! AuraOS v4.5 macOS Genie Effect Engine (Direct2D 36-Strip Curved Bezier Suction / Un-Suction!)
//! - Captures a crisp real-time snapshot of any window (`PrintWindow(PW_RENDERFULLCONTENT)` + BitBlt fallback)
//! - Slices the window into 36 smooth GPU strips warped along a classic macOS cubic S-curve funnel
//!   flowing directly into (or blooming out of!) the exact pixel coordinate of that app's Dock icon!
//! - Zero idle cost: overlay HWND stays `SW_HIDE` whenever no genie transition is active!

use crate::renderer::{GpuEngine, WindowSurface};
use crate::win32_shell::to_wide;
use std::time::Instant;
use windows::core::PCWSTR;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Direct2D::Common::{
    D2D1_ALPHA_MODE_PREMULTIPLIED, D2D1_PIXEL_FORMAT, D2D_RECT_F, D2D_SIZE_U,
};
use windows::Win32::Graphics::Direct2D::{
    D2D1_BITMAP_INTERPOLATION_MODE_LINEAR, D2D1_BITMAP_PROPERTIES, ID2D1Bitmap,
};
use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM;
use windows::Win32::Graphics::Gdi::{
    BitBlt, CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, GetDC, ReleaseDC,
    SelectObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, HDC, SRCCOPY,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;

#[link(name = "user32")]
extern "system" {
    fn PrintWindow(hwnd: HWND, hdcblt: HDC, nflags: u32) -> i32;
}
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, GetWindowPlacement, GetWindowRect, IsIconic,
    IsWindow, RegisterClassW, SetForegroundWindow, SetWindowPos, ShowWindow, HWND_TOPMOST,
    SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SW_HIDE, SW_MINIMIZE, SW_RESTORE, SW_SHOWNOACTIVATE,
    WINDOWPLACEMENT, WNDCLASSW, WS_EX_NOACTIVATE, WS_EX_NOREDIRECTIONBITMAP, WS_EX_TOOLWINDOW,
    WS_EX_TOPMOST, WS_EX_TRANSPARENT, WS_POPUP,
};

const GENIE_SLICES: usize = 36;
const GENIE_DURATION_SEC: f32 = 0.30;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum GenieDir {
    MinimizeIn,
    RestoreOut,
}

struct ActiveGenie {
    target_hwnd: HWND,
    dir: GenieDir,
    win_rect: RECT,
    dock_x: f32,
    dock_y: f32,
    is_left_dock: bool,
    bmp_w: u32,
    bmp_h: u32,
    bgra: Vec<u8>,
    started: Instant,
    action_applied: bool,
}

unsafe extern "system" fn genie_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    DefWindowProcW(hwnd, msg, wparam, lparam)
}

fn capture_window_bgra(hwnd: HWND, rc: RECT) -> Option<(u32, u32, Vec<u8>)> {
    let w = (rc.right - rc.left).clamp(64, 3840) as u32;
    let h = (rc.bottom - rc.top).clamp(64, 2160) as u32;
    unsafe {
        let hdc_screen = GetDC(None);
        if hdc_screen.is_invalid() {
            return None;
        }
        let hdc_mem = CreateCompatibleDC(hdc_screen);
        let bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: w as i32,
                biHeight: -(h as i32), // top-down BGRA
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut bits: *mut std::ffi::c_void = std::ptr::null_mut();
        let hbmp = CreateDIBSection(hdc_mem, &bmi, DIB_RGB_COLORS, &mut bits, None, 0).ok();
        let mut out: Option<Vec<u8>> = None;

        if let Some(hbmp) = hbmp {
            if !hbmp.is_invalid() && !bits.is_null() {
                let old = SelectObject(hdc_mem, hbmp);
                // PW_RENDERFULLCONTENT = 2 captures DirectComposition/D2D/Chrome/Electron windows cleanly!
                let printed = PrintWindow(hwnd, hdc_mem, 2) != 0;
                if !printed {
                    let _ = BitBlt(
                        hdc_mem,
                        0,
                        0,
                        w as i32,
                        h as i32,
                        hdc_screen,
                        rc.left,
                        rc.top,
                        SRCCOPY,
                    );
                }
                let byte_len = (w * h * 4) as usize;
                let slice = std::slice::from_raw_parts(bits as *const u8, byte_len);
                let mut buf = slice.to_vec();
                // Ensure opaque alpha for premultiplied Direct2D rendering
                let mut non_zero = 0usize;
                for px in buf.chunks_exact_mut(4) {
                    if (px[0] | px[1] | px[2]) != 0 {
                        non_zero += 1;
                    }
                    px[3] = 255;
                }
                if non_zero > 64 {
                    out = Some(buf);
                } else {
                    // Fallback to direct screen BitBlt if PrintWindow yielded blank surface
                    let _ = BitBlt(
                        hdc_mem,
                        0,
                        0,
                        w as i32,
                        h as i32,
                        hdc_screen,
                        rc.left,
                        rc.top,
                        SRCCOPY,
                    );
                    let mut buf2 = std::slice::from_raw_parts(bits as *const u8, byte_len).to_vec();
                    for px in buf2.chunks_exact_mut(4) {
                        px[3] = 255;
                    }
                    out = Some(buf2);
                }
                SelectObject(hdc_mem, old);
                let _ = DeleteObject(hbmp);
            }
        }
        let _ = DeleteDC(hdc_mem);
        let _ = ReleaseDC(None, hdc_screen);
        out.map(|bgra| (w, h, bgra))
    }
}

fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0).max(0.0001)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

pub struct GenieAnimator {
    hwnd: HWND,
    surf: Option<WindowSurface>,
    screen_w: i32,
    screen_h: i32,
    active: Option<ActiveGenie>,
    cached_snapshots: Vec<(isize, RECT, u32, u32, Vec<u8>)>,
}

impl GenieAnimator {
    pub fn new(gpu: &GpuEngine, sw: i32, sh: i32) -> Self {
        let hwnd = unsafe {
            let hinst = GetModuleHandleW(None).unwrap_or_default();
            let cls = to_wide("AuraGenieOverlay_v45");
            let wc = WNDCLASSW {
                lpfnWndProc: Some(genie_wnd_proc),
                hInstance: hinst.into(),
                lpszClassName: PCWSTR(cls.as_ptr()),
                ..Default::default()
            };
            RegisterClassW(&wc);
            let title = to_wide("AuraGenieOverlay");
            CreateWindowExW(
                WS_EX_NOREDIRECTIONBITMAP
                    | WS_EX_TOOLWINDOW
                    | WS_EX_TOPMOST
                    | WS_EX_NOACTIVATE
                    | WS_EX_TRANSPARENT,
                PCWSTR(cls.as_ptr()),
                PCWSTR(title.as_ptr()),
                WS_POPUP,
                0,
                0,
                sw,
                sh,
                None,
                None,
                hinst,
                None,
            )
            .unwrap_or_default()
        };
        let surf = if !hwnd.is_invalid() {
            WindowSurface::new(gpu, hwnd, sw as u32, sh as u32).ok()
        } else {
            None
        };
        Self {
            hwnd,
            surf,
            screen_w: sw,
            screen_h: sh,
            active: None,
            cached_snapshots: Vec::new(),
        }
    }

    pub fn is_active(&self) -> bool {
        self.active.is_some()
    }

    /// Triggers macOS Genie Suction into the Dock icon, then minimizes the window cleanly!
    pub fn start_minimize(&mut self, target_hwnd: isize, dock_x: f32, dock_y: f32, is_left_dock: bool) {
        if self.hwnd.is_invalid() || self.surf.is_none() || target_hwnd == 0 {
            return;
        }
        let hw = HWND(target_hwnd as *mut _);
        unsafe {
            if !IsWindow(hw).as_bool() || IsIconic(hw).as_bool() {
                return;
            }
            let mut rc = RECT::default();
            if GetWindowRect(hw, &mut rc).is_err() {
                return;
            }
            if let Some((bw, bh, bgra)) = capture_window_bgra(hw, rc) {
                // Save snapshot for smooth Genie Un-Suction when the user restores this window!
                self.cached_snapshots.retain(|(h, ..)| *h != target_hwnd);
                if self.cached_snapshots.len() >= 8 {
                    self.cached_snapshots.remove(0);
                }
                self.cached_snapshots.push((target_hwnd, rc, bw, bh, bgra.clone()));

                // Hide/minimize real window right after grabbing its frame so ONLY our curved GPU Genie moves!
                let _ = ShowWindow(hw, SW_MINIMIZE);

                self.active = Some(ActiveGenie {
                    target_hwnd: hw,
                    dir: GenieDir::MinimizeIn,
                    win_rect: rc,
                    dock_x,
                    dock_y,
                    is_left_dock,
                    bmp_w: bw,
                    bmp_h: bh,
                    bgra,
                    started: Instant::now(),
                    action_applied: true,
                });
                let _ = SetWindowPos(
                    self.hwnd,
                    HWND_TOPMOST,
                    0,
                    0,
                    0,
                    0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
                );
                let _ = ShowWindow(self.hwnd, SW_SHOWNOACTIVATE);
            } else {
                let _ = ShowWindow(hw, SW_MINIMIZE);
            }
        }
    }

    /// Triggers reverse macOS Genie Un-Suction blooming out of the Dock icon, then restores & focuses the window!
    pub fn start_restore(&mut self, target_hwnd: isize, dock_x: f32, dock_y: f32, is_left_dock: bool) -> bool {
        if self.hwnd.is_invalid() || self.surf.is_none() || target_hwnd == 0 {
            return false;
        }
        let hw = HWND(target_hwnd as *mut _);
        unsafe {
            if !IsWindow(hw).as_bool() {
                return false;
            }
        }
        let cached = self
            .cached_snapshots
            .iter()
            .find(|(h, ..)| *h == target_hwnd)
            .cloned();

        let (rc, bw, bh, bgra) = if let Some((_, r, w, h, b)) = cached {
            (r, w, h, b)
        } else {
            // Synthesize sleek matte obsidian glass card matching the window's restored rect
            let mut wp = WINDOWPLACEMENT {
                length: std::mem::size_of::<WINDOWPLACEMENT>() as u32,
                ..Default::default()
            };
            let rc = unsafe {
                if GetWindowPlacement(hw, &mut wp).is_ok() {
                    wp.rcNormalPosition
                } else {
                    RECT {
                        left: self.screen_w / 6,
                        top: self.screen_h / 6,
                        right: (self.screen_w * 5) / 6,
                        bottom: (self.screen_h * 5) / 6,
                    }
                }
            };
            let bw = 240u32;
            let bh = 150u32;
            let mut buf = vec![0u8; (bw * bh * 4) as usize];
            for px in buf.chunks_exact_mut(4) {
                px[0] = 36;
                px[1] = 24;
                px[2] = 26;
                px[3] = 240;
            }
            (rc, bw, bh, buf)
        };

        self.active = Some(ActiveGenie {
            target_hwnd: hw,
            dir: GenieDir::RestoreOut,
            win_rect: rc,
            dock_x,
            dock_y,
            is_left_dock,
            bmp_w: bw,
            bmp_h: bh,
            bgra,
            started: Instant::now(),
            action_applied: false,
        });
        unsafe {
            let _ = SetWindowPos(
                self.hwnd,
                HWND_TOPMOST,
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
            );
            let _ = ShowWindow(self.hwnd, SW_SHOWNOACTIVATE);
        }
        true
    }

    pub fn tick(&mut self, gpu: &mut GpuEngine) -> bool {
        let Some(act) = &mut self.active else {
            return false;
        };
        let raw_p = (act.started.elapsed().as_secs_f32() / GENIE_DURATION_SEC).clamp(0.0, 1.0);
        let t = match act.dir {
            GenieDir::MinimizeIn => raw_p,
            GenieDir::RestoreOut => 1.0 - raw_p,
        };

        if let Some(surf) = &mut self.surf {
            let win_l = act.win_rect.left as f32;
            let win_t = act.win_rect.top as f32;
            let win_r = act.win_rect.right as f32;
            let win_b = act.win_rect.bottom as f32;
            let dock_x = act.dock_x;
            let dock_y = act.dock_y;
            let is_left = act.is_left_dock;
            let bw = act.bmp_w;
            let bh = act.bmp_h;
            let bgra = &act.bgra;

            surf.draw(gpu, |ctx| {
                let props = D2D1_BITMAP_PROPERTIES {
                    pixelFormat: D2D1_PIXEL_FORMAT {
                        format: DXGI_FORMAT_B8G8R8A8_UNORM,
                        alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
                    },
                    dpiX: 96.0,
                    dpiY: 96.0,
                };
                let bmp: Option<ID2D1Bitmap> = unsafe {
                    ctx.rt
                        .CreateBitmap(
                            D2D_SIZE_U { width: bw, height: bh },
                            Some(bgra.as_ptr() as *const _),
                            bw * 4,
                            &props,
                        )
                        .ok()
                };
                let Some(bmp) = bmp else { return };

                // Phase 1 (0.0 -> 0.45): funnel curves pinch smoothly toward the Dock icon
                // Phase 2 (0.20 -> 1.0): window slides smoothly into the curved funnel nozzle!
                let pinch = smoothstep(0.0, 0.55, t);
                let slide = smoothstep(0.18, 1.0, t);
                let alpha = (1.0 - smoothstep(0.84, 1.0, t)).max(0.05);
                let icon_half = 20.0f32;

                if !is_left {
                    // Bottom Dock: horizontal slices curving downward into (dock_x, dock_y)
                    let top_now = win_t + (dock_y - win_t) * slide;
                    let bot_now = win_b + (dock_y - win_b) * (slide * 0.65 + pinch * 0.35);
                    let total_span = (bot_now - top_now).max(4.0);
                    let slice_src_h = bh as f32 / GENIE_SLICES as f32;
                    let slice_dst_h = total_span / GENIE_SLICES as f32;

                    for i in 0..GENIE_SLICES {
                        let u0 = i as f32 / GENIE_SLICES as f32;
                        let u1 = (i + 1) as f32 / GENIE_SLICES as f32;
                        let y_dst0 = top_now + u0 * total_span;
                        let y_dst1 = top_now + u1 * total_span + 0.8;
                        // Normalized vertical proximity to Dock (0 at original top, 1 at Dock y)
                        let v = ((y_dst0 - win_t) / (dock_y - win_t).max(40.0)).clamp(0.0, 1.0);
                        // Classic macOS Genie S-curve funnel profile!
                        let s_curve = smoothstep(0.0, 1.0, v) * pinch;
                        let left_edge = win_l + ((dock_x - icon_half) - win_l) * s_curve;
                        let right_edge = win_r + ((dock_x + icon_half) - win_r) * s_curve;
                        if right_edge <= left_edge + 1.0 {
                            continue;
                        }
                        let src_r = D2D_RECT_F {
                            left: 0.0,
                            top: i as f32 * slice_src_h,
                            right: bw as f32,
                            bottom: (i + 1) as f32 * slice_src_h,
                        };
                        let dst_r = D2D_RECT_F {
                            left: left_edge,
                            top: y_dst0,
                            right: right_edge,
                            bottom: y_dst1.max(y_dst0 + slice_dst_h),
                        };
                        unsafe {
                            ctx.rt.DrawBitmap(
                                &bmp,
                                Some(&dst_r),
                                alpha,
                                D2D1_BITMAP_INTERPOLATION_MODE_LINEAR,
                                Some(&src_r),
                            );
                        }
                    }
                } else {
                    // Left Dock: vertical slices curving leftward into (dock_x, dock_y)!
                    let left_now = win_l + (dock_x - win_l) * (slide * 0.65 + pinch * 0.35);
                    let right_now = win_r + (dock_x - win_r) * slide;
                    let total_span = (right_now - left_now).max(4.0);
                    let slice_src_w = bw as f32 / GENIE_SLICES as f32;
                    let slice_dst_w = total_span / GENIE_SLICES as f32;

                    for i in 0..GENIE_SLICES {
                        let u0 = i as f32 / GENIE_SLICES as f32;
                        let u1 = (i + 1) as f32 / GENIE_SLICES as f32;
                        let x_dst0 = left_now + u0 * total_span;
                        let x_dst1 = left_now + u1 * total_span + 0.8;
                        // Proximity to Left Dock (1 at dock_x, 0 at win_r)
                        let v = ((win_r - x_dst0) / (win_r - dock_x).max(40.0)).clamp(0.0, 1.0);
                        let s_curve = smoothstep(0.0, 1.0, v) * pinch;
                        let top_edge = win_t + ((dock_y - icon_half) - win_t) * s_curve;
                        let bot_edge = win_b + ((dock_y + icon_half) - win_b) * s_curve;
                        if bot_edge <= top_edge + 1.0 {
                            continue;
                        }
                        let src_r = D2D_RECT_F {
                            left: i as f32 * slice_src_w,
                            top: 0.0,
                            right: (i + 1) as f32 * slice_src_w,
                            bottom: bh as f32,
                        };
                        let dst_r = D2D_RECT_F {
                            left: x_dst0,
                            top: top_edge,
                            right: x_dst1.max(x_dst0 + slice_dst_w),
                            bottom: bot_edge,
                        };
                        unsafe {
                            ctx.rt.DrawBitmap(
                                &bmp,
                                Some(&dst_r),
                                alpha,
                                D2D1_BITMAP_INTERPOLATION_MODE_LINEAR,
                                Some(&src_r),
                            );
                        }
                    }
                }
            });
        }

        if raw_p >= 1.0 {
            let finished = self.active.take().unwrap();
            if finished.dir == GenieDir::RestoreOut && !finished.action_applied {
                unsafe {
                    if IsWindow(finished.target_hwnd).as_bool() {
                        let _ = ShowWindow(finished.target_hwnd, SW_RESTORE);
                        let _ = SetForegroundWindow(finished.target_hwnd);
                    }
                }
            }
            unsafe {
                let _ = ShowWindow(self.hwnd, SW_HIDE);
            }
            return false;
        }
        true
    }

    pub fn shutdown(&mut self) {
        self.active = None;
        unsafe {
            if !self.hwnd.is_invalid() {
                let _ = ShowWindow(self.hwnd, SW_HIDE);
                let _ = DestroyWindow(self.hwnd);
                self.hwnd = HWND::default();
            }
        }
    }
}
