//! AuraPhotos Studio v5.0 — Pure Rust + Direct2D Instant Image Viewer & Non-Destructive Image Editor
//! - Launches in <40ms, replaces both Windows Photos AND basic Pixelmator/macOS Preview editing!
//! - Viewer: Full-res GPU WIC decoding, smooth zoom & pan, folder filmstrip, 1-click AuraOS Wallpaper
//! - Studio Editor (`E` or `Editar Studio`):
//!   * Live Color Pipeline: Brightness, Contrast, Saturation, Warmth + Presets (Auto, B&W, Sepia, Reset)
//!   * Transform & Crop: Rotate 90°, Flip Horizontal/Vertical, Aspect Crop (`Libre`, `1:1`, `16:9`, `9:16`, `4:3`)
//!   * Markup & Privacy: Freehand Brush, Arrow, Rectangle, Highlighter, Mosaic Pixelate, Color Swatches, Undo
//!   * Export: 1-click Copy (`CF_DIB` Clipboard) + Save PNG/JPG (`IFileSaveDialog` + WIC)!

#![windows_subsystem = "windows"]

use aura_os::agent_bridge;
use aura_os::aura_window::{self, SnapZone};
use aura_os::models::{load_settings, save_settings};
use aura_os::renderer::{DecodedAsset, DrawCtx, GpuEngine, Rect, TextStyle, WindowSurface};
use aura_os::theme::{self, VectorIcon};
use aura_os::win32_shell;
use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::time::Instant;
use windows::core::PCWSTR;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Imaging::{
    CLSID_WICImagingFactory, GUID_ContainerFormatJpeg, GUID_ContainerFormatPng,
    GUID_WICPixelFormat24bppBGR, GUID_WICPixelFormat32bppBGRA, IWICImagingFactory,
    WICBitmapEncoderNoCache,
};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    ReleaseCapture, SetCapture, VK_DELETE, VK_DOWN, VK_ESCAPE, VK_LEFT, VK_OEM_MINUS, VK_OEM_PLUS,
    VK_RIGHT, VK_UP,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, GetClientRect, GetMessageW, IsZoomed,
    LoadCursorW, PostQuitMessage, RegisterClassW, SendMessageW, SetTimer, ShowWindow,
    TranslateMessage, HTCAPTION, IDC_ARROW, MSG, SW_HIDE, SW_MAXIMIZE, SW_MINIMIZE, SW_RESTORE,
    SW_SHOW, WM_CLOSE, WM_DESTROY, WM_GETMINMAXINFO, WM_KEYDOWN, WM_LBUTTONDBLCLK, WM_LBUTTONDOWN,
    WM_LBUTTONUP, WM_MOUSEMOVE, WM_MOUSEWHEEL, WM_NCCALCSIZE, WM_NCHITTEST, WM_NCLBUTTONDOWN,
    WM_RBUTTONDOWN, WM_SIZE, WM_SYSKEYDOWN, WM_TIMER, WNDCLASSW, WS_EX_NOREDIRECTIONBITMAP,
};

const TIMER_ASYNC: usize = 20;
const TOP_BAR_H: f32 = 48.0;
const FILMSTRIP_H: f32 = 76.0;
const EDITOR_W: f32 = 282.0;

#[derive(Clone, Copy, PartialEq, Eq)]
enum EditTool {
    PanZoom,
    Crop,
    Brush,
    Highlighter,
    Arrow,
    RectBox,
    Pixelate,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SliderKind {
    Brightness,
    Contrast,
    Saturation,
    Warmth,
}

#[derive(Clone)]
enum ImgMark {
    Stroke { pts: Vec<(f32, f32)>, color: [u8; 3], highlight: bool },
    Arrow { x0: f32, y0: f32, x1: f32, y1: f32, color: [u8; 3] },
    Rect { x0: f32, y0: f32, x1: f32, y1: f32, color: [u8; 3] },
    Pixelate { x0: f32, y0: f32, x1: f32, y1: f32 },
}

#[derive(Clone)]
struct ColorParams {
    brightness: f32, // -100..+100
    contrast: f32,   // -100..+100
    saturation: f32, // -100..+100
    warmth: f32,     // -100..+100
    sepia: bool,
}

impl Default for ColorParams {
    fn default() -> Self {
        Self {
            brightness: 0.0,
            contrast: 0.0,
            saturation: 0.0,
            warmth: 0.0,
            sepia: false,
        }
    }
}

impl ColorParams {
    fn is_identity(&self) -> bool {
        self.brightness.abs() < 0.5
            && self.contrast.abs() < 0.5
            && self.saturation.abs() < 0.5
            && self.warmth.abs() < 0.5
            && !self.sepia
    }
}

const SWATCHES: [[u8; 3]; 5] = [
    [124, 92, 255],
    [244, 63, 94],
    [250, 204, 21],
    [34, 211, 238],
    [248, 249, 252],
];

#[derive(Clone)]
enum PhotoAct {
    WinClose,
    WinMin,
    WinMax,
    OpenDialog,
    Prev,
    Next,
    ZoomIn,
    ZoomOut,
    ZoomFit,
    Zoom100,
    RotateCw,
    FlipH,
    FlipV,
    SetWallpaper,
    OpenInAuraFiles,
    DeleteRecycle,
    SelectIndex(usize),
    ToggleFilmstrip,
    ToggleEditor,
    SetEditTool(EditTool),
    SetColorSwatch(usize),
    SetCropPreset(Option<(f32, f32)>),
    ApplyCrop,
    PresetAuto,
    PresetBW,
    PresetSepia,
    ResetEdits,
    UndoMark,
    CopyClipboard,
    SaveCopyDialog,
}

#[derive(Clone)]
struct Hit {
    id: String,
    rect: Rect,
    act: PhotoAct,
    slider: Option<SliderKind>,
}

struct PhotoRuntime {
    hwnd: HWND,
    gpu: GpuEngine,
    surf: WindowSurface,
    width: f32,
    height: f32,
    maximized: bool,

    siblings: Vec<PathBuf>,
    cur_idx: usize,
    base_asset: Option<DecodedAsset>,
    preview_bgra: Vec<u8>,
    preview_w: u32,
    preview_h: u32,
    preview_dirty: bool,
    asset_rev: u64,
    file_bytes: u64,

    edit_mode: bool,
    tool: EditTool,
    swatch_idx: usize,
    params: ColorParams,
    marks: Vec<ImgMark>,
    active_stroke: Vec<(f32, f32)>,
    drag_shape: Option<((f32, f32), (f32, f32))>,
    crop_norm: Rect, // normalized [0..1] crop box

    active_slider: Option<(SliderKind, Rect)>,

    zoom: f32,
    pan_x: f32,
    pan_y: f32,
    dragging: bool,
    drag_last: (f32, f32),
    last_img_screen_rect: Rect,

    show_filmstrip: bool,
    hovered_id: Option<String>,
    hits: Vec<Hit>,
    toast: Option<(String, Instant)>,
}

thread_local! {
    static PHOTO_RT: RefCell<Option<PhotoRuntime>> = const { RefCell::new(None) };
}

fn format_bytes(b: u64) -> String {
    if b >= 1_048_576 {
        format!("{:.1} MB", b as f64 / 1_048_576.0)
    } else if b >= 1024 {
        format!("{:.0} KB", b as f64 / 1024.0)
    } else {
        format!("{} B", b)
    }
}

fn rotate_bgra_90cw(asset: &DecodedAsset) -> DecodedAsset {
    let w = asset.width as usize;
    let h = asset.height as usize;
    let mut out = vec![0u8; w * h * 4];
    for y in 0..h {
        for x in 0..w {
            let si = (y * w + x) * 4;
            let nx = h - 1 - y;
            let ny = x;
            let di = (ny * h + nx) * 4;
            out[di..di + 4].copy_from_slice(&asset.bgra[si..si + 4]);
        }
    }
    DecodedAsset {
        width: asset.height,
        height: asset.width,
        bgra: out,
    }
}

fn flip_bgra(asset: &DecodedAsset, horiz: bool) -> DecodedAsset {
    let w = asset.width as usize;
    let h = asset.height as usize;
    let mut out = vec![0u8; w * h * 4];
    for y in 0..h {
        for x in 0..w {
            let sx = if horiz { w - 1 - x } else { x };
            let sy = if horiz { y } else { h - 1 - y };
            let si = (sy * w + sx) * 4;
            let di = (y * w + x) * 4;
            out[di..di + 4].copy_from_slice(&asset.bgra[si..si + 4]);
        }
    }
    DecodedAsset {
        width: asset.width,
        height: asset.height,
        bgra: out,
    }
}

fn crop_asset(asset: &DecodedAsset, norm: Rect) -> DecodedAsset {
    let w = asset.width.max(1);
    let h = asset.height.max(1);
    let x0 = ((norm.x.clamp(0.0, 0.98) * w as f32).round() as u32).min(w - 1);
    let y0 = ((norm.y.clamp(0.0, 0.98) * h as f32).round() as u32).min(h - 1);
    let cw = ((norm.w.clamp(0.02, 1.0) * w as f32).round() as u32).clamp(1, w - x0);
    let ch = ((norm.h.clamp(0.02, 1.0) * h as f32).round() as u32).clamp(1, h - y0);

    let mut out = vec![0u8; (cw * ch * 4) as usize];
    for ry in 0..ch {
        let so = (((y0 + ry) * w + x0) * 4) as usize;
        let doff = ((ry * cw) * 4) as usize;
        let row_b = (cw * 4) as usize;
        out[doff..doff + row_b].copy_from_slice(&asset.bgra[so..so + row_b]);
    }
    DecodedAsset {
        width: cw,
        height: ch,
        bgra: out,
    }
}

fn apply_color_and_marks(asset: &DecodedAsset, params: &ColorParams, marks: &[ImgMark]) -> Vec<u8> {
    let mut buf = asset.bgra.clone();
    if !params.is_identity() {
        let br = params.brightness * 1.8;
        let cf = ((params.contrast + 100.0) / 100.0).powi(2);
        let sf = (params.saturation + 100.0) / 100.0;
        let warm = params.warmth * 0.65;

        for px in buf.chunks_exact_mut(4) {
            let a = px[3];
            if a == 0 {
                continue;
            }
            let mut b = px[0] as f32;
            let mut g = px[1] as f32;
            let mut r = px[2] as f32;

            // Brightness & Warmth
            r += br + warm;
            g += br + warm * 0.18;
            b += br - warm;

            // Contrast around midpoint 128
            r = (r - 128.0) * cf + 128.0;
            g = (g - 128.0) * cf + 128.0;
            b = (b - 128.0) * cf + 128.0;

            // Saturation
            let luma = 0.299 * r + 0.587 * g + 0.114 * b;
            r = luma + (r - luma) * sf;
            g = luma + (g - luma) * sf;
            b = luma + (b - luma) * sf;

            if params.sepia {
                let sr = (r * 0.393 + g * 0.769 + b * 0.189).min(255.0);
                let sg = (r * 0.349 + g * 0.686 + b * 0.168).min(255.0);
                let sb = (r * 0.272 + g * 0.534 + b * 0.131).min(255.0);
                r = sr;
                g = sg;
                b = sb;
            }

            px[0] = b.clamp(0.0, 255.0) as u8;
            px[1] = g.clamp(0.0, 255.0) as u8;
            px[2] = r.clamp(0.0, 255.0) as u8;
        }
    }

    let fw = asset.width as i32;
    let fh = asset.height as i32;
    if fw <= 0 || fh <= 0 || marks.is_empty() {
        return buf;
    }

    let put = |buf: &mut [u8], x: i32, y: i32, c: [u8; 3], alpha: f32| {
        if x < 0 || y < 0 || x >= fw || y >= fh {
            return;
        }
        let i = ((y * fw + x) * 4) as usize;
        let inv = 1.0 - alpha;
        buf[i] = (c[2] as f32 * alpha + buf[i] as f32 * inv).clamp(0.0, 255.0) as u8;
        buf[i + 1] = (c[1] as f32 * alpha + buf[i + 1] as f32 * inv).clamp(0.0, 255.0) as u8;
        buf[i + 2] = (c[0] as f32 * alpha + buf[i + 2] as f32 * inv).clamp(0.0, 255.0) as u8;
        buf[i + 3] = 255;
    };

    let seg = |buf: &mut [u8], x0: f32, y0: f32, x1: f32, y1: f32, c: [u8; 3], rad: i32, alpha: f32| {
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
            ImgMark::Pixelate { x0, y0, x1, y1 } => {
                let lx = (x0.min(*x1) * fw as f32) as i32;
                let rx = (x0.max(*x1) * fw as f32) as i32;
                let ty = (y0.min(*y1) * fh as f32) as i32;
                let by = (y0.max(*y1) * fh as f32) as i32;
                let blk = (fw / 64).clamp(10, 28);
                let mut yy = ty.max(0);
                while yy < by.min(fh) {
                    let mut xx = lx.max(0);
                    while xx < rx.min(fw) {
                        let sx = (xx + blk / 2).min(fw - 1);
                        let sy = (yy + blk / 2).min(fh - 1);
                        let si = ((sy * fw + sx) * 4) as usize;
                        let sample = [buf[si], buf[si + 1], buf[si + 2], 255];
                        for py in yy..(yy + blk).min(by.min(fh)) {
                            for px in xx..(xx + blk).min(rx.min(fw)) {
                                let di = ((py * fw + px) * 4) as usize;
                                buf[di..di + 4].copy_from_slice(&sample);
                            }
                        }
                        xx += blk;
                    }
                    yy += blk;
                }
            }
            ImgMark::Stroke { pts, color, highlight } => {
                let rad = if *highlight { (fw / 90).clamp(6, 22) } else { (fw / 340).clamp(2, 7) };
                let alpha = if *highlight { 0.25 } else { 1.0 };
                for pair in pts.windows(2) {
                    seg(
                        &mut buf,
                        pair[0].0 * fw as f32,
                        pair[0].1 * fh as f32,
                        pair[1].0 * fw as f32,
                        pair[1].1 * fh as f32,
                        *color,
                        rad,
                        alpha,
                    );
                }
            }
            ImgMark::Rect { x0, y0, x1, y1, color } => {
                let lx = x0.min(*x1) * fw as f32;
                let rx = x0.max(*x1) * fw as f32;
                let ty = y0.min(*y1) * fh as f32;
                let by = y0.max(*y1) * fh as f32;
                let rad = (fw / 380).clamp(2, 6);
                seg(&mut buf, lx, ty, rx, ty, *color, rad, 1.0);
                seg(&mut buf, lx, by, rx, by, *color, rad, 1.0);
                seg(&mut buf, lx, ty, lx, by, *color, rad, 1.0);
                seg(&mut buf, rx, ty, rx, by, *color, rad, 1.0);
            }
            ImgMark::Arrow { x0, y0, x1, y1, color } => {
                let sx0 = *x0 * fw as f32;
                let sy0 = *y0 * fh as f32;
                let sx1 = *x1 * fw as f32;
                let sy1 = *y1 * fh as f32;
                let rad = (fw / 360).clamp(2, 6);
                seg(&mut buf, sx0, sy0, sx1, sy1, *color, rad, 1.0);
                let ang = (sy1 - sy0).atan2(sx1 - sx0);
                let head = (fw as f32 * 0.022).clamp(14.0, 34.0);
                seg(&mut buf, sx1, sy1, sx1 + head * (ang + 2.62).cos(), sy1 + head * (ang + 2.62).sin(), *color, rad, 1.0);
                seg(&mut buf, sx1, sy1, sx1 + head * (ang - 2.62).cos(), sy1 + head * (ang - 2.62).sin(), *color, rad, 1.0);
            }
        }
    }
    buf
}

fn save_edited_image(path: &Path, width: u32, height: u32, bgra: &[u8]) -> Result<(), String> {
    let is_jpg = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|s| matches!(s.to_lowercase().as_str(), "jpg" | "jpeg"))
        .unwrap_or(false);

    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let wic: IWICImagingFactory =
            CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER)
                .map_err(|e| e.to_string())?;
        if let Some(p) = path.parent() {
            let _ = std::fs::create_dir_all(p);
        }
        let stream = wic.CreateStream().map_err(|e| e.to_string())?;
        let wp = win32_shell::to_wide(&path.to_string_lossy());
        stream
            .InitializeFromFilename(PCWSTR(wp.as_ptr()), windows::Win32::Foundation::GENERIC_WRITE.0)
            .map_err(|e| e.to_string())?;

        if is_jpg {
            let mut bgr24 = Vec::with_capacity(width as usize * height as usize * 3);
            for px in bgra.chunks_exact(4) {
                bgr24.push(px[0]);
                bgr24.push(px[1]);
                bgr24.push(px[2]);
            }
            let bmp = wic
                .CreateBitmapFromMemory(width, height, &GUID_WICPixelFormat24bppBGR, width * 3, &bgr24)
                .map_err(|e| e.to_string())?;
            let enc = wic.CreateEncoder(&GUID_ContainerFormatJpeg, std::ptr::null()).map_err(|e| e.to_string())?;
            enc.Initialize(&stream, WICBitmapEncoderNoCache).map_err(|e| e.to_string())?;
            let mut frame = None;
            let mut bag = None;
            enc.CreateNewFrame(&mut frame, &mut bag).map_err(|e| e.to_string())?;
            let frame = frame.ok_or("Sin frame JPEG")?;
            frame.Initialize(bag.as_ref()).map_err(|e| e.to_string())?;
            frame.SetSize(width, height).map_err(|e| e.to_string())?;
            let mut pf = GUID_WICPixelFormat24bppBGR;
            frame.SetPixelFormat(&mut pf).map_err(|e| e.to_string())?;
            frame.WriteSource(&bmp, std::ptr::null()).map_err(|e| e.to_string())?;
            frame.Commit().map_err(|e| e.to_string())?;
            enc.Commit().map_err(|e| e.to_string())?;
        } else {
            let bmp = wic
                .CreateBitmapFromMemory(width, height, &GUID_WICPixelFormat32bppBGRA, width * 4, bgra)
                .map_err(|e| e.to_string())?;
            let enc = wic.CreateEncoder(&GUID_ContainerFormatPng, std::ptr::null()).map_err(|e| e.to_string())?;
            enc.Initialize(&stream, WICBitmapEncoderNoCache).map_err(|e| e.to_string())?;
            let mut frame = None;
            let mut bag = None;
            enc.CreateNewFrame(&mut frame, &mut bag).map_err(|e| e.to_string())?;
            let frame = frame.ok_or("Sin frame PNG")?;
            frame.Initialize(bag.as_ref()).map_err(|e| e.to_string())?;
            frame.SetSize(width, height).map_err(|e| e.to_string())?;
            let mut pf = GUID_WICPixelFormat32bppBGRA;
            frame.SetPixelFormat(&mut pf).map_err(|e| e.to_string())?;
            frame.WriteSource(&bmp, std::ptr::null()).map_err(|e| e.to_string())?;
            frame.Commit().map_err(|e| e.to_string())?;
            enc.Commit().map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

fn scan_sibling_images(path: &Path) -> (Vec<PathBuf>, usize) {
    let Some(parent) = path.parent() else {
        return (vec![path.to_path_buf()], 0);
    };
    let mut list: Vec<PathBuf> = std::fs::read_dir(parent)
        .ok()
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.is_file()
                && p.extension()
                    .and_then(|e| e.to_str())
                    .map(win32_shell::is_image_ext)
                    .unwrap_or(false)
        })
        .collect();
    list.sort_by(|a, b| {
        a.file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_lowercase()
            .cmp(&b.file_name().unwrap_or_default().to_string_lossy().to_lowercase())
    });
    let target_low = path.to_string_lossy().to_lowercase();
    let idx = list
        .iter()
        .position(|p| p.to_string_lossy().to_lowercase() == target_low)
        .unwrap_or(0);
    if list.is_empty() {
        (vec![path.to_path_buf()], 0)
    } else {
        (list, idx)
    }
}

fn find_sample_image() -> Option<PathBuf> {
    if let Ok(prof) = std::env::var("USERPROFILE") {
        for sub in ["Pictures", "Downloads", r"OneDrive\Pictures"] {
            let dir = PathBuf::from(&prof).join(sub);
            if let Ok(rd) = std::fs::read_dir(&dir) {
                for e in rd.flatten() {
                    let p = e.path();
                    if p.is_file()
                        && p.extension()
                            .and_then(|x| x.to_str())
                            .map(win32_shell::is_image_ext)
                            .unwrap_or(false)
                    {
                        return Some(p);
                    }
                }
            }
        }
    }
    let fallback = aura_os::models::app_data_dir().join("AuraShell_256.png");
    if fallback.exists() {
        Some(fallback)
    } else {
        None
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() >= 3
        && (args[1].eq_ignore_ascii_case("--capture")
            || args[1].eq_ignore_ascii_case("--capture-edit"))
    {
        let edit = args[1].eq_ignore_ascii_case("--capture-edit");
        let out_path = PathBuf::from(&args[2]);
        let sample = args.get(3).map(PathBuf::from).or_else(find_sample_image);
        run_capture(&out_path, sample, edit);
        return;
    }

    let initial_file = args.get(1).map(PathBuf::from).filter(|p| p.exists());

    let mut gpu = match GpuEngine::new() {
        Ok(g) => g,
        Err(_) => return,
    };
    gpu.enable_async_loader();

    let (sw, sh) = win32_shell::screen_size();
    let w = (sw as f32 * 0.76).clamp(1020.0, 1440.0) as i32;
    let h = (sh as f32 * 0.80).clamp(660.0, 900.0) as i32;
    let x = (sw - w) / 2;
    let y = ((sh - h) / 2).max(38);

    let cls = win32_shell::to_wide("AuraPhotos_Rust_v5_Wnd");
    let title = win32_shell::to_wide("AuraPhotos Studio");
    let hwnd = unsafe {
        let hinst = GetModuleHandleW(None).unwrap_or_default();
        let wc = WNDCLASSW {
            lpfnWndProc: Some(photos_wnd_proc),
            hInstance: hinst.into(),
            hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
            lpszClassName: PCWSTR(cls.as_ptr()),
            ..Default::default()
        };
        RegisterClassW(&wc);
        CreateWindowExW(
            WS_EX_NOREDIRECTIONBITMAP,
            PCWSTR(cls.as_ptr()),
            PCWSTR(title.as_ptr()),
            aura_window::AURA_APP_STYLE,
            x,
            y,
            w,
            h,
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
    aura_window::configure_frameless_dwm(hwnd);

    let Ok(surf) = WindowSurface::new(&gpu, hwnd, w as u32, h as u32) else {
        return;
    };

    let mut rt = PhotoRuntime {
        hwnd,
        gpu,
        surf,
        width: w as f32,
        height: h as f32,
        maximized: false,
        siblings: Vec::new(),
        cur_idx: 0,
        base_asset: None,
        preview_bgra: Vec::new(),
        preview_w: 0,
        preview_h: 0,
        preview_dirty: false,
        asset_rev: 0,
        file_bytes: 0,
        edit_mode: false,
        tool: EditTool::PanZoom,
        swatch_idx: 0,
        params: ColorParams::default(),
        marks: Vec::new(),
        active_stroke: Vec::new(),
        drag_shape: None,
        crop_norm: Rect::new(0.08, 0.08, 0.84, 0.84),
        active_slider: None,
        zoom: 0.0,
        pan_x: 0.0,
        pan_y: 0.0,
        dragging: false,
        drag_last: (0.0, 0.0),
        last_img_screen_rect: Rect::new(0.0, 0.0, 1.0, 1.0),
        show_filmstrip: true,
        hovered_id: None,
        hits: Vec::new(),
        toast: None,
    };

    if let Some(p) = initial_file {
        rt.open_file(&p);
    }
    rt.redraw();

    unsafe {
        let _ = ShowWindow(hwnd, SW_SHOW);
        let _ = SetTimer(hwnd, TIMER_ASYNC, 33, None);
    }

    PHOTO_RT.with(|c| *c.borrow_mut() = Some(rt));

    unsafe {
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, HWND::default(), 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }

    PHOTO_RT.with(|c| {
        if let Ok(mut g) = c.try_borrow_mut() {
            *g = None;
        }
    });
}

fn run_capture(out_path: &Path, sample: Option<PathBuf>, edit_mode: bool) {
    let Ok(mut gpu) = GpuEngine::new() else { return };
    let (siblings, cur_idx) = sample.as_deref().map(scan_sibling_images).unwrap_or_default();
    let asset = siblings
        .get(cur_idx)
        .and_then(|p| gpu.decode_image_full(&p.to_string_lossy(), 2000));
    let fbytes = siblings
        .get(cur_idx)
        .and_then(|p| std::fs::metadata(p).ok().map(|m| m.len()))
        .unwrap_or(0);

    let params = ColorParams {
        brightness: if edit_mode { 10.0 } else { 0.0 },
        contrast: if edit_mode { 18.0 } else { 0.0 },
        saturation: if edit_mode { 22.0 } else { 0.0 },
        warmth: if edit_mode { 12.0 } else { 0.0 },
        sepia: false,
    };
    let (pw, ph, pbgra) = if let Some(ref a) = asset {
        (a.width, a.height, apply_color_and_marks(a, &params, &[]))
    } else {
        (0, 0, Vec::new())
    };

    let _ = gpu.render_to_png(1320, 820, out_path, |ctx| {
        let mut hits = Vec::new();
        let mut img_r = Rect::new(0.0, 0.0, 1.0, 1.0);
        render_photos_ui(
            ctx,
            1320.0,
            820.0,
            false,
            &siblings,
            cur_idx,
            pw,
            ph,
            &pbgra,
            1,
            fbytes,
            0.0,
            0.0,
            0.0,
            !edit_mode,
            edit_mode,
            if edit_mode { EditTool::Crop } else { EditTool::PanZoom },
            0,
            &params,
            Rect::new(0.10, 0.10, 0.80, 0.80),
            &[],
            None,
            None,
            if edit_mode {
                Some("Modo Estudio activo · Ajustes GPU en tiempo real")
            } else {
                None
            },
            &mut img_r,
            &mut hits,
        );
    });
}

impl PhotoRuntime {
    fn open_file(&mut self, path: &Path) {
        let (sibs, idx) = scan_sibling_images(path);
        self.siblings = sibs;
        self.cur_idx = idx;
        self.load_current();
        let start = self.cur_idx.saturating_sub(6);
        let end = (self.cur_idx + 8).min(self.siblings.len());
        let nearby: Vec<String> = self.siblings[start..end]
            .iter()
            .map(|p| p.to_string_lossy().to_string())
            .collect();
        self.gpu.prewarm_images(nearby);
    }

    fn load_current(&mut self) {
        self.zoom = 0.0;
        self.pan_x = 0.0;
        self.pan_y = 0.0;
        self.params = ColorParams::default();
        self.marks.clear();
        self.crop_norm = Rect::new(0.08, 0.08, 0.84, 0.84);
        let Some(p) = self.siblings.get(self.cur_idx) else {
            self.base_asset = None;
            self.preview_bgra.clear();
            self.preview_w = 0;
            self.preview_h = 0;
            return;
        };
        self.file_bytes = std::fs::metadata(p).map(|m| m.len()).unwrap_or(0);
        if let Some(a) = self.gpu.decode_image_full(&p.to_string_lossy(), 3200) {
            self.preview_w = a.width;
            self.preview_h = a.height;
            self.preview_bgra = a.bgra.clone();
            self.base_asset = Some(a);
            self.preview_dirty = false;
            self.asset_rev = self.asset_rev.wrapping_add(1);
        }
    }

    fn rebuild_preview_if_needed(&mut self) {
        if !self.preview_dirty {
            return;
        }
        if let Some(ref a) = self.base_asset {
            self.preview_w = a.width;
            self.preview_h = a.height;
            self.preview_bgra = apply_color_and_marks(a, &self.params, &self.marks);
            self.asset_rev = self.asset_rev.wrapping_add(1);
        }
        self.preview_dirty = false;
    }

    fn current_path(&self) -> Option<&PathBuf> {
        self.siblings.get(self.cur_idx)
    }

    fn set_toast(&mut self, msg: &str) {
        self.toast = Some((msg.to_string(), Instant::now()));
    }

    fn screen_to_img_norm(&self, sx: f32, sy: f32) -> Option<(f32, f32)> {
        let r = self.last_img_screen_rect;
        if r.w <= 4.0 || r.h <= 4.0 || !r.contains(sx, sy) {
            return None;
        }
        Some((((sx - r.x) / r.w).clamp(0.0, 1.0), ((sy - r.y) / r.h).clamp(0.0, 1.0)))
    }

    fn effective_zoom(&self) -> f32 {
        if self.zoom > 0.0 {
            return self.zoom;
        }
        if self.preview_w == 0 || self.preview_h == 0 {
            return 1.0;
        }
        let ed_w = if self.edit_mode { EDITOR_W } else { 0.0 };
        let vp_w = (self.width - ed_w - 48.0).max(100.0);
        let strip_h = if !self.edit_mode && self.show_filmstrip && self.siblings.len() > 1 {
            FILMSTRIP_H
        } else {
            16.0
        };
        let vp_h = (self.height - TOP_BAR_H - strip_h - 28.0).max(100.0);
        (vp_w / self.preview_w as f32).min(vp_h / self.preview_h as f32).min(1.0)
    }

    fn update_slider(&mut self, kind: SliderKind, r: Rect, mx: f32) {
        let pct = ((mx - r.x) / r.w.max(1.0)).clamp(0.0, 1.0);
        let val = (pct * 200.0 - 100.0).round();
        match kind {
            SliderKind::Brightness => self.params.brightness = val,
            SliderKind::Contrast => self.params.contrast = val,
            SliderKind::Saturation => self.params.saturation = val,
            SliderKind::Warmth => self.params.warmth = val,
        }
        self.preview_dirty = true;
    }

    fn execute(&mut self, act: PhotoAct) {
        match act {
            PhotoAct::WinClose => unsafe {
                let _ = ShowWindow(self.hwnd, SW_HIDE);
                PostQuitMessage(0);
                return;
            },
            PhotoAct::WinMin => unsafe {
                let _ = ShowWindow(self.hwnd, SW_MINIMIZE);
            },
            PhotoAct::WinMax => unsafe {
                self.maximized = !self.maximized;
                let _ = ShowWindow(
                    self.hwnd,
                    if self.maximized { SW_MAXIMIZE } else { SW_RESTORE },
                );
            },
            PhotoAct::OpenDialog => {
                if let Some(path) = win32_shell::pick_file_dialog(
                    "Abrir imagen en AuraPhotos Studio",
                    "Imágenes (*.png;*.jpg;*.jpeg;*.webp;*.bmp;*.gif)",
                    "*.png;*.jpg;*.jpeg;*.webp;*.bmp;*.gif;*.ico",
                ) {
                    self.open_file(Path::new(&path));
                }
            }
            PhotoAct::Prev => {
                if !self.siblings.is_empty() {
                    self.cur_idx = if self.cur_idx == 0 {
                        self.siblings.len() - 1
                    } else {
                        self.cur_idx - 1
                    };
                    self.load_current();
                }
            }
            PhotoAct::Next => {
                if !self.siblings.is_empty() {
                    self.cur_idx = (self.cur_idx + 1) % self.siblings.len();
                    self.load_current();
                }
            }
            PhotoAct::SelectIndex(i) => {
                if i < self.siblings.len() && i != self.cur_idx {
                    self.cur_idx = i;
                    self.load_current();
                }
            }
            PhotoAct::ZoomIn => {
                let base = self.effective_zoom();
                self.zoom = (base * 1.25).clamp(0.12, 8.0);
            }
            PhotoAct::ZoomOut => {
                let base = self.effective_zoom();
                self.zoom = (base / 1.25).clamp(0.12, 8.0);
            }
            PhotoAct::ZoomFit => {
                self.zoom = 0.0;
                self.pan_x = 0.0;
                self.pan_y = 0.0;
            }
            PhotoAct::Zoom100 => {
                self.zoom = 1.0;
                self.pan_x = 0.0;
                self.pan_y = 0.0;
            }
            PhotoAct::RotateCw => {
                if let Some(a) = &self.base_asset {
                    self.base_asset = Some(rotate_bgra_90cw(a));
                    self.preview_dirty = true;
                }
            }
            PhotoAct::FlipH => {
                if let Some(a) = &self.base_asset {
                    self.base_asset = Some(flip_bgra(a, true));
                    self.preview_dirty = true;
                    self.set_toast("Volteada horizontalmente");
                }
            }
            PhotoAct::FlipV => {
                if let Some(a) = &self.base_asset {
                    self.base_asset = Some(flip_bgra(a, false));
                    self.preview_dirty = true;
                    self.set_toast("Volteada verticalmente");
                }
            }
            PhotoAct::SetWallpaper => {
                if let Some(p) = self.current_path().cloned() {
                    let mut st = load_settings();
                    st.wallpaper_id = format!("image:{}", p.to_string_lossy());
                    save_settings(&st);
                    win32_shell::notify_shell_settings_changed();
                    self.set_toast("Fondo aplicado en vivo en AuraOS ✓");
                }
            }
            PhotoAct::OpenInAuraFiles => {
                if let Some(p) = self.current_path() {
                    if let Some(parent) = p.parent() {
                        win32_shell::launch_aurafiles(Some(&parent.to_string_lossy()));
                    }
                }
            }
            PhotoAct::DeleteRecycle => {
                if let Some(p) = self.current_path().cloned() {
                    if win32_shell::move_to_recycle_bin(&p.to_string_lossy()) {
                        self.siblings.remove(self.cur_idx);
                        if self.cur_idx >= self.siblings.len() && !self.siblings.is_empty() {
                            self.cur_idx = self.siblings.len() - 1;
                        }
                        self.load_current();
                        self.set_toast("Movido a la Papelera");
                    }
                }
            }
            PhotoAct::ToggleFilmstrip => {
                self.show_filmstrip = !self.show_filmstrip;
            }
            PhotoAct::ToggleEditor => {
                self.edit_mode = !self.edit_mode;
                if !self.edit_mode {
                    self.tool = EditTool::PanZoom;
                }
            }
            PhotoAct::SetEditTool(t) => {
                self.tool = t;
            }
            PhotoAct::SetColorSwatch(i) => {
                self.swatch_idx = i % SWATCHES.len();
            }
            PhotoAct::SetCropPreset(aspect) => {
                self.tool = EditTool::Crop;
                if let (Some((aw, ah)), Some(ref a)) = (aspect, &self.base_asset) {
                    let img_ar = a.width.max(1) as f32 / a.height.max(1) as f32;
                    let target_ar = aw / ah;
                    let (nw, nh) = if target_ar > img_ar {
                        (0.86, (0.86 * img_ar / target_ar).clamp(0.1, 0.86))
                    } else {
                        ((0.86 * target_ar / img_ar).clamp(0.1, 0.86), 0.86)
                    };
                    self.crop_norm = Rect::new((1.0 - nw) * 0.5, (1.0 - nh) * 0.5, nw, nh);
                } else {
                    self.crop_norm = Rect::new(0.06, 0.06, 0.88, 0.88);
                }
            }
            PhotoAct::ApplyCrop => {
                if let Some(ref a) = self.base_asset {
                    let cropped = crop_asset(a, self.crop_norm);
                    self.base_asset = Some(cropped);
                    self.crop_norm = Rect::new(0.05, 0.05, 0.90, 0.90);
                    self.tool = EditTool::PanZoom;
                    self.preview_dirty = true;
                    self.set_toast("Recorte aplicado ✓");
                }
            }
            PhotoAct::PresetAuto => {
                self.params = ColorParams {
                    brightness: 8.0,
                    contrast: 18.0,
                    saturation: 16.0,
                    warmth: 6.0,
                    sepia: false,
                };
                self.preview_dirty = true;
                self.set_toast("Auto Mejora aplicada");
            }
            PhotoAct::PresetBW => {
                self.params.saturation = -100.0;
                self.params.contrast = 20.0;
                self.params.sepia = false;
                self.preview_dirty = true;
                self.set_toast("Filtro Blanco y Negro");
            }
            PhotoAct::PresetSepia => {
                self.params.sepia = !self.params.sepia;
                self.preview_dirty = true;
            }
            PhotoAct::ResetEdits => {
                self.load_current();
                self.set_toast("Imagen restaurada al original");
            }
            PhotoAct::UndoMark => {
                if self.marks.pop().is_some() {
                    self.preview_dirty = true;
                }
            }
            PhotoAct::CopyClipboard => {
                self.rebuild_preview_if_needed();
                if self.preview_w > 0
                    && win32_shell::copy_bgra_to_clipboard(
                        self.preview_w,
                        self.preview_h,
                        &self.preview_bgra,
                    )
                {
                    self.set_toast("Imagen editada copiada al portapapeles ✓");
                }
            }
            PhotoAct::SaveCopyDialog => {
                self.rebuild_preview_if_needed();
                if self.preview_w == 0 {
                    return;
                }
                let def_name = self
                    .current_path()
                    .and_then(|p| p.file_stem())
                    .map(|s| format!("{}_editada.png", s.to_string_lossy()))
                    .unwrap_or_else(|| "AuraPhotos_Editada.png".into());
                if let Some(out) = win32_shell::save_file_dialog(
                    "Guardar imagen editada",
                    "Imagen PNG / JPEG (*.png;*.jpg)",
                    "*.png;*.jpg;*.jpeg",
                    "png",
                    &def_name,
                ) {
                    match save_edited_image(
                        Path::new(&out),
                        self.preview_w,
                        self.preview_h,
                        &self.preview_bgra,
                    ) {
                        Ok(()) => self.set_toast("Imagen guardada correctamente ✓"),
                        Err(e) => self.set_toast(&e),
                    }
                }
            }
        }
        self.redraw();
    }

    fn redraw(&mut self) {
        self.rebuild_preview_if_needed();
        if let Some((_, ts)) = &self.toast {
            if ts.elapsed().as_secs_f32() > 3.2 {
                self.toast = None;
            }
        }
        let w = self.width;
        let h = self.height;
        let maxd = self.maximized;
        let sibs = &self.siblings;
        let idx = self.cur_idx;
        let pw = self.preview_w;
        let ph = self.preview_h;
        let pbgra = &self.preview_bgra;
        let rev = self.asset_rev;
        let fb = self.file_bytes;
        let zoom = self.zoom;
        let px = self.pan_x;
        let py = self.pan_y;
        let strip = self.show_filmstrip;
        let ed = self.edit_mode;
        let tool = self.tool;
        let sw = self.swatch_idx;
        let params = &self.params;
        let crop = self.crop_norm;
        let stroke = &self.active_stroke;
        let dshape = self.drag_shape;
        let hov = self.hovered_id.as_deref();
        let toast = self.toast.as_ref().map(|(s, _)| s.as_str());
        let img_rect = &mut self.last_img_screen_rect;
        let hits = &mut self.hits;

        self.surf.draw(&mut self.gpu, |ctx| {
            render_photos_ui(
                ctx, w, h, maxd, sibs, idx, pw, ph, pbgra, rev, fb, zoom, px, py, strip, ed, tool,
                sw, params, crop, stroke, dshape, hov, toast, img_rect, hits,
            );
        });

        let nodes: Vec<(String, String, f32, f32, f32, f32)> = self
            .hits
            .iter()
            .map(|h| (h.id.clone(), h.id.clone(), h.rect.x, h.rect.y, h.rect.w, h.rect.h))
            .collect();
        agent_bridge::publish_ui_tree("AuraPhotos", self.hwnd, &nodes);
    }
}

#[allow(clippy::too_many_arguments)]
fn render_photos_ui(
    ctx: &mut DrawCtx<'_>,
    w: f32,
    h: f32,
    maximized: bool,
    siblings: &[PathBuf],
    cur_idx: usize,
    img_w: u32,
    img_h: u32,
    img_bgra: &[u8],
    asset_rev: u64,
    file_bytes: u64,
    zoom: f32,
    pan_x: f32,
    pan_y: f32,
    show_filmstrip: bool,
    edit_mode: bool,
    tool: EditTool,
    swatch_idx: usize,
    params: &ColorParams,
    crop_norm: Rect,
    active_stroke: &[(f32, f32)],
    drag_shape: Option<((f32, f32), (f32, f32))>,
    hovered_id: Option<&str>,
    toast: Option<&str>,
    out_img_rect: &mut Rect,
    hits: &mut Vec<Hit>,
) {
    hits.clear();
    let corner = if maximized { 0.0 } else { 18.0 };
    let bounds = Rect::new(0.0, 0.0, w, h);
    let _clip = if corner > 0.0 {
        ctx.push_rrect_mask_layer(bounds, bounds, corner, 1.0)
    } else {
        None
    };

    ctx.fill_rrect(bounds, corner, theme::CANVAS);

    // Top Header Bar
    ctx.fill_rrect(Rect::new(0.0, 0.0, w, TOP_BAR_H), 0.0, theme::SURFACE);
    ctx.draw_line(0.0, TOP_BAR_H, w, TOP_BAR_H, theme::BORDER, 1.0);

    let lights = [
        ("ph_close", 20.0, theme::CORAL, "×", PhotoAct::WinClose),
        ("ph_min", 40.0, theme::AMBER, "−", PhotoAct::WinMin),
        ("ph_max", 60.0, theme::EMERALD, "+", PhotoAct::WinMax),
    ];
    let any_light_hov = matches!(hovered_id, Some("ph_close") | Some("ph_min") | Some("ph_max"));
    for (id, cx, col, glyph, act) in lights {
        ctx.fill_circle(cx, 24.0, if any_light_hov { 6.3 } else { 5.8 }, col);
        if any_light_hov {
            ctx.draw_text_center(glyph, Rect::new(cx - 6.0, 17.0, 12.0, 13.0), TextStyle::Small9Reg, theme::DARK_INK);
        }
        hits.push(Hit { id: id.into(), rect: Rect::new(cx - 8.0, 14.0, 16.0, 20.0), act, slider: None });
    }

    let title_str = siblings
        .get(cur_idx)
        .and_then(|p| p.file_name())
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "AuraPhotos Studio".to_string());
    ctx.draw_vector_icon(VectorIcon::Image, 92.0, 24.0, 13.0, theme::VIOLET_LIGHT);
    ctx.draw_text(&title_str, Rect::new(106.0, 13.0, 210.0, 22.0), TextStyle::Body12Semi, theme::TEXT_PRIMARY);

    if img_w > 0 {
        let meta = format!(
            "{} / {} · {}×{} · {}",
            cur_idx + 1,
            siblings.len().max(1),
            img_w,
            img_h,
            format_bytes(file_bytes)
        );
        let meta_r = Rect::new(322.0, 13.0, 226.0, 22.0);
        ctx.fill_rrect(meta_r, 8.0, theme::ELEVATED);
        ctx.draw_text_center(&meta, meta_r, TextStyle::Mono11Med, theme::TEXT_SECONDARY);
    }

    // Top-Right Action Toolbar Pills
    let mut rx = w - 14.0;
    let action_defs: [(&str, &str, Option<VectorIcon>, PhotoAct, bool); 8] = [
        ("act_del", "", Some(VectorIcon::Trash), PhotoAct::DeleteRecycle, false),
        ("act_wp", "Usar fondo", None, PhotoAct::SetWallpaper, false),
        ("act_edit", if edit_mode { "Cerrar Editor ✓" } else { "Editar Studio" }, None, PhotoAct::ToggleEditor, true),
        ("act_rot", "", Some(VectorIcon::RotateCw), PhotoAct::RotateCw, false),
        ("act_fit", "Ajustar", None, PhotoAct::ZoomFit, false),
        ("act_zplus", "", Some(VectorIcon::ZoomIn), PhotoAct::ZoomIn, false),
        ("act_zminus", "", Some(VectorIcon::ZoomOut), PhotoAct::ZoomOut, false),
        ("act_open", "Abrir...", None, PhotoAct::OpenDialog, false),
    ];
    for (id, lbl, ic, act, accent) in action_defs {
        let bw = if lbl.is_empty() { 30.0 } else { (lbl.len() as f32 * 6.9 + 22.0).max(68.0) };
        rx -= bw;
        let br = Rect::new(rx, 10.0, bw, 28.0);
        let hov = hovered_id == Some(id);
        let bg = if accent {
            if hov { theme::VIOLET_LIGHT } else { theme::VIOLET }
        } else if hov {
            theme::ELEVATED_HOVER
        } else {
            theme::ELEVATED
        };
        ctx.fill_card(br, 9.0, bg, if accent { theme::VIOLET_BORDER } else { theme::BORDER });
        if let Some(icon) = ic {
            let ic_col = if id == "act_del" && hov { theme::CORAL } else { theme::TEXT_PRIMARY };
            ctx.draw_vector_icon(icon, br.x + br.w * 0.5, br.y + 14.0, 12.0, ic_col);
        } else {
            ctx.draw_text_center(lbl, br, TextStyle::Body11Med, theme::CRISP_WHITE);
        }
        hits.push(Hit { id: id.into(), rect: br, act, slider: None });
        rx -= 6.0;
    }

    // ==================== RIGHT STUDIO EDITOR PANEL ====================
    let ed_w = if edit_mode { EDITOR_W } else { 0.0 };
    if edit_mode {
        let ep = Rect::new(w - EDITOR_W, TOP_BAR_H, EDITOR_W, h - TOP_BAR_H);
        ctx.fill_rrect(ep, 0.0, theme::SURFACE);
        ctx.draw_line(ep.x, TOP_BAR_H, ep.x, h, theme::BORDER, 1.0);

        let lx = ep.x + 14.0;
        let iw = EDITOR_W - 28.0;
        let mut ey = TOP_BAR_H + 14.0;

        ctx.draw_text("AJUSTES DE COLOR GPU", Rect::new(lx, ey, iw, 16.0), TextStyle::Small10Bold, theme::TEXT_TERTIARY);
        ey += 20.0;

        let sliders = [
            ("sl_br", "Brillo", params.brightness, SliderKind::Brightness),
            ("sl_ct", "Contraste", params.contrast, SliderKind::Contrast),
            ("sl_st", "Saturación", params.saturation, SliderKind::Saturation),
            ("sl_wm", "Calidez", params.warmth, SliderKind::Warmth),
        ];
        for (sid, name, val, skind) in sliders {
            ctx.draw_text(name, Rect::new(lx, ey, 140.0, 16.0), TextStyle::Body11Med, theme::TEXT_PRIMARY);
            ctx.draw_text_right(&format!("{:+.0}", val), Rect::new(lx + iw - 60.0, ey, 60.0, 16.0), TextStyle::Mono11Med, theme::VIOLET_LIGHT);
            ey += 18.0;
            let tr = Rect::new(lx, ey, iw, 12.0);
            ctx.fill_rrect(Rect::new(tr.x, tr.y + 3.0, tr.w, 6.0), 3.0, theme::ELEVATED);
            let norm = ((val + 100.0) / 200.0).clamp(0.0, 1.0);
            let kx = tr.x + norm * tr.w;
            let mid = tr.x + tr.w * 0.5;
            ctx.fill_rrect(
                Rect::new(mid.min(kx), tr.y + 3.0, (kx - mid).abs().max(2.0), 6.0),
                3.0,
                theme::VIOLET_LIGHT,
            );
            ctx.fill_circle(kx, tr.y + 6.0, 6.0, theme::CRISP_WHITE);
            hits.push(Hit {
                id: sid.into(),
                rect: tr.inset(-4.0, -4.0),
                act: PhotoAct::SetEditTool(tool),
                slider: Some(skind),
            });
            ey += 20.0;
        }

        // Presets Row
        let presets = [
            ("pr_auto", "Auto", PhotoAct::PresetAuto),
            ("pr_bw", "B&N", PhotoAct::PresetBW),
            ("pr_sep", "Sepia", PhotoAct::PresetSepia),
            ("pr_rst", "Reset", PhotoAct::ResetEdits),
        ];
        let pw_btn = (iw - 18.0) / 4.0;
        for (pi, (pid, plbl, pact)) in presets.into_iter().enumerate() {
            let pr = Rect::new(lx + pi as f32 * (pw_btn + 6.0), ey, pw_btn, 26.0);
            ctx.fill_card(pr, 8.0, if hovered_id == Some(pid) { theme::ELEVATED_HOVER } else { theme::ELEVATED }, theme::BORDER);
            ctx.draw_text_center(plbl, pr, TextStyle::Body11Med, theme::TEXT_PRIMARY);
            hits.push(Hit { id: pid.into(), rect: pr, act: pact, slider: None });
        }
        ey += 38.0;

        // Transform & Crop Section
        ctx.draw_text("TRANSFORMAR Y RECORTAR", Rect::new(lx, ey, iw, 16.0), TextStyle::Small10Bold, theme::TEXT_TERTIARY);
        ey += 20.0;

        let fh_r = Rect::new(lx, ey, (iw - 6.0) * 0.5, 28.0);
        let fv_r = Rect::new(lx + fh_r.w + 6.0, ey, (iw - 6.0) * 0.5, 28.0);
        ctx.fill_card(fh_r, 8.0, theme::ELEVATED, theme::BORDER);
        ctx.draw_text_center("Voltear H", fh_r, TextStyle::Body11Med, theme::TEXT_PRIMARY);
        hits.push(Hit { id: "ed_fh".into(), rect: fh_r, act: PhotoAct::FlipH, slider: None });

        ctx.fill_card(fv_r, 8.0, theme::ELEVATED, theme::BORDER);
        ctx.draw_text_center("Voltear V", fv_r, TextStyle::Body11Med, theme::TEXT_PRIMARY);
        hits.push(Hit { id: "ed_fv".into(), rect: fv_r, act: PhotoAct::FlipV, slider: None });
        ey += 34.0;

        let crops: [(&str, &str, Option<(f32, f32)>); 4] = [
            ("cp_free", "Libre", None),
            ("cp_11", "1:1", Some((1.0, 1.0))),
            ("cp_169", "16:9", Some((16.0, 9.0))),
            ("cp_916", "9:16", Some((9.0, 16.0))),
        ];
        let cw_btn = (iw - 18.0) / 4.0;
        for (ci, (cid, clbl, ar)) in crops.into_iter().enumerate() {
            let cr = Rect::new(lx + ci as f32 * (cw_btn + 6.0), ey, cw_btn, 26.0);
            let actv = tool == EditTool::Crop && cid == "cp_free";
            ctx.fill_card(
                cr,
                8.0,
                if actv { theme::VIOLET_TINT } else { theme::ELEVATED },
                if actv { theme::VIOLET_LIGHT } else { theme::BORDER },
            );
            ctx.draw_text_center(clbl, cr, TextStyle::Body11Med, theme::TEXT_PRIMARY);
            hits.push(Hit { id: cid.into(), rect: cr, act: PhotoAct::SetCropPreset(ar), slider: None });
        }
        ey += 32.0;

        if tool == EditTool::Crop {
            let ap_r = Rect::new(lx, ey, iw, 30.0);
            ctx.fill_card(ap_r, 9.0, theme::EMERALD, theme::BORDER);
            ctx.draw_text_center("Aplicar recorte ✓", ap_r, TextStyle::Body11Med, theme::DARK_INK);
            hits.push(Hit { id: "cp_apply".into(), rect: ap_r, act: PhotoAct::ApplyCrop, slider: None });
            ey += 38.0;
        }

        // Markup & Privacy Tools
        ctx.draw_text("ANOTAR Y CENSURAR", Rect::new(lx, ey, iw, 16.0), TextStyle::Small10Bold, theme::TEXT_TERTIARY);
        ey += 20.0;

        let mtools = [
            ("mt_br", "Pincel", EditTool::Brush),
            ("mt_ar", "Flecha", EditTool::Arrow),
            ("mt_rc", "Cuadro", EditTool::RectBox),
            ("mt_hl", "Resaltar", EditTool::Highlighter),
            ("mt_px", "Pixelar", EditTool::Pixelate),
            ("mt_pz", "Mover", EditTool::PanZoom),
        ];
        let mw_btn = (iw - 12.0) / 3.0;
        for (mi, (mid, mlbl, mt)) in mtools.into_iter().enumerate() {
            let col_i = mi % 3;
            let row_i = mi / 3;
            let mr = Rect::new(lx + col_i as f32 * (mw_btn + 6.0), ey + row_i as f32 * 32.0, mw_btn, 27.0);
            let actv = tool == mt;
            ctx.fill_card(
                mr,
                8.0,
                if actv { theme::VIOLET_TINT } else { theme::ELEVATED },
                if actv { theme::VIOLET_LIGHT } else { theme::BORDER },
            );
            ctx.draw_text_center(mlbl, mr, TextStyle::Body11Med, if actv { theme::VIOLET_LIGHT } else { theme::TEXT_PRIMARY });
            hits.push(Hit { id: mid.into(), rect: mr, act: PhotoAct::SetEditTool(mt), slider: None });
        }
        ey += 70.0;

        // Swatches + Undo
        for (si, rgb_c) in SWATCHES.iter().enumerate() {
            let cx = lx + 12.0 + si as f32 * 26.0;
            let cy = ey + 12.0;
            ctx.fill_circle(cx, cy, 8.0, theme::rgb(rgb_c[0], rgb_c[1], rgb_c[2]));
            if swatch_idx == si {
                ctx.stroke_circle(cx, cy, 10.5, theme::CRISP_WHITE, 1.8);
            }
            hits.push(Hit {
                id: format!("sw_{}", si),
                rect: Rect::new(cx - 11.0, cy - 11.0, 22.0, 22.0),
                act: PhotoAct::SetColorSwatch(si),
                slider: None,
            });
        }
        let un_r = Rect::new(lx + iw - 86.0, ey, 86.0, 26.0);
        ctx.fill_card(un_r, 8.0, theme::ELEVATED, theme::BORDER);
        ctx.draw_text_center("Deshacer", un_r, TextStyle::Body11Med, theme::TEXT_PRIMARY);
        hits.push(Hit { id: "ed_undo".into(), rect: un_r, act: PhotoAct::UndoMark, slider: None });
        ey += 40.0;

        // Export Buttons at Bottom of Inspector
        let cpy_r = Rect::new(lx, ey, iw, 32.0);
        ctx.fill_card(cpy_r, 9.0, theme::ELEVATED_HOVER, theme::BORDER_STRONG);
        ctx.draw_text_center("Copiar imagen (Ctrl+C)", cpy_r, TextStyle::Body11Med, theme::CRISP_WHITE);
        hits.push(Hit { id: "ed_copy".into(), rect: cpy_r, act: PhotoAct::CopyClipboard, slider: None });
        ey += 38.0;

        let sav_r = Rect::new(lx, ey, iw, 34.0);
        ctx.fill_card(sav_r, 10.0, theme::VIOLET, theme::VIOLET_LIGHT);
        ctx.draw_text_center("Guardar copia PNG / JPG...", sav_r, TextStyle::Body12Semi, theme::CRISP_WHITE);
        hits.push(Hit { id: "ed_save".into(), rect: sav_r, act: PhotoAct::SaveCopyDialog, slider: None });
    }

    // ==================== MAIN VIEWPORT ====================
    let strip_h = if !edit_mode && show_filmstrip && siblings.len() > 1 {
        FILMSTRIP_H
    } else {
        0.0
    };
    let vp = Rect::new(0.0, TOP_BAR_H, (w - ed_w).max(160.0), (h - TOP_BAR_H - strip_h).max(120.0));
    ctx.push_clip(vp);

    if img_w > 0 && !img_bgra.is_empty() {
        let fit_scale = ((vp.w - 48.0) / img_w as f32)
            .min((vp.h - 36.0) / img_h as f32)
            .min(1.0);
        let eff_scale = if zoom > 0.0 { zoom } else { fit_scale };
        let dw = img_w as f32 * eff_scale;
        let dh = img_h as f32 * eff_scale;
        let dx = vp.x + (vp.w - dw) * 0.5 + pan_x;
        let dy = vp.y + (vp.h - dh) * 0.5 + pan_y;
        let img_r = Rect::new(dx, dy, dw, dh);
        *out_img_rect = img_r;

        ctx.fill_rrect(Rect::new(dx - 2.0, dy + 2.0, dw + 4.0, dh + 4.0), 6.0, theme::rgba(0, 0, 0, 0.45));
        let key = format!("photo_live_{}", asset_rev);
        ctx.draw_bgra_alpha_live(&key, img_w, img_h, img_bgra, img_r);

        // Interactive Crop Overlay
        if edit_mode && tool == EditTool::Crop {
            let cr = Rect::new(
                img_r.x + crop_norm.x * img_r.w,
                img_r.y + crop_norm.y * img_r.h,
                crop_norm.w * img_r.w,
                crop_norm.h * img_r.h,
            );
            ctx.stroke_rrect(cr, 2.0, theme::VIOLET_LIGHT, 2.2);
            // Rule of thirds guide lines
            ctx.draw_line(cr.x + cr.w / 3.0, cr.y, cr.x + cr.w / 3.0, cr.y + cr.h, theme::rgba(255, 255, 255, 0.35), 1.0);
            ctx.draw_line(cr.x + cr.w * 2.0 / 3.0, cr.y, cr.x + cr.w * 2.0 / 3.0, cr.y + cr.h, theme::rgba(255, 255, 255, 0.35), 1.0);
            ctx.draw_line(cr.x, cr.y + cr.h / 3.0, cr.x + cr.w, cr.y + cr.h / 3.0, theme::rgba(255, 255, 255, 0.35), 1.0);
            ctx.draw_line(cr.x, cr.y + cr.h * 2.0 / 3.0, cr.x + cr.w, cr.y + cr.h * 2.0 / 3.0, theme::rgba(255, 255, 255, 0.35), 1.0);
        }

        // Live in-progress stroke / shape
        let c = SWATCHES[swatch_idx % SWATCHES.len()];
        let col = theme::rgb(c[0], c[1], c[2]);
        if active_stroke.len() >= 2 {
            for pair in active_stroke.windows(2) {
                ctx.draw_line(
                    img_r.x + pair[0].0 * img_r.w,
                    img_r.y + pair[0].1 * img_r.h,
                    img_r.x + pair[1].0 * img_r.w,
                    img_r.y + pair[1].1 * img_r.h,
                    col,
                    if tool == EditTool::Highlighter { 16.0 } else { 3.5 },
                );
            }
        }
        if let Some(((x0, y0), (x1, y1))) = drag_shape {
            let sx0 = img_r.x + x0 * img_r.w;
            let sy0 = img_r.y + y0 * img_r.h;
            let sx1 = img_r.x + x1 * img_r.w;
            let sy1 = img_r.y + y1 * img_r.h;
            if tool == EditTool::Arrow {
                ctx.draw_line(sx0, sy0, sx1, sy1, col, 3.0);
            } else {
                ctx.stroke_rrect(
                    Rect::new(sx0.min(sx1), sy0.min(sy1), (sx1 - sx0).abs(), (sy1 - sy0).abs()),
                    4.0,
                    col,
                    2.5,
                );
            }
        }

        let zp = Rect::new(vp.x + vp.w - 72.0, vp.y + vp.h - 34.0, 56.0, 24.0);
        ctx.fill_card(zp, 8.0, theme::rgba(20, 20, 28, 0.85), theme::BORDER);
        ctx.draw_text_center(&format!("{}%", (eff_scale * 100.0).round() as i32), zp, TextStyle::Mono11Med, theme::TEXT_SECONDARY);
        hits.push(Hit { id: "act_z100".into(), rect: zp, act: PhotoAct::Zoom100, slider: None });
    } else {
        let card = Rect::new(vp.x + (vp.w - 380.0) * 0.5, vp.y + (vp.h - 176.0) * 0.5, 380.0, 176.0);
        ctx.fill_card(card, 18.0, theme::SURFACE, theme::BORDER);
        ctx.draw_vector_icon(VectorIcon::Image, card.x + 190.0, card.y + 44.0, 28.0, theme::VIOLET_LIGHT);
        ctx.draw_text_center("AuraPhotos Studio v5.0", Rect::new(card.x, card.y + 72.0, card.w, 24.0), TextStyle::Title14Semi, theme::TEXT_PRIMARY);
        ctx.draw_text_center("Visor instantáneo + Editor GPU de color, recorte y anotación", Rect::new(card.x, card.y + 96.0, card.w, 20.0), TextStyle::Body11Reg, theme::TEXT_SECONDARY);
        let btn = Rect::new(card.x + 105.0, card.y + 126.0, 170.0, 32.0);
        ctx.fill_card(btn, 10.0, theme::VIOLET, theme::VIOLET_BORDER);
        ctx.draw_text_center("Seleccionar imagen", btn, TextStyle::Body12Semi, theme::CRISP_WHITE);
        hits.push(Hit { id: "empty_open".into(), rect: btn, act: PhotoAct::OpenDialog, slider: None });
    }

    if !edit_mode && siblings.len() > 1 {
        let nav_y = vp.y + vp.h * 0.5 - 22.0;
        let prev_r = Rect::new(16.0, nav_y, 40.0, 44.0);
        let next_r = Rect::new(vp.w - 56.0, nav_y, 40.0, 44.0);
        ctx.fill_card(prev_r, 12.0, theme::rgba(20, 20, 28, 0.78), theme::BORDER);
        ctx.draw_vector_icon(VectorIcon::ChevronLeft, prev_r.x + 20.0, prev_r.y + 22.0, 15.0, theme::TEXT_PRIMARY);
        hits.push(Hit { id: "nav_prev".into(), rect: prev_r, act: PhotoAct::Prev, slider: None });

        ctx.fill_card(next_r, 12.0, theme::rgba(20, 20, 28, 0.78), theme::BORDER);
        ctx.draw_vector_icon(VectorIcon::ChevronRight, next_r.x + 20.0, next_r.y + 22.0, 15.0, theme::TEXT_PRIMARY);
        hits.push(Hit { id: "nav_next".into(), rect: next_r, act: PhotoAct::Next, slider: None });
    }

    if let Some(msg) = toast {
        let tw = (msg.len() as f32 * 6.8 + 36.0).clamp(200.0, vp.w - 60.0);
        let tr = Rect::new(vp.x + (vp.w - tw) * 0.5, vp.y + 14.0, tw, 32.0);
        ctx.fill_card(tr, 10.0, theme::rgba(22, 34, 28, 0.94), theme::EMERALD);
        ctx.draw_text_center(msg, tr, TextStyle::Body11Med, theme::TEXT_PRIMARY);
    }
    ctx.pop_clip();

    // Bottom Filmstrip (Viewer Mode)
    if strip_h > 0.0 {
        let fy = h - strip_h;
        ctx.fill_rrect(Rect::new(0.0, fy, w, strip_h), 0.0, theme::SURFACE);
        ctx.draw_line(0.0, fy, w, fy, theme::BORDER, 1.0);

        let thumb_w = 72.0;
        let thumb_h = 54.0;
        let gap = 8.0;
        let max_visible = ((w - 32.0) / (thumb_w + gap)).floor().max(1.0) as usize;
        let start = cur_idx.saturating_sub(max_visible / 2);
        let end = (start + max_visible).min(siblings.len());
        let shown = end - start;
        let total_w = shown as f32 * thumb_w + shown.saturating_sub(1) as f32 * gap;
        let mut tx = (w - total_w) * 0.5;
        let ty = fy + (strip_h - thumb_h) * 0.5;

        for i in start..end {
            let tr = Rect::new(tx, ty, thumb_w, thumb_h);
            let active = i == cur_idx;
            let id = format!("thumb_{}", i);
            ctx.fill_card(tr, 8.0, theme::CARD_BG, if active { theme::VIOLET_LIGHT } else { theme::BORDER });
            let inner = tr.inset(2.0, 2.0);
            ctx.push_clip(inner);
            let p_str = siblings[i].to_string_lossy().to_string();
            let _ = ctx.draw_image_file(&p_str, inner, if active { 1.0 } else { 0.78 });
            ctx.pop_clip();
            hits.push(Hit { id, rect: tr, act: PhotoAct::SelectIndex(i), slider: None });
            tx += thumb_w + gap;
        }
    }

    if corner > 0.0 {
        ctx.stroke_rrect(bounds.inset(0.5, 0.5), corner, theme::BORDER, 1.0);
        ctx.pop_layer();
    }
}

unsafe extern "system" fn photos_wnd_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if msg == WM_CLOSE {
        let _ = ShowWindow(hwnd, SW_HIDE);
        PostQuitMessage(0);
        return LRESULT(0);
    }
    if msg == WM_DESTROY {
        PostQuitMessage(0);
        return LRESULT(0);
    }
    if msg == WM_NCCALCSIZE {
        if let Some(res) = aura_window::handle_nccalcsize(hwnd, wparam, lparam) {
            return res;
        }
    }
    if msg == WM_GETMINMAXINFO {
        return aura_window::handle_getminmaxinfo(hwnd, lparam, 680, 440);
    }
    if msg == WM_NCHITTEST {
        return aura_window::handle_nchittest(hwnd, lparam, TOP_BAR_H, |lx, ly| {
            PHOTO_RT.with(|c| {
                c.try_borrow()
                    .ok()
                    .and_then(|g| g.as_ref().map(|r| r.hits.iter().any(|h| h.rect.contains(lx, ly))))
                    .unwrap_or(false)
            })
        });
    }
    if msg == WM_SYSKEYDOWN {
        let vk = wparam.0 as u16;
        if vk == VK_LEFT.0 { aura_window::snap_window(hwnd, SnapZone::LeftHalf); return LRESULT(0); }
        if vk == VK_RIGHT.0 { aura_window::snap_window(hwnd, SnapZone::RightHalf); return LRESULT(0); }
        if vk == VK_UP.0 { aura_window::snap_window(hwnd, SnapZone::Maximize); return LRESULT(0); }
        if vk == VK_DOWN.0 { aura_window::snap_window(hwnd, SnapZone::Center); return LRESULT(0); }
    }

    let mut start_title_drag = false;
    let mut deferred_max: Option<bool> = None;
    let mut deferred_min = false;
    let mut deferred_snap = false;

    let handled = PHOTO_RT.with(|cell| {
        let Ok(mut opt) = cell.try_borrow_mut() else { return false };
        let Some(rt) = opt.as_mut() else { return false };

        match msg {
            WM_TIMER if wparam.0 == TIMER_ASYNC => {
                if rt.gpu.drain_decoded_assets()
                    || rt.toast.as_ref().map(|(_, t)| t.elapsed().as_secs_f32() > 3.2).unwrap_or(false)
                {
                    rt.redraw();
                }
                true
            }
            WM_SIZE => {
                let mut rc = RECT::default();
                if GetClientRect(hwnd, &mut rc).is_ok() {
                    let nw = (rc.right - rc.left).max(400) as u32;
                    let nh = (rc.bottom - rc.top).max(300) as u32;
                    rt.maximized = IsZoomed(hwnd).as_bool();
                    rt.width = nw as f32;
                    rt.height = nh as f32;
                    rt.surf.resize(&rt.gpu, nw, nh);
                    rt.redraw();
                }
                true
            }
            WM_MOUSEMOVE => {
                let px = (lparam.0 & 0xFFFF) as i16 as f32;
                let py = ((lparam.0 >> 16) & 0xFFFF) as i16 as f32;

                if let Some((sk, sr)) = rt.active_slider {
                    rt.update_slider(sk, sr, px);
                    rt.redraw();
                    return true;
                }
                if !rt.active_stroke.is_empty() {
                    if let Some(pt) = rt.screen_to_img_norm(px, py) {
                        rt.active_stroke.push(pt);
                        rt.redraw();
                    }
                    return true;
                }
                if let Some((s0, _)) = rt.drag_shape {
                    if let Some(s1) = rt.screen_to_img_norm(px, py) {
                        if rt.tool == EditTool::Crop {
                            rt.crop_norm = Rect::new(
                                s0.0.min(s1.0),
                                s0.1.min(s1.1),
                                (s1.0 - s0.0).abs().max(0.05),
                                (s1.1 - s0.1).abs().max(0.05),
                            );
                        } else {
                            rt.drag_shape = Some((s0, s1));
                        }
                        rt.redraw();
                    }
                    return true;
                }
                if rt.dragging {
                    rt.pan_x += px - rt.drag_last.0;
                    rt.pan_y += py - rt.drag_last.1;
                    rt.drag_last = (px, py);
                    rt.redraw();
                    return true;
                }

                let new_hov = rt.hits.iter().rev().find(|h| h.rect.contains(px, py)).map(|h| h.id.clone());
                if new_hov != rt.hovered_id {
                    rt.hovered_id = new_hov;
                    rt.redraw();
                }
                true
            }
            WM_LBUTTONDOWN => {
                let px = (lparam.0 & 0xFFFF) as i16 as f32;
                let py = ((lparam.0 >> 16) & 0xFFFF) as i16 as f32;
                if let Some(hit) = rt.hits.iter().rev().find(|h| h.rect.contains(px, py)).cloned() {
                    if let Some(sk) = hit.slider {
                        rt.active_slider = Some((sk, hit.rect));
                        rt.update_slider(sk, hit.rect, px);
                        SetCapture(hwnd);
                        rt.redraw();
                        return true;
                    }
                    match hit.act {
                        PhotoAct::WinMax => {
                            rt.maximized = !rt.maximized;
                            deferred_max = Some(rt.maximized);
                        }
                        PhotoAct::WinMin => {
                            deferred_min = true;
                        }
                        other => rt.execute(other),
                    }
                    return true;
                }
                if py < TOP_BAR_H {
                    start_title_drag = true;
                    return true;
                }
                if rt.edit_mode && rt.tool != EditTool::PanZoom {
                    if let Some(norm) = rt.screen_to_img_norm(px, py) {
                        match rt.tool {
                            EditTool::Brush | EditTool::Highlighter => {
                                rt.active_stroke.clear();
                                rt.active_stroke.push(norm);
                                SetCapture(hwnd);
                            }
                            EditTool::Arrow | EditTool::RectBox | EditTool::Pixelate | EditTool::Crop => {
                                rt.drag_shape = Some((norm, norm));
                                SetCapture(hwnd);
                            }
                            EditTool::PanZoom => {}
                        }
                        return true;
                    }
                }
                if rt.preview_w > 0 {
                    rt.dragging = true;
                    rt.drag_last = (px, py);
                    SetCapture(hwnd);
                }
                true
            }
            WM_LBUTTONUP => {
                if rt.active_slider.take().is_some() {
                    let _ = ReleaseCapture();
                } else if !rt.active_stroke.is_empty() {
                    let pts = std::mem::take(&mut rt.active_stroke);
                    if pts.len() >= 2 {
                        let c = SWATCHES[rt.swatch_idx % SWATCHES.len()];
                        rt.marks.push(ImgMark::Stroke {
                            pts,
                            color: c,
                            highlight: rt.tool == EditTool::Highlighter,
                        });
                        rt.preview_dirty = true;
                    }
                    let _ = ReleaseCapture();
                    rt.redraw();
                } else if let Some(((x0, y0), (x1, y1))) = rt.drag_shape.take() {
                    if rt.tool != EditTool::Crop && (x1 - x0).hypot(y1 - y0) > 0.015 {
                        let c = SWATCHES[rt.swatch_idx % SWATCHES.len()];
                        match rt.tool {
                            EditTool::Arrow => rt.marks.push(ImgMark::Arrow { x0, y0, x1, y1, color: c }),
                            EditTool::RectBox => rt.marks.push(ImgMark::Rect { x0, y0, x1, y1, color: c }),
                            EditTool::Pixelate => rt.marks.push(ImgMark::Pixelate { x0, y0, x1, y1 }),
                            _ => {}
                        }
                        rt.preview_dirty = true;
                    }
                    let _ = ReleaseCapture();
                    rt.redraw();
                } else if rt.dragging {
                    rt.dragging = false;
                    let _ = ReleaseCapture();
                }
                true
            }
            WM_RBUTTONDOWN => {
                let px = (lparam.0 & 0xFFFF) as i16 as f32;
                let py = ((lparam.0 >> 16) & 0xFFFF) as i16 as f32;
                if let Some(hit) = rt.hits.iter().rev().find(|h| h.rect.contains(px, py)) {
                    if matches!(hit.act, PhotoAct::WinMax) {
                        deferred_snap = true;
                    }
                }
                true
            }
            WM_LBUTTONDBLCLK => {
                let py = ((lparam.0 >> 16) & 0xFFFF) as i16 as f32;
                if py < TOP_BAR_H {
                    rt.maximized = !rt.maximized;
                    deferred_max = Some(rt.maximized);
                } else if !rt.edit_mode {
                    if rt.zoom == 0.0 {
                        rt.execute(PhotoAct::Zoom100);
                    } else {
                        rt.execute(PhotoAct::ZoomFit);
                    }
                }
                true
            }
            WM_MOUSEWHEEL => {
                let delta = ((wparam.0 >> 16) as i16) as f32;
                if delta > 0.0 {
                    rt.execute(PhotoAct::ZoomIn);
                } else {
                    rt.execute(PhotoAct::ZoomOut);
                }
                true
            }
            WM_KEYDOWN => {
                let vk = wparam.0 as u16;
                if vk == VK_LEFT.0 && !rt.edit_mode {
                    rt.execute(PhotoAct::Prev);
                } else if vk == VK_RIGHT.0 && !rt.edit_mode {
                    rt.execute(PhotoAct::Next);
                } else if vk == b'E' as u16 {
                    rt.execute(PhotoAct::ToggleEditor);
                } else if vk == b'F' as u16 && !rt.edit_mode {
                    rt.execute(PhotoAct::ToggleFilmstrip);
                } else if vk == b'O' as u16 {
                    rt.execute(PhotoAct::OpenInAuraFiles);
                } else if vk == VK_OEM_PLUS.0 {
                    rt.execute(PhotoAct::ZoomIn);
                } else if vk == VK_OEM_MINUS.0 {
                    rt.execute(PhotoAct::ZoomOut);
                } else if vk == b'R' as u16 {
                    rt.execute(PhotoAct::RotateCw);
                } else if vk == VK_DELETE.0 {
                    rt.execute(PhotoAct::DeleteRecycle);
                } else if vk == VK_ESCAPE.0 {
                    if rt.edit_mode {
                        rt.execute(PhotoAct::ToggleEditor);
                    } else {
                        rt.execute(PhotoAct::WinClose);
                    }
                }
                true
            }
            _ => false,
        }
    });

    if start_title_drag {
        let _ = ReleaseCapture();
        let _ = SendMessageW(hwnd, WM_NCLBUTTONDOWN, WPARAM(HTCAPTION as usize), LPARAM(0));
    } else if deferred_min {
        if !win32_shell::request_genie_minimize(hwnd) {
            let _ = ShowWindow(hwnd, SW_MINIMIZE);
        }
    } else if deferred_snap {
        aura_window::cycle_snap_window(hwnd);
    } else if let Some(max) = deferred_max {
        let _ = ShowWindow(hwnd, if max { SW_MAXIMIZE } else { SW_RESTORE });
    }

    if handled {
        LRESULT(0)
    } else {
        DefWindowProcW(hwnd, msg, wparam, lparam)
    }
}
