//! AuraShot v5.0 — `<25 ms` Native Rust Screen Snipping & Annotation Studio
//! - Replaces slow Windows Snipping Tool (`PrintScreen` / `Win+Shift+S` / Control Center)
//! - Instant native-DPI `BitBlt` desktop freeze before showing overlay
//! - Region crop + Fullscreen mode + Live Markup (Arrow, Rectangle, Highlighter, Pen, Mosaic Pixelate)
//! - 1-click Copy (`CF_DIB` Win32 Clipboard) & Save PNG to `Pictures\AuraShots\`!

#![windows_subsystem = "windows"]

use aura_os::agent_bridge;
use aura_os::aura_window;
use aura_os::renderer::{DrawCtx, GpuEngine, Rect, TextStyle, WindowSurface};
use aura_os::theme;
use aura_os::win32_shell;
use std::cell::RefCell;
use std::path::{Path, PathBuf};
use windows::core::PCWSTR;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject, GetDC, GetDIBits,
    ReleaseDC, SelectObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, SRCCOPY,
};
use windows::Win32::Graphics::Imaging::{
    CLSID_WICImagingFactory, GUID_ContainerFormatPng, GUID_WICPixelFormat32bppBGRA,
    IWICImagingFactory, WICBitmapEncoderNoCache,
};
use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    ReleaseCapture, SetCapture, VK_ESCAPE, VK_RETURN,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, GetMessageW, LoadCursorW, PostQuitMessage,
    RegisterClassW, ShowWindow, TranslateMessage, HWND_TOPMOST, IDC_CROSS, MSG, SW_HIDE, SW_SHOW,
    SetWindowPos, SWP_SHOWWINDOW, WM_CLOSE, WM_DESTROY, WM_KEYDOWN, WM_LBUTTONDOWN, WM_LBUTTONUP,
    WM_MOUSEMOVE, WM_RBUTTONDOWN, WNDCLASSW, WS_EX_NOREDIRECTIONBITMAP, WS_EX_TOOLWINDOW,
    WS_EX_TOPMOST, WS_POPUP,
};

#[derive(Clone, Copy, PartialEq, Eq)]
enum ShotTool {
    RegionSelect,
    Arrow,
    RectBox,
    Highlighter,
    Pen,
    Pixelate,
}

#[derive(Clone)]
enum ShotMark {
    Arrow { x0: f32, y0: f32, x1: f32, y1: f32, color: [u8; 3] },
    Rect { x0: f32, y0: f32, x1: f32, y1: f32, color: [u8; 3] },
    Pixelate { x0: f32, y0: f32, x1: f32, y1: f32 },
    Stroke { pts: Vec<(f32, f32)>, color: [u8; 3], highlight: bool },
}

#[derive(Clone)]
enum ShotAct {
    SetTool(ShotTool),
    FullScreenSel,
    Undo,
    OcrCopyExit,
    CopyExit,
    SavePngExit,
    CancelExit,
}

#[derive(Clone)]
struct Hit {
    id: String,
    label: String,
    rect: Rect,
    act: ShotAct,
}

struct ShotRuntime {
    hwnd: HWND,
    gpu: GpuEngine,
    surf: WindowSurface,
    sw: u32,
    sh: u32,
    desktop_bgra: Vec<u8>,

    selection: Option<Rect>,
    selecting_region: bool,
    sel_start: (f32, f32),

    tool: ShotTool,
    marks: Vec<ShotMark>,
    drag_mark_start: Option<(f32, f32)>,
    drag_mark_cur: Option<(f32, f32)>,
    active_stroke: Vec<(f32, f32)>,

    hovered_id: Option<String>,
    hits: Vec<Hit>,
}

thread_local! {
    static SHOT_RT: RefCell<Option<ShotRuntime>> = const { RefCell::new(None) };
}

fn capture_screen_bgra() -> (u32, u32, Vec<u8>) {
    let (sw_i, sh_i) = win32_shell::screen_size();
    let sw = sw_i.max(320) as u32;
    let sh = sh_i.max(240) as u32;
    let mut bgra = vec![0u8; (sw * sh * 4) as usize];

    unsafe {
        let hdc_scr = GetDC(HWND::default());
        let hdc_mem = CreateCompatibleDC(hdc_scr);
        let hbmp = CreateCompatibleBitmap(hdc_scr, sw as i32, sh as i32);
        let old = SelectObject(hdc_mem, hbmp);
        let _ = BitBlt(hdc_mem, 0, 0, sw as i32, sh as i32, hdc_scr, 0, 0, SRCCOPY);

        let mut bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: sw as i32,
                biHeight: -(sh as i32), // top-down BGRA
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let _ = GetDIBits(
            hdc_mem,
            hbmp,
            0,
            sh,
            Some(bgra.as_mut_ptr() as *mut _),
            &mut bmi,
            DIB_RGB_COLORS,
        );

        SelectObject(hdc_mem, old);
        let _ = DeleteObject(hbmp);
        let _ = DeleteDC(hdc_mem);
        ReleaseDC(HWND::default(), hdc_scr);
    }

    for px in bgra.chunks_exact_mut(4) {
        px[3] = 255;
    }
    (sw, sh, bgra)
}

/// Native Windows 11 WinRT OCR (`Windows.Media.Ocr.OcrEngine`) — zero external dependencies!
fn run_winrt_ocr_bgra(width: u32, height: u32, bgra: &[u8]) -> Option<String> {
    use windows::Graphics::Imaging::{BitmapPixelFormat, SoftwareBitmap};
    use windows::Media::Ocr::OcrEngine;
    use windows::Storage::Streams::DataWriter;

    if width < 8 || height < 8 || bgra.len() < (width * height * 4) as usize {
        return None;
    }
    let writer = DataWriter::new().ok()?;
    writer.WriteBytes(&bgra[..(width * height * 4) as usize]).ok()?;
    let ibuf = writer.DetachBuffer().ok()?;
    let soft_bmp = SoftwareBitmap::CreateCopyFromBuffer(
        &ibuf,
        BitmapPixelFormat::Bgra8,
        width as i32,
        height as i32,
    )
    .ok()?;
    let engine = OcrEngine::TryCreateFromUserProfileLanguages().ok()?;
    let result = engine.RecognizeAsync(&soft_bmp).ok()?.get().ok()?;
    let txt = result.Text().ok()?.to_string();
    Some(txt)
}

fn save_bgra_png(path: &Path, width: u32, height: u32, bgra: &[u8]) -> bool {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let Ok(wic): Result<IWICImagingFactory, _> =
            CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER)
        else {
            return false;
        };
        if let Some(p) = path.parent() {
            let _ = std::fs::create_dir_all(p);
        }
        let Ok(bmp) = wic.CreateBitmapFromMemory(width, height, &GUID_WICPixelFormat32bppBGRA, width * 4, bgra) else {
            return false;
        };
        let Ok(stream) = wic.CreateStream() else { return false };
        let wp = win32_shell::to_wide(&path.to_string_lossy());
        if stream
            .InitializeFromFilename(PCWSTR(wp.as_ptr()), windows::Win32::Foundation::GENERIC_WRITE.0)
            .is_err()
        {
            return false;
        }
        let Ok(enc) = wic.CreateEncoder(&GUID_ContainerFormatPng, std::ptr::null()) else { return false };
        if enc.Initialize(&stream, WICBitmapEncoderNoCache).is_err() {
            return false;
        }
        let mut frame = None;
        let mut bag = None;
        if enc.CreateNewFrame(&mut frame, &mut bag).is_err() {
            return false;
        }
        let Some(frame) = frame else { return false };
        let _ = frame.Initialize(bag.as_ref());
        let _ = frame.SetSize(width, height);
        let mut pf = GUID_WICPixelFormat32bppBGRA;
        let _ = frame.SetPixelFormat(&mut pf);
        if frame.WriteSource(&bmp, std::ptr::null()).is_err() {
            return false;
        }
        frame.Commit().is_ok() && enc.Commit().is_ok()
    }
}

fn bake_cropped_region(
    sw: u32,
    sh: u32,
    src_bgra: &[u8],
    sel: Rect,
    marks: &[ShotMark],
) -> (u32, u32, Vec<u8>) {
    let mut full = src_bgra.to_vec();
    let fw = sw as i32;
    let fh = sh as i32;

    let put = |buf: &mut [u8], x: i32, y: i32, c: [u8; 3], alpha: f32| {
        if x < 0 || y < 0 || x >= fw || y >= fh {
            return;
        }
        let idx = ((y * fw + x) * 4) as usize;
        let inv = 1.0 - alpha;
        buf[idx] = (c[2] as f32 * alpha + buf[idx] as f32 * inv).clamp(0.0, 255.0) as u8;
        buf[idx + 1] = (c[1] as f32 * alpha + buf[idx + 1] as f32 * inv).clamp(0.0, 255.0) as u8;
        buf[idx + 2] = (c[0] as f32 * alpha + buf[idx + 2] as f32 * inv).clamp(0.0, 255.0) as u8;
        buf[idx + 3] = 255;
    };

    let draw_seg = |buf: &mut [u8], x0: f32, y0: f32, x1: f32, y1: f32, c: [u8; 3], rad: i32, alpha: f32| {
        let steps = ((x1 - x0).abs().max((y1 - y0).abs()) as i32).max(1);
        for s in 0..=steps {
            let cx = (x0 + (x1 - x0) * (s as f32 / steps as f32)).round() as i32;
            let cy = (y0 + (y1 - y0) * (s as f32 / steps as f32)).round() as i32;
            for oy in -rad..=rad {
                for ox in -rad..=rad {
                    if ox * ox + oy * oy <= rad * rad {
                        put(buf, cx + ox, cy + oy, c, alpha);
                    }
                }
            }
        }
    };

    for m in marks {
        match m {
            ShotMark::Pixelate { x0, y0, x1, y1 } => {
                let lx = (x0.min(*x1) as i32).clamp(0, fw - 1);
                let rx = (x0.max(*x1) as i32).clamp(0, fw);
                let ty = (y0.min(*y1) as i32).clamp(0, fh - 1);
                let by = (y0.max(*y1) as i32).clamp(0, fh);
                let blk = 12i32;
                let mut by_y = ty;
                while by_y < by {
                    let mut bx_x = lx;
                    while bx_x < rx {
                        let sx = (bx_x + blk / 2).min(fw - 1);
                        let sy = (by_y + blk / 2).min(fh - 1);
                        let si = ((sy * fw + sx) * 4) as usize;
                        let sample = [full[si], full[si + 1], full[si + 2], 255];
                        for yy in by_y..(by_y + blk).min(by) {
                            for xx in bx_x..(bx_x + blk).min(rx) {
                                let di = ((yy * fw + xx) * 4) as usize;
                                full[di..di + 4].copy_from_slice(&sample);
                            }
                        }
                        bx_x += blk;
                    }
                    by_y += blk;
                }
            }
            ShotMark::Rect { x0, y0, x1, y1, color } => {
                let lx = x0.min(*x1);
                let rx = x0.max(*x1);
                let ty = y0.min(*y1);
                let by = y0.max(*y1);
                draw_seg(&mut full, lx, ty, rx, ty, *color, 2, 1.0);
                draw_seg(&mut full, lx, by, rx, by, *color, 2, 1.0);
                draw_seg(&mut full, lx, ty, lx, by, *color, 2, 1.0);
                draw_seg(&mut full, rx, ty, rx, by, *color, 2, 1.0);
            }
            ShotMark::Arrow { x0, y0, x1, y1, color } => {
                draw_seg(&mut full, *x0, *y0, *x1, *y1, *color, 2, 1.0);
                let ang = (y1 - y0).atan2(x1 - x0);
                let head = 15.0f32;
                let a1 = ang + 2.62;
                let a2 = ang - 2.62;
                draw_seg(&mut full, *x1, *y1, x1 + head * a1.cos(), y1 + head * a1.sin(), *color, 2, 1.0);
                draw_seg(&mut full, *x1, *y1, x1 + head * a2.cos(), y1 + head * a2.sin(), *color, 2, 1.0);
            }
            ShotMark::Stroke { pts, color, highlight } => {
                let rad = if *highlight { 9 } else { 2 };
                let alpha = if *highlight { 0.25 } else { 1.0 };
                for pair in pts.windows(2) {
                    draw_seg(&mut full, pair[0].0, pair[0].1, pair[1].0, pair[1].1, *color, rad, alpha);
                }
            }
        }
    }

    let cx0 = (sel.x.round() as i32).clamp(0, fw - 1) as u32;
    let cy0 = (sel.y.round() as i32).clamp(0, fh - 1) as u32;
    let cw = (sel.w.round().max(1.0) as u32).min(sw - cx0);
    let ch = (sel.h.round().max(1.0) as u32).min(sh - cy0);

    let mut cropped = vec![0u8; (cw * ch * 4) as usize];
    for row in 0..ch {
        let s_off = (((cy0 + row) * sw + cx0) * 4) as usize;
        let d_off = ((row * cw) * 4) as usize;
        let bytes = (cw * 4) as usize;
        cropped[d_off..d_off + bytes].copy_from_slice(&full[s_off..s_off + bytes]);
    }
    (cw, ch, cropped)
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() >= 3 && args[1].eq_ignore_ascii_case("--capture") {
        run_capture(Path::new(&args[2]));
        return;
    }

    // 1. Freeze desktop pixels BEFORE opening window (zero UI flicker!)
    let (sw, sh, desktop_bgra) = capture_screen_bgra();

    let gpu = match GpuEngine::new() {
        Ok(g) => g,
        Err(_) => return,
    };

    let cls = win32_shell::to_wide("AuraShot_Studio_v5_Wnd");
    let title = win32_shell::to_wide("AuraShot");
    let hwnd = unsafe {
        let hinst = GetModuleHandleW(None).unwrap_or_default();
        let wc = WNDCLASSW {
            lpfnWndProc: Some(shot_wnd_proc),
            hInstance: hinst.into(),
            hCursor: LoadCursorW(None, IDC_CROSS).unwrap_or_default(),
            lpszClassName: PCWSTR(cls.as_ptr()),
            ..Default::default()
        };
        RegisterClassW(&wc);
        CreateWindowExW(
            WS_EX_NOREDIRECTIONBITMAP | WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
            PCWSTR(cls.as_ptr()),
            PCWSTR(title.as_ptr()),
            WS_POPUP,
            0,
            0,
            sw as i32,
            sh as i32,
            HWND::default(),
            None,
            hinst,
            None,
        )
        .unwrap_or_default()
    };
    if hwnd.is_invalid() {
        return;
    }
    aura_window::apply_app_icon(hwnd);

    let Ok(surf) = WindowSurface::new(&gpu, hwnd, sw, sh) else {
        return;
    };

    let mut rt = ShotRuntime {
        hwnd,
        gpu,
        surf,
        sw,
        sh,
        desktop_bgra,
        selection: None,
        selecting_region: false,
        sel_start: (0.0, 0.0),
        tool: ShotTool::RegionSelect,
        marks: Vec::new(),
        drag_mark_start: None,
        drag_mark_cur: None,
        active_stroke: Vec::new(),
        hovered_id: None,
        hits: Vec::new(),
    };

    rt.redraw();
    unsafe {
        let _ = SetWindowPos(hwnd, HWND_TOPMOST, 0, 0, sw as i32, sh as i32, SWP_SHOWWINDOW);
        let _ = ShowWindow(hwnd, SW_SHOW);
        win32_shell::bring_to_front(hwnd);
    }

    SHOT_RT.with(|c| *c.borrow_mut() = Some(rt));

    unsafe {
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, HWND::default(), 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }

    SHOT_RT.with(|c| {
        if let Ok(mut g) = c.try_borrow_mut() {
            *g = None;
        }
    });
}

fn run_capture(out_path: &Path) {
    let Ok(mut gpu) = GpuEngine::new() else { return };
    let w = 1280u32;
    let h = 760u32;
    // Synthesize a rich desktop backdrop so QA screenshot clearly demonstrates region spotlight + markup!
    let mut mock = vec![0u8; (w * h * 4) as usize];
    for y in 0..h {
        for x in 0..w {
            let i = ((y * w + x) * 4) as usize;
            let fx = x as f32 / w as f32;
            let fy = y as f32 / h as f32;
            mock[i] = (34.0 + 70.0 * (1.0 - fy) + 45.0 * fx) as u8;
            mock[i + 1] = (22.0 + 38.0 * fy) as u8;
            mock[i + 2] = (28.0 + 82.0 * fx * (1.0 - fy * 0.5)) as u8;
            mock[i + 3] = 255;
        }
    }
    let sel = Some(Rect::new(220.0, 120.0, 820.0, 460.0));
    let marks = vec![
        ShotMark::Rect {
            x0: 290.0,
            y0: 190.0,
            x1: 620.0,
            y1: 360.0,
            color: [124, 92, 255],
        },
        ShotMark::Arrow {
            x0: 780.0,
            y0: 430.0,
            x1: 635.0,
            y1: 330.0,
            color: [244, 63, 94],
        },
    ];

    let _ = gpu.render_to_png(w, h, out_path, |ctx| {
        let mut hits = Vec::new();
        render_shot_ui(
            ctx,
            w,
            h,
            &mock,
            sel,
            ShotTool::Arrow,
            &marks,
            None,
            None,
            &[],
            None,
            &mut hits,
        );
    });
}

impl ShotRuntime {
    fn effective_sel(&self) -> Rect {
        self.selection
            .filter(|r| r.w >= 8.0 && r.h >= 8.0)
            .unwrap_or_else(|| Rect::new(0.0, 0.0, self.sw as f32, self.sh as f32))
    }

    fn execute(&mut self, act: ShotAct) {
        match act {
            ShotAct::SetTool(t) => {
                self.tool = t;
            }
            ShotAct::FullScreenSel => {
                self.selection = Some(Rect::new(0.0, 0.0, self.sw as f32, self.sh as f32));
                self.tool = ShotTool::Arrow;
            }
            ShotAct::Undo => {
                self.marks.pop();
            }
            ShotAct::OcrCopyExit => {
                let sel = self.effective_sel();
                let (cw, ch, cropped) =
                    bake_cropped_region(self.sw, self.sh, &self.desktop_bgra, sel, &self.marks);
                if let Some(text) = run_winrt_ocr_bgra(cw, ch, &cropped) {
                    if !text.trim().is_empty() {
                        win32_shell::copy_text_to_clipboard(text.trim());
                    } else {
                        let _ = win32_shell::copy_bgra_to_clipboard(cw, ch, &cropped);
                    }
                } else {
                    let _ = win32_shell::copy_bgra_to_clipboard(cw, ch, &cropped);
                }
                unsafe {
                    let _ = ShowWindow(self.hwnd, SW_HIDE);
                    PostQuitMessage(0);
                }
                return;
            }
            ShotAct::CopyExit => {
                let sel = self.effective_sel();
                let (cw, ch, cropped) =
                    bake_cropped_region(self.sw, self.sh, &self.desktop_bgra, sel, &self.marks);
                let _ = win32_shell::copy_bgra_to_clipboard(cw, ch, &cropped);
                unsafe {
                    let _ = ShowWindow(self.hwnd, SW_HIDE);
                    PostQuitMessage(0);
                }
                return;
            }
            ShotAct::SavePngExit => {
                let sel = self.effective_sel();
                let (cw, ch, cropped) =
                    bake_cropped_region(self.sw, self.sh, &self.desktop_bgra, sel, &self.marks);
                let _ = win32_shell::copy_bgra_to_clipboard(cw, ch, &cropped);
                let base_dir = std::env::var("USERPROFILE")
                    .map(|p| PathBuf::from(p).join("Pictures").join("AuraShots"))
                    .unwrap_or_else(|_| aura_os::models::app_data_dir().join("AuraShots"));
                let ts = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0);
                let out = base_dir.join(format!("AuraShot_{}.png", ts));
                let _ = save_bgra_png(&out, cw, ch, &cropped);
                unsafe {
                    let _ = ShowWindow(self.hwnd, SW_HIDE);
                    PostQuitMessage(0);
                }
                return;
            }
            ShotAct::CancelExit => unsafe {
                let _ = ShowWindow(self.hwnd, SW_HIDE);
                PostQuitMessage(0);
                return;
            },
        }
        self.redraw();
    }

    fn redraw(&mut self) {
        let sw = self.sw;
        let sh = self.sh;
        let bgra = &self.desktop_bgra;
        let sel = self.selection;
        let tool = self.tool;
        let marks = &self.marks;
        let d0 = self.drag_mark_start;
        let d1 = self.drag_mark_cur;
        let stroke = &self.active_stroke;
        let hov = self.hovered_id.as_deref();
        let hits = &mut self.hits;

        self.surf.draw(&mut self.gpu, |ctx| {
            render_shot_ui(ctx, sw, sh, bgra, sel, tool, marks, d0, d1, stroke, hov, hits);
        });

        let nodes: Vec<(String, String, f32, f32, f32, f32)> = self
            .hits
            .iter()
            .map(|h| (h.id.clone(), h.label.clone(), h.rect.x, h.rect.y, h.rect.w, h.rect.h))
            .collect();
        agent_bridge::publish_ui_tree("AuraShot", self.hwnd, &nodes);
    }
}

#[allow(clippy::too_many_arguments)]
fn render_shot_ui(
    ctx: &mut DrawCtx<'_>,
    sw: u32,
    sh: u32,
    desktop_bgra: &[u8],
    selection: Option<Rect>,
    tool: ShotTool,
    marks: &[ShotMark],
    drag_start: Option<(f32, f32)>,
    drag_cur: Option<(f32, f32)>,
    active_stroke: &[(f32, f32)],
    hovered_id: Option<&str>,
    hits: &mut Vec<Hit>,
) {
    hits.clear();
    let fw = sw as f32;
    let fh = sh as f32;

    // 1. Frozen Desktop Frame
    ctx.draw_bgra_live("shot_desk", sw, sh, desktop_bgra, Rect::new(0.0, 0.0, fw, fh));

    // 2. Render Annotations
    for m in marks {
        draw_shot_mark(ctx, m);
    }
    if let (Some((x0, y0)), Some((x1, y1))) = (drag_start, drag_cur) {
        match tool {
            ShotTool::Arrow => draw_shot_mark(
                ctx,
                &ShotMark::Arrow { x0, y0, x1, y1, color: [244, 63, 94] },
            ),
            ShotTool::RectBox => draw_shot_mark(
                ctx,
                &ShotMark::Rect { x0, y0, x1, y1, color: [124, 92, 255] },
            ),
            ShotTool::Pixelate => draw_shot_mark(ctx, &ShotMark::Pixelate { x0, y0, x1, y1 }),
            _ => {}
        }
    }
    if active_stroke.len() >= 2 {
        draw_shot_mark(
            ctx,
            &ShotMark::Stroke {
                pts: active_stroke.to_vec(),
                color: if tool == ShotTool::Highlighter { [250, 204, 21] } else { [244, 63, 94] },
                highlight: tool == ShotTool::Highlighter,
            },
        );
    }

    // 3. Dark Shroud Around Selection
    let dim = theme::rgba(8, 8, 14, 0.56);
    if let Some(s) = selection.filter(|r| r.w > 4.0 && r.h > 4.0) {
        ctx.fill_rrect(Rect::new(0.0, 0.0, fw, s.y), 0.0, dim);
        ctx.fill_rrect(Rect::new(0.0, s.y + s.h, fw, (fh - (s.y + s.h)).max(0.0)), 0.0, dim);
        ctx.fill_rrect(Rect::new(0.0, s.y, s.x, s.h), 0.0, dim);
        ctx.fill_rrect(Rect::new(s.x + s.w, s.y, (fw - (s.x + s.w)).max(0.0), s.h), 0.0, dim);

        // Crisp Electric-Violet Selection Frame + Corner Handles
        ctx.stroke_rrect(s, 2.0, theme::VIOLET_LIGHT, 2.0);
        for (cx, cy) in [
            (s.x, s.y),
            (s.x + s.w, s.y),
            (s.x, s.y + s.h),
            (s.x + s.w, s.y + s.h),
        ] {
            ctx.fill_circle(cx, cy, 4.5, theme::CRISP_WHITE);
            ctx.stroke_circle(cx, cy, 4.5, theme::VIOLET, 1.8);
        }

        // Dimensions Pill (`820 × 460 px`)
        let dim_pill = Rect::new(s.x, (s.y - 30.0).max(8.0), 116.0, 24.0);
        ctx.fill_card(dim_pill, 7.0, theme::rgba(14, 14, 22, 0.92), theme::VIOLET_BORDER);
        ctx.draw_text_center(
            &format!("{} × {} px", s.w.round() as i32, s.h.round() as i32),
            dim_pill,
            TextStyle::Mono11Med,
            theme::CRISP_WHITE,
        );
    } else {
        ctx.fill_rrect(Rect::new(0.0, 0.0, fw, fh), 0.0, theme::rgba(8, 8, 14, 0.42));
        let banner = Rect::new((fw - 440.0) * 0.5, 28.0, 440.0, 42.0);
        ctx.fill_card(banner, 14.0, theme::rgba(18, 18, 28, 0.94), theme::VIOLET_BORDER);
        ctx.draw_text_center(
            "AuraShot · Arrastra para recortar región o pulsa 'Pantalla completa'",
            banner,
            TextStyle::Body12Semi,
            theme::CRISP_WHITE,
        );
    }

    // 4. Floating Studio Action Bar (with Built-In WinRT OCR Text Extraction!)
    let bar_w = 816.0f32;
    let bar_h = 44.0f32;
    let (bx, by) = if let Some(s) = selection.filter(|r| r.w > 20.0 && r.h > 20.0) {
        let x = (s.x + s.w - bar_w).clamp(16.0, (fw - bar_w - 16.0).max(16.0));
        let y = if s.y + s.h + bar_h + 18.0 < fh {
            s.y + s.h + 10.0
        } else {
            (s.y - bar_h - 10.0).max(14.0)
        };
        (x, y)
    } else {
        ((fw - bar_w) * 0.5, fh - 76.0)
    };

    let bar = Rect::new(bx, by, bar_w, bar_h);
    ctx.fill_card(bar, 14.0, theme::rgba(18, 18, 27, 0.96), theme::BORDER_STRONG);

    let items: [(&str, &str, ShotAct, bool, bool); 11] = [
        ("sh_reg", "Región", ShotAct::SetTool(ShotTool::RegionSelect), tool == ShotTool::RegionSelect, false),
        ("sh_arr", "Flecha", ShotAct::SetTool(ShotTool::Arrow), tool == ShotTool::Arrow, false),
        ("sh_rct", "Cuadro", ShotAct::SetTool(ShotTool::RectBox), tool == ShotTool::RectBox, false),
        ("sh_hl", "Resaltar", ShotAct::SetTool(ShotTool::Highlighter), tool == ShotTool::Highlighter, false),
        ("sh_pen", "Pluma", ShotAct::SetTool(ShotTool::Pen), tool == ShotTool::Pen, false),
        ("sh_px", "Pixelar", ShotAct::SetTool(ShotTool::Pixelate), tool == ShotTool::Pixelate, false),
        ("sh_und", "Deshacer", ShotAct::Undo, false, false),
        ("sh_ful", "Completa", ShotAct::FullScreenSel, false, false),
        ("sh_ocr", "Texto OCR", ShotAct::OcrCopyExit, false, false),
        ("sh_sav", "Guardar PNG", ShotAct::SavePngExit, false, false),
        ("sh_cpy", "Copiar ✓", ShotAct::CopyExit, false, true),
    ];

    let mut ix = bx + 8.0;
    for (id, lbl, act, active, primary) in items {
        let iw = (lbl.len() as f32 * 6.5 + 16.0).max(56.0);
        let ir = Rect::new(ix, by + 7.0, iw, 30.0);
        let hov = hovered_id == Some(id);
        let bg = if primary {
            if hov { theme::VIOLET_LIGHT } else { theme::VIOLET }
        } else if active {
            theme::VIOLET_TINT
        } else if hov {
            theme::ELEVATED_HOVER
        } else {
            theme::ELEVATED
        };
        let border = if primary || active { theme::VIOLET_LIGHT } else { theme::BORDER };
        ctx.fill_card(ir, 9.0, bg, border);
        ctx.draw_text_center(
            lbl,
            ir,
            TextStyle::Body11Med,
            if active && !primary { theme::VIOLET_LIGHT } else { theme::CRISP_WHITE },
        );
        hits.push(Hit { id: id.into(), label: lbl.into(), rect: ir, act });
        ix += iw + 5.0;
    }
}

fn draw_shot_mark(ctx: &mut DrawCtx<'_>, m: &ShotMark) {
    match m {
        ShotMark::Rect { x0, y0, x1, y1, color } => {
            let r = Rect::new(x0.min(*x1), y0.min(*y1), (x1 - x0).abs().max(2.0), (y1 - y0).abs().max(2.0));
            ctx.stroke_rrect(r, 4.0, theme::rgb(color[0], color[1], color[2]), 2.8);
        }
        ShotMark::Arrow { x0, y0, x1, y1, color } => {
            let col = theme::rgb(color[0], color[1], color[2]);
            ctx.draw_line(*x0, *y0, *x1, *y1, col, 3.2);
            let ang = (y1 - y0).atan2(x1 - x0);
            let head = 15.0f32;
            ctx.draw_line(*x1, *y1, x1 + head * (ang + 2.62).cos(), y1 + head * (ang + 2.62).sin(), col, 3.2);
            ctx.draw_line(*x1, *y1, x1 + head * (ang - 2.62).cos(), y1 + head * (ang - 2.62).sin(), col, 3.2);
        }
        ShotMark::Pixelate { x0, y0, x1, y1 } => {
            let r = Rect::new(x0.min(*x1), y0.min(*y1), (x1 - x0).abs().max(4.0), (y1 - y0).abs().max(4.0));
            ctx.fill_rrect(r, 4.0, theme::rgba(20, 20, 30, 0.78));
            ctx.stroke_rrect(r, 4.0, theme::AMBER, 1.5);
            ctx.draw_text_center("CENSURADO", r, TextStyle::Mono11Med, theme::AMBER);
        }
        ShotMark::Stroke { pts, color, highlight } => {
            let col = theme::rgba(color[0], color[1], color[2], if *highlight { 0.34 } else { 0.96 });
            let sw = if *highlight { 18.0 } else { 3.2 };
            for pair in pts.windows(2) {
                ctx.draw_line(pair[0].0, pair[0].1, pair[1].0, pair[1].1, col, sw);
            }
        }
    }
}

unsafe extern "system" fn shot_wnd_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if msg == WM_CLOSE || msg == WM_DESTROY {
        PostQuitMessage(0);
        return LRESULT(0);
    }

    let handled = SHOT_RT.with(|cell| {
        let Ok(mut opt) = cell.try_borrow_mut() else { return false };
        let Some(rt) = opt.as_mut() else { return false };

        match msg {
            WM_MOUSEMOVE => {
                let px = (lparam.0 & 0xFFFF) as i16 as f32;
                let py = ((lparam.0 >> 16) & 0xFFFF) as i16 as f32;
                if rt.selecting_region {
                    let (sx, sy) = rt.sel_start;
                    rt.selection = Some(Rect::new(
                        sx.min(px),
                        sy.min(py),
                        (px - sx).abs(),
                        (py - sy).abs(),
                    ));
                    rt.redraw();
                    return true;
                }
                if !rt.active_stroke.is_empty() {
                    rt.active_stroke.push((px, py));
                    rt.redraw();
                    return true;
                }
                if rt.drag_mark_start.is_some() {
                    rt.drag_mark_cur = Some((px, py));
                    rt.redraw();
                    return true;
                }
                let nh = rt.hits.iter().rev().find(|h| h.rect.contains(px, py)).map(|h| h.id.clone());
                if nh != rt.hovered_id {
                    rt.hovered_id = nh;
                    rt.redraw();
                }
                true
            }
            WM_LBUTTONDOWN => {
                let px = (lparam.0 & 0xFFFF) as i16 as f32;
                let py = ((lparam.0 >> 16) & 0xFFFF) as i16 as f32;
                if let Some(hit) = rt.hits.iter().rev().find(|h| h.rect.contains(px, py)).cloned() {
                    rt.execute(hit.act);
                    return true;
                }

                if rt.tool == ShotTool::RegionSelect || rt.selection.is_none() {
                    rt.selecting_region = true;
                    rt.sel_start = (px, py);
                    rt.selection = Some(Rect::new(px, py, 1.0, 1.0));
                    SetCapture(hwnd);
                } else {
                    match rt.tool {
                        ShotTool::Highlighter | ShotTool::Pen => {
                            rt.active_stroke.clear();
                            rt.active_stroke.push((px, py));
                            SetCapture(hwnd);
                        }
                        ShotTool::Arrow | ShotTool::RectBox | ShotTool::Pixelate => {
                            rt.drag_mark_start = Some((px, py));
                            rt.drag_mark_cur = Some((px, py));
                            SetCapture(hwnd);
                        }
                        ShotTool::RegionSelect => {}
                    }
                }
                true
            }
            WM_LBUTTONUP => {
                if rt.selecting_region {
                    rt.selecting_region = false;
                    let _ = ReleaseCapture();
                    if rt.selection.map(|r| r.w > 12.0 && r.h > 12.0).unwrap_or(false) {
                        rt.tool = ShotTool::Arrow;
                    }
                    rt.redraw();
                } else if !rt.active_stroke.is_empty() {
                    let pts = std::mem::take(&mut rt.active_stroke);
                    if pts.len() >= 2 {
                        let hl = rt.tool == ShotTool::Highlighter;
                        rt.marks.push(ShotMark::Stroke {
                            pts,
                            color: if hl { [250, 204, 21] } else { [244, 63, 94] },
                            highlight: hl,
                        });
                    }
                    let _ = ReleaseCapture();
                    rt.redraw();
                } else if let (Some((x0, y0)), Some((x1, y1))) = (rt.drag_mark_start.take(), rt.drag_mark_cur.take()) {
                    if (x1 - x0).hypot(y1 - y0) > 6.0 {
                        match rt.tool {
                            ShotTool::Arrow => rt.marks.push(ShotMark::Arrow { x0, y0, x1, y1, color: [244, 63, 94] }),
                            ShotTool::RectBox => rt.marks.push(ShotMark::Rect { x0, y0, x1, y1, color: [124, 92, 255] }),
                            ShotTool::Pixelate => rt.marks.push(ShotMark::Pixelate { x0, y0, x1, y1 }),
                            _ => {}
                        }
                    }
                    let _ = ReleaseCapture();
                    rt.redraw();
                }
                true
            }
            0x0215 | 0x001F => {
                // WM_CAPTURECHANGED / WM_CANCELMODE: release any stuck drag capture cleanly!
                rt.selecting_region = false;
                rt.active_stroke.clear();
                rt.drag_mark_start = None;
                rt.drag_mark_cur = None;
                true
            }
            WM_RBUTTONDOWN => {
                rt.execute(ShotAct::CancelExit);
                true
            }
            WM_KEYDOWN => {
                let vk = wparam.0 as u16;
                if vk == VK_ESCAPE.0 {
                    rt.execute(ShotAct::CancelExit);
                } else if vk == VK_RETURN.0 {
                    rt.execute(ShotAct::CopyExit);
                }
                true
            }
            _ => false,
        }
    });

    if handled {
        LRESULT(0)
    } else {
        DefWindowProcW(hwnd, msg, wparam, lparam)
    }
}
