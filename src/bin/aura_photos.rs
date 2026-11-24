//! AuraPhotos v4.0 — Pure Rust + Direct2D / DirectComposition Instant Image Viewer
//! - Launches in <40ms, replaces slow Windows Photos with Drool Dark Mode precision
//! - Full-res GPU WIC decoding, smooth zoom & drag-pan, 90° rotation, folder filmstrip navigation,
//!   1-click "Usar como fondo" for AuraOS, Recycle Bin delete, and instant clean exit!

#![windows_subsystem = "windows"]

use aura_os::models::{load_settings, save_settings};
use aura_os::aura_window::{self, SnapZone};
use aura_os::renderer::{DecodedAsset, DrawCtx, GpuEngine, Rect, TextStyle, WindowSurface};
use aura_os::theme::{self, VectorIcon};
use aura_os::win32_shell;
use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::time::Instant;
use windows::core::PCWSTR;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    ReleaseCapture, SetCapture, VK_DELETE, VK_ESCAPE, VK_LEFT, VK_OEM_MINUS, VK_OEM_PLUS, VK_RIGHT,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, GetClientRect, GetMessageW,
    IsZoomed, LoadCursorW, PostQuitMessage, RegisterClassW, SendMessageW, SetTimer, ShowWindow,
    TranslateMessage, HTCAPTION, IDC_ARROW, MSG, SW_HIDE, SW_MAXIMIZE, SW_MINIMIZE, SW_RESTORE,
    SW_SHOW, WM_CLOSE, WM_DESTROY, WM_GETMINMAXINFO, WM_KEYDOWN, WM_LBUTTONDBLCLK, WM_LBUTTONDOWN,
    WM_LBUTTONUP, WM_MOUSEMOVE, WM_MOUSEWHEEL, WM_NCCALCSIZE, WM_NCHITTEST, WM_NCLBUTTONDOWN,
    WM_RBUTTONDOWN, WM_SIZE, WM_SYSKEYDOWN, WM_TIMER, WNDCLASSW, WS_EX_NOREDIRECTIONBITMAP,
};

const TIMER_ASYNC: usize = 20;
const TOP_BAR_H: f32 = 46.0;
const FILMSTRIP_H: f32 = 76.0;

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
    SetWallpaper,
    OpenInAuraFiles,
    DeleteRecycle,
    SelectIndex(usize),
    ToggleFilmstrip,
}

#[derive(Clone)]
struct Hit {
    id: String,
    rect: Rect,
    act: PhotoAct,
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
    current_asset: Option<DecodedAsset>,
    asset_rev: u64,
    orig_w: u32,
    orig_h: u32,
    file_bytes: u64,
    rotation_steps: u8, // 0, 1, 2, 3

    zoom: f32, // 0.0 = auto-fit!
    pan_x: f32,
    pan_y: f32,
    dragging: bool,
    drag_last: (f32, f32),

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
            let src_i = (y * w + x) * 4;
            let nx = h - 1 - y;
            let ny = x;
            let dst_i = (ny * h + nx) * 4;
            out[dst_i..dst_i + 4].copy_from_slice(&asset.bgra[src_i..src_i + 4]);
        }
    }
    DecodedAsset {
        width: asset.height,
        height: asset.width,
        bgra: out,
    }
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

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() >= 3 && args[1].eq_ignore_ascii_case("--capture") {
        let out_path = PathBuf::from(&args[2]);
        let sample = args.get(3).map(PathBuf::from).or_else(find_sample_image);
        run_capture(&out_path, sample);
        return;
    }

    let initial_file = args.get(1).map(PathBuf::from).filter(|p| p.exists());

    let mut gpu = match GpuEngine::new() {
        Ok(g) => g,
        Err(_) => return,
    };
    gpu.enable_async_loader();

    let (sw, sh) = win32_shell::screen_size();
    let w = (sw as f32 * 0.72).clamp(960.0, 1380.0) as i32;
    let h = (sh as f32 * 0.78).clamp(640.0, 880.0) as i32;
    let x = (sw - w) / 2;
    let y = (sh - h) / 2;

    let cls = win32_shell::to_wide("AuraPhotos_Rust_v4_Wnd");
    let title = win32_shell::to_wide("AuraPhotos");
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
        current_asset: None,
        asset_rev: 0,
        orig_w: 0,
        orig_h: 0,
        file_bytes: 0,
        rotation_steps: 0,
        zoom: 0.0,
        pan_x: 0.0,
        pan_y: 0.0,
        dragging: false,
        drag_last: (0.0, 0.0),
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
        SetTimer(hwnd, TIMER_ASYNC, 33, None);
    }

    PHOTO_RT.with(|c| *c.borrow_mut() = Some(rt));

    unsafe {
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, HWND::default(), 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
    // Clean deterministic GPU/COM teardown BEFORE process exit (prevents KERNELBASE 0x0000087a!)
    PHOTO_RT.with(|c| {
        if let Ok(mut g) = c.try_borrow_mut() {
            *g = None;
        }
    });
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

fn run_capture(out_path: &Path, sample: Option<PathBuf>) {
    let Ok(mut gpu) = GpuEngine::new() else { return };
    let (siblings, cur_idx) = sample
        .as_deref()
        .map(scan_sibling_images)
        .unwrap_or_default();
    let cur_asset = siblings
        .get(cur_idx)
        .and_then(|p| gpu.decode_image_full(&p.to_string_lossy(), 2400));
    let (ow, oh) = cur_asset.as_ref().map(|a| (a.width, a.height)).unwrap_or((0, 0));
    let fbytes = siblings
        .get(cur_idx)
        .and_then(|p| std::fs::metadata(p).ok().map(|m| m.len()))
        .unwrap_or(0);

    let _ = gpu.render_to_png(1240, 780, out_path, |ctx| {
        let mut hits = Vec::new();
        render_photos_ui(
            ctx,
            1240.0,
            780.0,
            false,
            &siblings,
            cur_idx,
            cur_asset.as_ref(),
            1,
            ow,
            oh,
            fbytes,
            0.0,
            0.0,
            0.0,
            true,
            None,
            None,
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
        // Prewarm nearby thumbnails asynchronously for instant filmstrip
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
        self.rotation_steps = 0;
        let Some(p) = self.siblings.get(self.cur_idx) else {
            self.current_asset = None;
            return;
        };
        self.file_bytes = std::fs::metadata(p).map(|m| m.len()).unwrap_or(0);
        if let Some(a) = self.gpu.decode_image_full(&p.to_string_lossy(), 3840) {
            self.orig_w = a.width;
            self.orig_h = a.height;
            self.current_asset = Some(a);
            self.asset_rev = self.asset_rev.wrapping_add(1);
        } else {
            self.current_asset = None;
        }
    }

    fn current_path(&self) -> Option<&PathBuf> {
        self.siblings.get(self.cur_idx)
    }

    fn set_toast(&mut self, msg: &str) {
        self.toast = Some((msg.to_string(), Instant::now()));
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
                    "Abrir imagen en AuraPhotos",
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
                if let Some(a) = &self.current_asset {
                    let rot = rotate_bgra_90cw(a);
                    self.orig_w = rot.width;
                    self.orig_h = rot.height;
                    self.current_asset = Some(rot);
                    self.rotation_steps = (self.rotation_steps + 1) % 4;
                    self.asset_rev = self.asset_rev.wrapping_add(1);
                }
            }
            PhotoAct::SetWallpaper => {
                if let Some(p) = self.current_path().cloned() {
                    let mut st = load_settings();
                    st.wallpaper_id = format!("image:{}", p.to_string_lossy());
                    save_settings(&st);
                    self.set_toast("Fondo guardado en AuraOS (abre Fondos o reinicia AuraShell para aplicar)");
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
        }
        self.redraw();
    }

    fn effective_zoom(&self) -> f32 {
        if self.zoom > 0.0 {
            return self.zoom;
        }
        let Some(a) = &self.current_asset else { return 1.0 };
        let vp_w = (self.width - 48.0).max(100.0);
        let vp_h = (self.height - TOP_BAR_H - if self.show_filmstrip && self.siblings.len() > 1 { FILMSTRIP_H } else { 16.0 } - 28.0).max(100.0);
        (vp_w / a.width.max(1) as f32).min(vp_h / a.height.max(1) as f32).min(1.0)
    }

    fn redraw(&mut self) {
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
        let asset = self.current_asset.as_ref();
        let rev = self.asset_rev;
        let ow = self.orig_w;
        let oh = self.orig_h;
        let fb = self.file_bytes;
        let zoom = self.zoom;
        let px = self.pan_x;
        let py = self.pan_y;
        let strip = self.show_filmstrip;
        let hov = self.hovered_id.as_deref();
        let toast_txt = self.toast.as_ref().map(|(s, _)| s.as_str());
        let hits = &mut self.hits;

        self.surf.draw(&mut self.gpu, |ctx| {
            render_photos_ui(
                ctx, w, h, maxd, sibs, idx, asset, rev, ow, oh, fb, zoom, px, py, strip, hov,
                toast_txt, hits,
            );
        });
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
    asset: Option<&DecodedAsset>,
    asset_rev: u64,
    orig_w: u32,
    orig_h: u32,
    file_bytes: u64,
    zoom: f32,
    pan_x: f32,
    pan_y: f32,
    show_filmstrip: bool,
    hovered_id: Option<&str>,
    toast: Option<&str>,
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

    // Drool Matte Obsidian Background
    ctx.fill_rrect(bounds, corner, theme::CANVAS);

    // Top Header Bar
    ctx.fill_rrect(Rect::new(0.0, 0.0, w, TOP_BAR_H), 0.0, theme::SURFACE);
    ctx.draw_line(0.0, TOP_BAR_H, w, TOP_BAR_H, theme::BORDER, 1.0);

    // macOS Traffic Lights (with crisp × − + hover glyphs)
    let lights = [
        ("ph_close", 20.0, theme::CORAL, "×", PhotoAct::WinClose),
        ("ph_min", 40.0, theme::AMBER, "−", PhotoAct::WinMin),
        ("ph_max", 60.0, theme::EMERALD, "+", PhotoAct::WinMax),
    ];
    let any_light_hov = matches!(hovered_id, Some("ph_close") | Some("ph_min") | Some("ph_max"));
    for (id, cx, col, glyph, act) in lights {
        ctx.fill_circle(cx, 23.0, if any_light_hov { 6.3 } else { 5.8 }, col);
        if any_light_hov {
            ctx.draw_text_center(glyph, Rect::new(cx - 6.0, 16.0, 12.0, 13.0), TextStyle::Small9Reg, theme::DARK_INK);
        }
        hits.push(Hit {
            id: id.into(),
            rect: Rect::new(cx - 8.0, 14.0, 16.0, 18.0),
            act,
        });
    }

    // App Title + Current File Name & Counter
    let title_str = siblings
        .get(cur_idx)
        .and_then(|p| p.file_name())
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "AuraPhotos".to_string());
    ctx.draw_vector_icon(VectorIcon::Image, 92.0, 23.0, 13.0, theme::VIOLET_LIGHT);
    ctx.draw_text(
        &title_str,
        Rect::new(106.0, 12.0, (w * 0.30).max(160.0), 22.0),
        TextStyle::Body12Semi,
        theme::TEXT_PRIMARY,
    );

    if !siblings.is_empty() {
        let meta = format!(
            "{} / {}  ·  {}×{}  ·  {}",
            cur_idx + 1,
            siblings.len(),
            orig_w,
            orig_h,
            format_bytes(file_bytes)
        );
        let mx = 106.0 + (w * 0.25).min(260.0);
        let meta_r = Rect::new(mx, 12.0, 240.0, 22.0);
        ctx.fill_rrect(meta_r, 8.0, theme::ELEVATED);
        ctx.draw_text_center(&meta, meta_r, TextStyle::Mono11Med, theme::TEXT_SECONDARY);
    }

    // Top-Right Action Toolbar Pills
    let mut rx = w - 14.0;
    let action_defs: [(&str, &str, Option<VectorIcon>, PhotoAct, bool); 7] = [
        ("act_del", "", Some(VectorIcon::Trash), PhotoAct::DeleteRecycle, false),
        ("act_files", "Carpeta", None, PhotoAct::OpenInAuraFiles, false),
        ("act_wp", "Usar fondo", None, PhotoAct::SetWallpaper, true),
        ("act_rot", "", Some(VectorIcon::RotateCw), PhotoAct::RotateCw, false),
        ("act_fit", "Ajustar", None, PhotoAct::ZoomFit, false),
        ("act_zplus", "", Some(VectorIcon::ZoomIn), PhotoAct::ZoomIn, false),
        ("act_zminus", "", Some(VectorIcon::ZoomOut), PhotoAct::ZoomOut, false),
    ];
    for (id, lbl, ic, act, accent) in action_defs {
        let bw = if lbl.is_empty() { 30.0 } else { (lbl.len() as f32 * 7.2 + 22.0).max(66.0) };
        rx -= bw;
        let br = Rect::new(rx, 9.0, bw, 28.0);
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
            ctx.draw_text_center(lbl, br, TextStyle::Body11Med, theme::TEXT_PRIMARY);
        }
        hits.push(Hit { id: id.into(), rect: br, act });
        rx -= 6.0;
    }

    // Open Image Button
    rx -= 76.0;
    let open_r = Rect::new(rx, 9.0, 76.0, 28.0);
    let open_hov = hovered_id == Some("act_open");
    ctx.fill_card(open_r, 9.0, if open_hov { theme::ELEVATED_HOVER } else { theme::ELEVATED }, theme::BORDER);
    ctx.draw_text_center("Abrir...", open_r, TextStyle::Body11Med, theme::TEXT_PRIMARY);
    hits.push(Hit { id: "act_open".into(), rect: open_r, act: PhotoAct::OpenDialog });

    // ==================== MAIN VIEWPORT ====================
    let strip_h = if show_filmstrip && siblings.len() > 1 { FILMSTRIP_H } else { 0.0 };
    let vp = Rect::new(0.0, TOP_BAR_H, w, (h - TOP_BAR_H - strip_h).max(120.0));
    ctx.push_clip(vp);

    if let Some(a) = asset {
        let fit_scale = ((vp.w - 48.0) / a.width.max(1) as f32)
            .min((vp.h - 36.0) / a.height.max(1) as f32)
            .min(1.0);
        let eff_scale = if zoom > 0.0 { zoom } else { fit_scale };
        let dw = a.width as f32 * eff_scale;
        let dh = a.height as f32 * eff_scale;
        let dx = vp.x + (vp.w - dw) * 0.5 + pan_x;
        let dy = vp.y + (vp.h - dh) * 0.5 + pan_y;
        let img_r = Rect::new(dx, dy, dw, dh);

        // Subtle dark shadow matte under image
        ctx.fill_rrect(
            Rect::new(dx - 1.0, dy - 1.0, dw + 2.0, dh + 2.0),
            6.0,
            theme::rgba(0, 0, 0, 0.45),
        );
        let key = format!("photo_{}", asset_rev);
        ctx.draw_bgra_cached(&key, a, img_r);

        // Zoom percentage pill in bottom-right of viewport
        let z_txt = format!("{}%", (eff_scale * 100.0).round() as i32);
        let zp = Rect::new(w - 72.0, vp.y + vp.h - 34.0, 56.0, 24.0);
        ctx.fill_card(zp, 8.0, theme::rgba(20, 20, 28, 0.85), theme::BORDER);
        ctx.draw_text_center(&z_txt, zp, TextStyle::Mono11Med, theme::TEXT_SECONDARY);
        hits.push(Hit { id: "act_z100".into(), rect: zp, act: PhotoAct::Zoom100 });
    } else {
        // Empty Welcome State
        let card = Rect::new((w - 360.0) * 0.5, vp.y + (vp.h - 170.0) * 0.5, 360.0, 170.0);
        ctx.fill_card(card, 18.0, theme::SURFACE, theme::BORDER);
        ctx.draw_vector_icon(VectorIcon::Image, card.x + 180.0, card.y + 44.0, 28.0, theme::VIOLET_LIGHT);
        ctx.draw_text_center("AuraPhotos · Visor GPU", Rect::new(card.x, card.y + 72.0, card.w, 24.0), TextStyle::Title14Semi, theme::TEXT_PRIMARY);
        ctx.draw_text_center("Abre una imagen PNG, JPG, WebP, GIF o BMP", Rect::new(card.x, card.y + 96.0, card.w, 20.0), TextStyle::Body11Reg, theme::TEXT_SECONDARY);
        let btn = Rect::new(card.x + 95.0, card.y + 124.0, 170.0, 32.0);
        let hov = hovered_id == Some("empty_open");
        ctx.fill_card(btn, 10.0, if hov { theme::VIOLET_LIGHT } else { theme::VIOLET }, theme::VIOLET_BORDER);
        ctx.draw_text_center("Seleccionar imagen", btn, TextStyle::Body12Semi, theme::TEXT_PRIMARY);
        hits.push(Hit { id: "empty_open".into(), rect: btn, act: PhotoAct::OpenDialog });
    }

    // Floating Prev / Next Navigation Buttons when multiple images in folder
    if siblings.len() > 1 {
        let nav_y = vp.y + vp.h * 0.5 - 22.0;
        let prev_r = Rect::new(16.0, nav_y, 40.0, 44.0);
        let next_r = Rect::new(w - 56.0, nav_y, 40.0, 44.0);
        let p_hov = hovered_id == Some("nav_prev");
        let n_hov = hovered_id == Some("nav_next");
        ctx.fill_card(prev_r, 12.0, if p_hov { theme::VIOLET_TINT } else { theme::rgba(20, 20, 28, 0.78) }, if p_hov { theme::VIOLET_BORDER } else { theme::BORDER });
        ctx.draw_vector_icon(VectorIcon::ChevronLeft, prev_r.x + 20.0, prev_r.y + 22.0, 15.0, theme::TEXT_PRIMARY);
        hits.push(Hit { id: "nav_prev".into(), rect: prev_r, act: PhotoAct::Prev });

        ctx.fill_card(next_r, 12.0, if n_hov { theme::VIOLET_TINT } else { theme::rgba(20, 20, 28, 0.78) }, if n_hov { theme::VIOLET_BORDER } else { theme::BORDER });
        ctx.draw_vector_icon(VectorIcon::ChevronRight, next_r.x + 20.0, next_r.y + 22.0, 15.0, theme::TEXT_PRIMARY);
        hits.push(Hit { id: "nav_next".into(), rect: next_r, act: PhotoAct::Next });
    }

    // Toast notification pill
    if let Some(msg) = toast {
        let tw = (msg.len() as f32 * 6.8 + 36.0).clamp(200.0, w - 80.0);
        let tr = Rect::new((w - tw) * 0.5, vp.y + 14.0, tw, 32.0);
        ctx.fill_card(tr, 10.0, theme::rgba(22, 34, 28, 0.94), theme::EMERALD);
        ctx.draw_text_center(msg, tr, TextStyle::Body11Med, theme::TEXT_PRIMARY);
    }
    ctx.pop_clip();

    // ==================== BOTTOM THUMBNAIL FILMSTRIP ====================
    if strip_h > 0.0 {
        let fy = h - strip_h;
        ctx.fill_rrect(Rect::new(0.0, fy, w, strip_h), 0.0, theme::SURFACE);
        ctx.draw_line(0.0, fy, w, fy, theme::BORDER, 1.0);

        let thumb_w = 72.0;
        let thumb_h = 54.0;
        let gap = 8.0;
        let max_visible = ((w - 32.0) / (thumb_w + gap)).floor().max(1.0) as usize;
        let half = max_visible / 2;
        let start = cur_idx.saturating_sub(half);
        let end = (start + max_visible).min(siblings.len());
        let shown = end - start;
        let total_w = shown as f32 * thumb_w + shown.saturating_sub(1) as f32 * gap;
        let mut tx = (w - total_w) * 0.5;
        let ty = fy + (strip_h - thumb_h) * 0.5;

        for i in start..end {
            let tr = Rect::new(tx, ty, thumb_w, thumb_h);
            let active = i == cur_idx;
            let id = format!("thumb_{}", i);
            let hov = hovered_id == Some(id.as_str());
            ctx.fill_card(
                tr,
                8.0,
                theme::CARD_BG,
                if active {
                    theme::VIOLET_LIGHT
                } else if hov {
                    theme::BORDER
                } else {
                    theme::BORDER
                },
            );
            let inner = tr.inset(2.0, 2.0);
            ctx.push_clip(inner);
            let p_str = siblings[i].to_string_lossy().to_string();
            if !ctx.draw_image_file(&p_str, inner, if active { 1.0 } else { 0.78 }) {
                ctx.draw_vector_icon(VectorIcon::Image, inner.x + inner.w * 0.5, inner.y + inner.h * 0.5, 14.0, theme::TEXT_TERTIARY);
            }
            ctx.pop_clip();
            if active {
                ctx.stroke_rrect(tr, 8.0, theme::VIOLET_LIGHT, 2.0);
            }
            hits.push(Hit { id, rect: tr, act: PhotoAct::SelectIndex(i) });
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
        return aura_window::handle_getminmaxinfo(hwnd, lparam, 500, 360);
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
        if vk == windows::Win32::UI::Input::KeyboardAndMouse::VK_UP.0 { aura_window::snap_window(hwnd, SnapZone::Maximize); return LRESULT(0); }
        if vk == windows::Win32::UI::Input::KeyboardAndMouse::VK_DOWN.0 { aura_window::snap_window(hwnd, SnapZone::Center); return LRESULT(0); }
    }

    let mut start_title_drag = false;
    let mut deferred_max: Option<bool> = None;
    let mut deferred_min = false;
    let mut deferred_snap: Option<SnapZone> = None;

    let handled = PHOTO_RT.with(|cell| {
        let Ok(mut opt) = cell.try_borrow_mut() else { return false };
        let Some(rt) = opt.as_mut() else { return false };

        match msg {
            WM_TIMER if wparam.0 == TIMER_ASYNC => {
                if rt.gpu.drain_decoded_assets() || rt.toast.as_ref().map(|(_, t)| t.elapsed().as_secs_f32() > 3.2).unwrap_or(false) {
                    rt.redraw();
                }
                true
            }
            WM_SIZE => {
                let mut rc = RECT::default();
                if GetClientRect(hwnd, &mut rc).is_ok() {
                    let raw_w = rc.right - rc.left;
                    let raw_h = rc.bottom - rc.top;
                    if raw_w > 0 && raw_h > 0 {
                        let nw = raw_w.max(400) as u32;
                        let nh = raw_h.max(300) as u32;
                        rt.maximized = IsZoomed(hwnd).as_bool();
                        rt.width = nw as f32;
                        rt.height = nh as f32;
                        rt.surf.resize(&rt.gpu, nw, nh);
                        rt.redraw();
                    }
                }
                true
            }
            WM_MOUSEMOVE => {
                let px = (lparam.0 & 0xFFFF) as i16 as f32;
                let py = ((lparam.0 >> 16) & 0xFFFF) as i16 as f32;
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
                if rt.current_asset.is_some() {
                    rt.dragging = true;
                    rt.drag_last = (px, py);
                    SetCapture(hwnd);
                }
                true
            }
            WM_RBUTTONDOWN => {
                let px = (lparam.0 & 0xFFFF) as i16 as f32;
                let py = ((lparam.0 >> 16) & 0xFFFF) as i16 as f32;
                if let Some(hit) = rt.hits.iter().rev().find(|h| h.rect.contains(px, py)) {
                    if matches!(hit.act, PhotoAct::WinMax) {
                        deferred_snap = Some(SnapZone::LeftHalf);
                    }
                }
                true
            }
            WM_LBUTTONUP => {
                if rt.dragging {
                    rt.dragging = false;
                    let _ = ReleaseCapture();
                }
                true
            }
            WM_LBUTTONDBLCLK => {
                let py = ((lparam.0 >> 16) & 0xFFFF) as i16 as f32;
                if py < TOP_BAR_H {
                    rt.maximized = !rt.maximized;
                    deferred_max = Some(rt.maximized);
                } else if rt.zoom == 0.0 {
                    rt.execute(PhotoAct::Zoom100);
                } else {
                    rt.execute(PhotoAct::ZoomFit);
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
                if vk == VK_LEFT.0 {
                    rt.execute(PhotoAct::Prev);
                } else if vk == VK_RIGHT.0 {
                    rt.execute(PhotoAct::Next);
                } else if vk == VK_OEM_PLUS.0 {
                    rt.execute(PhotoAct::ZoomIn);
                } else if vk == VK_OEM_MINUS.0 {
                    rt.execute(PhotoAct::ZoomOut);
                } else if vk == b'0' as u16 {
                    rt.execute(PhotoAct::ZoomFit);
                } else if vk == b'1' as u16 {
                    rt.execute(PhotoAct::Zoom100);
                } else if vk == b'R' as u16 {
                    rt.execute(PhotoAct::RotateCw);
                } else if vk == b'F' as u16 {
                    rt.execute(PhotoAct::ToggleFilmstrip);
                } else if vk == VK_DELETE.0 {
                    rt.execute(PhotoAct::DeleteRecycle);
                } else if vk == VK_ESCAPE.0 {
                    rt.execute(PhotoAct::WinClose);
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
    } else if deferred_snap.is_some() {
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
