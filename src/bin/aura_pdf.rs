//! AuraPDF Studio v5.0 — Native Pure-Rust + Direct2D / WinRT PDF Viewer, Annotator & Page Editor
//! - Crisp high-DPI vector PDF rendering via `Windows.Data.Pdf` + Direct2D GPU swapchain
//! - macOS Preview-inspired left thumbnail rail, smooth zoom & drag-pan, Dark Reading Mode (smart invert)
//! - Page Editor: Rotate 90° CW, Move Page Up/Down, Delete Page, Merge external PDF (`+ Combinar`)
//! - Live Markup & Signatures: Highlighter, Freehand Pen/Signature, Rectangle, Sticky Text Note, Color Swatches, Undo
//! - Standards-compliant `.pdf` export (`pdf_engine::export_edited_pdf`) + `agent_bridge` UI tree exposure!

#![windows_subsystem = "windows"]

use aura_os::agent_bridge;
use aura_os::aura_window::{self, SnapZone};
use aura_os::pdf_engine;
use aura_os::renderer::{DrawCtx, GpuEngine, Rect, TextStyle, WindowSurface};
use aura_os::theme::{self, VectorIcon};
use aura_os::win32_shell;
use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::time::Instant;
use windows::core::PCWSTR;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    ReleaseCapture, SetCapture, VK_BACK, VK_DELETE, VK_DOWN, VK_ESCAPE, VK_LEFT, VK_NEXT,
    VK_OEM_MINUS, VK_OEM_PLUS, VK_PRIOR, VK_RETURN, VK_RIGHT, VK_UP,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, GetClientRect, GetMessageW, IsZoomed,
    LoadCursorW, PostQuitMessage, RegisterClassW, SendMessageW, SetTimer, ShowWindow,
    TranslateMessage, HTCAPTION, IDC_ARROW, MSG, SW_HIDE, SW_MAXIMIZE, SW_MINIMIZE, SW_RESTORE,
    SW_SHOW, WM_CHAR, WM_CLOSE, WM_DESTROY, WM_GETMINMAXINFO, WM_KEYDOWN, WM_LBUTTONDBLCLK,
    WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE, WM_MOUSEWHEEL, WM_NCCALCSIZE, WM_NCHITTEST,
    WM_NCLBUTTONDOWN, WM_RBUTTONDOWN, WM_SIZE, WM_SYSKEYDOWN, WM_TIMER, WNDCLASSW,
    WS_EX_NOREDIRECTIONBITMAP,
};

const TIMER_IDLE: usize = 31;
const TOP_BAR_H: f32 = 50.0;
const TOOL_BAR_H: f32 = 42.0;
const HEADER_TOTAL_H: f32 = TOP_BAR_H + TOOL_BAR_H;
const SIDEBAR_W: f32 = 196.0;

#[derive(Clone, Copy, PartialEq, Eq)]
enum PdfTool {
    PanSelect,
    Highlighter,
    PenSign,
    RectBox,
    TextNote,
}

#[derive(Clone)]
enum Annotation {
    Stroke {
        pts: Vec<(f32, f32)>, // normalized [0..1] page coords
        color: [u8; 4],       // R, G, B, A
        thickness_norm: f32,
        is_highlight: bool,
    },
    Rect {
        x0: f32,
        y0: f32,
        x1: f32,
        y1: f32,
        color: [u8; 4],
    },
    Note {
        x: f32,
        y: f32,
        text: String,
        color: [u8; 4],
    },
}

#[derive(Clone)]
struct PdfPageEntry {
    width: u32,
    height: u32,
    base_bgra: Vec<u8>,
    thumb_w: u32,
    thumb_h: u32,
    thumb_bgra: Vec<u8>,
    annotations: Vec<Annotation>,
    rev: u64,
}

#[derive(Clone)]
enum PdfAct {
    WinClose,
    WinMin,
    WinMax,
    OpenPdf,
    MergePdf,
    SavePdf,
    PrevPage,
    NextPage,
    SelectPage(usize),
    ZoomIn,
    ZoomOut,
    ZoomFit,
    RotatePageCw,
    MovePageUp,
    MovePageDown,
    DeletePage,
    ToggleDarkRead,
    ToggleSidebar,
    SelectTool(PdfTool),
    SelectColor(usize),
    UndoAnnot,
}

#[derive(Clone)]
struct Hit {
    id: String,
    label: String,
    rect: Rect,
    act: PdfAct,
}

const PALETTE: [([u8; 4], &str); 5] = [
    ([250, 204, 21, 255], "Ámbar"),
    ([124, 92, 255, 255], "Violeta"),
    ([244, 63, 94, 255], "Coral"),
    ([34, 211, 238, 255], "Cian"),
    ([34, 197, 94, 255], "Esmeralda"),
];

struct PdfRuntime {
    hwnd: HWND,
    gpu: GpuEngine,
    surf: WindowSurface,
    width: f32,
    height: f32,
    maximized: bool,

    doc_title: String,
    doc_path: Option<PathBuf>,
    pages: Vec<PdfPageEntry>,
    cur_page: usize,
    sidebar_open: bool,
    sidebar_scroll: f32,
    dark_reading: bool,

    zoom: f32, // 0.0 = fit page cleanly
    pan_x: f32,
    pan_y: f32,

    tool: PdfTool,
    color_idx: usize,
    active_stroke: Vec<(f32, f32)>,
    rect_drag: Option<((f32, f32), (f32, f32))>,
    editing_note_idx: Option<usize>,

    panning: bool,
    pan_last: (f32, f32),
    last_page_screen_rect: Rect,

    hovered_id: Option<String>,
    pressed_id: Option<String>,
    hits: Vec<Hit>,
    toast: Option<(String, Instant)>,
    rev_counter: u64,
}

thread_local! {
    static PDF_RT: RefCell<Option<PdfRuntime>> = const { RefCell::new(None) };
}

fn downscale_bgra(w: u32, h: u32, src: &[u8], max_side: u32) -> (u32, u32, Vec<u8>) {
    let longest = w.max(h).max(1);
    if longest <= max_side {
        return (w, h, src.to_vec());
    }
    let scale = max_side as f32 / longest as f32;
    let nw = ((w as f32 * scale).round() as u32).max(1);
    let nh = ((h as f32 * scale).round() as u32).max(1);
    let mut dst = vec![0u8; (nw * nh * 4) as usize];
    for dy in 0..nh {
        let sy = ((dy * h) / nh).min(h - 1);
        for dx in 0..nw {
            let sx = ((dx * w) / nw).min(w - 1);
            let si = ((sy * w + sx) * 4) as usize;
            let di = ((dy * nw + dx) * 4) as usize;
            dst[di..di + 4].copy_from_slice(&src[si..si + 4]);
        }
    }
    (nw, nh, dst)
}

/// Rasters vector annotations onto a copy of `base_bgra` so exported `.pdf` preserves every highlight, signature & note!
fn bake_page_with_annotations(page: &PdfPageEntry) -> (u32, u32, Vec<u8>) {
    let w = page.width as i32;
    let h = page.height as i32;
    let mut buf = page.base_bgra.clone();
    if w <= 0 || h <= 0 {
        return (page.width, page.height, buf);
    }

    let blend_px = |buf: &mut [u8], x: i32, y: i32, col: [u8; 4], multiply_hl: bool| {
        if x < 0 || y < 0 || x >= w || y >= h {
            return;
        }
        let idx = ((y * w + x) * 4) as usize;
        let a = col[3] as f32 / 255.0;
        if multiply_hl {
            // Highlighter multiply-tint preserves crisp dark text underneath!
            let mr = col[0] as f32 / 255.0;
            let mg = col[1] as f32 / 255.0;
            let mb = col[2] as f32 / 255.0;
            buf[idx] = ((buf[idx] as f32 * (1.0 - a + a * mb)).clamp(0.0, 255.0)) as u8;
            buf[idx + 1] = ((buf[idx + 1] as f32 * (1.0 - a + a * mg)).clamp(0.0, 255.0)) as u8;
            buf[idx + 2] = ((buf[idx + 2] as f32 * (1.0 - a + a * mr)).clamp(0.0, 255.0)) as u8;
        } else {
            let inv = 1.0 - a;
            buf[idx] = (col[2] as f32 * a + buf[idx] as f32 * inv).clamp(0.0, 255.0) as u8;
            buf[idx + 1] = (col[1] as f32 * a + buf[idx + 1] as f32 * inv).clamp(0.0, 255.0) as u8;
            buf[idx + 2] = (col[0] as f32 * a + buf[idx + 2] as f32 * inv).clamp(0.0, 255.0) as u8;
        }
        buf[idx + 3] = 255;
    };

    for ann in &page.annotations {
        match ann {
            Annotation::Stroke {
                pts,
                color,
                thickness_norm,
                is_highlight,
            } => {
                let rad = ((thickness_norm * w as f32) * 0.5).clamp(1.5, 28.0) as i32;
                let mut eff_col = *color;
                if *is_highlight {
                    eff_col[3] = 110;
                }
                for pair in pts.windows(2) {
                    let (x0, y0) = ((pair[0].0 * w as f32) as i32, (pair[0].1 * h as f32) as i32);
                    let (x1, y1) = ((pair[1].0 * w as f32) as i32, (pair[1].1 * h as f32) as i32);
                    let steps = (x1 - x0).abs().max((y1 - y0).abs()).max(1);
                    for s in 0..=steps {
                        let cx = x0 + (x1 - x0) * s / steps;
                        let cy = y0 + (y1 - y0) * s / steps;
                        let ry = if *is_highlight { (rad * 2).max(6) } else { rad };
                        for oy in -ry..=ry {
                            for ox in -rad..=rad {
                                if *is_highlight || ox * ox + oy * oy <= rad * rad {
                                    blend_px(&mut buf, cx + ox, cy + oy, eff_col, *is_highlight);
                                }
                            }
                        }
                    }
                }
            }
            Annotation::Rect {
                x0,
                y0,
                x1,
                y1,
                color,
            } => {
                let lx = (x0.min(*x1) * w as f32) as i32;
                let rx = (x0.max(*x1) * w as f32) as i32;
                let ty = (y0.min(*y1) * h as f32) as i32;
                let by = (y0.max(*y1) * h as f32) as i32;
                let bw = (w / 240).clamp(2, 6);
                for x in lx..=rx {
                    for t in 0..bw {
                        blend_px(&mut buf, x, ty + t, *color, false);
                        blend_px(&mut buf, x, by - t, *color, false);
                    }
                }
                for y in ty..=by {
                    for t in 0..bw {
                        blend_px(&mut buf, lx + t, y, *color, false);
                        blend_px(&mut buf, rx - t, y, *color, false);
                    }
                }
            }
            Annotation::Note { x, y, color, .. } => {
                let nx = (*x * w as f32) as i32;
                let ny = (*y * h as f32) as i32;
                let nw = (w / 5).max(120);
                let nh = (h / 16).max(36);
                let mut fill = *color;
                fill[3] = 215;
                for yy in ny..(ny + nh) {
                    for xx in nx..(nx + nw) {
                        blend_px(&mut buf, xx, yy, fill, false);
                    }
                }
            }
        }
    }

    (page.width, page.height, buf)
}

fn load_pdf_pages(path: &Path, start_rev: &mut u64) -> Vec<PdfPageEntry> {
    let total = pdf_engine::get_pdf_page_count(path).unwrap_or(0);
    let mut out = Vec::new();
    let cap = total.min(80); // fast responsive load up to 80 pages
    for idx in 0..cap {
        if let Some(rp) = pdf_engine::render_pdf_page_bgra(path, idx, 1550) {
            *start_rev = start_rev.wrapping_add(1);
            let (tw, th, tbgra) = downscale_bgra(rp.width, rp.height, &rp.bgra, 220);
            out.push(PdfPageEntry {
                width: rp.width,
                height: rp.height,
                base_bgra: rp.bgra,
                thumb_w: tw,
                thumb_h: th,
                thumb_bgra: tbgra,
                annotations: Vec::new(),
                rev: *start_rev,
            });
        }
    }
    out
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() >= 3 && args[1].eq_ignore_ascii_case("--capture") {
        let out_path = PathBuf::from(&args[2]);
        let sample = args
            .get(3)
            .map(PathBuf::from)
            .filter(|p| p.exists())
            .unwrap_or_else(pdf_engine::ensure_sample_welcome_pdf);
        run_capture(&out_path, &sample);
        return;
    }

    let initial_pdf = args
        .get(1)
        .map(PathBuf::from)
        .filter(|p| p.exists())
        .unwrap_or_else(pdf_engine::ensure_sample_welcome_pdf);

    let gpu = match GpuEngine::new() {
        Ok(g) => g,
        Err(_) => return,
    };

    let (sw, sh) = win32_shell::screen_size();
    let w = (sw as f32 * 0.78).clamp(1020.0, 1460.0) as i32;
    let h = (sh as f32 * 0.82).clamp(680.0, 920.0) as i32;
    let x = (sw - w) / 2;
    let y = ((sh - h) / 2).max(38);

    let cls = win32_shell::to_wide("AuraPDF_Studio_v5_Wnd");
    let title = win32_shell::to_wide("AuraPDF Studio");
    let hwnd = unsafe {
        let hinst = GetModuleHandleW(None).unwrap_or_default();
        let wc = WNDCLASSW {
            lpfnWndProc: Some(pdf_wnd_proc),
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

    let mut rev_counter = 1u64;
    let pages = load_pdf_pages(&initial_pdf, &mut rev_counter);
    let doc_title = initial_pdf
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "Documento.pdf".into());

    let mut rt = PdfRuntime {
        hwnd,
        gpu,
        surf,
        width: w as f32,
        height: h as f32,
        maximized: false,
        doc_title,
        doc_path: Some(initial_pdf),
        pages,
        cur_page: 0,
        sidebar_open: true,
        sidebar_scroll: 0.0,
        dark_reading: false,
        zoom: 0.0,
        pan_x: 0.0,
        pan_y: 0.0,
        tool: PdfTool::PanSelect,
        color_idx: 0,
        active_stroke: Vec::new(),
        rect_drag: None,
        editing_note_idx: None,
        panning: false,
        pan_last: (0.0, 0.0),
        last_page_screen_rect: Rect::new(0.0, 0.0, 1.0, 1.0),
        hovered_id: None,
        pressed_id: None,
        hits: Vec::new(),
        toast: None,
        rev_counter,
    };

    rt.redraw();
    unsafe {
        let _ = ShowWindow(hwnd, SW_SHOW);
        let _ = SetTimer(hwnd, TIMER_IDLE, 250, None);
    }

    PDF_RT.with(|c| *c.borrow_mut() = Some(rt));

    unsafe {
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, HWND::default(), 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }

    PDF_RT.with(|c| {
        if let Ok(mut g) = c.try_borrow_mut() {
            *g = None;
        }
    });
}

fn run_capture(out_path: &Path, sample_pdf: &Path) {
    let Ok(mut gpu) = GpuEngine::new() else { return };
    let mut rev = 10u64;
    let mut pages = load_pdf_pages(sample_pdf, &mut rev);
    if let Some(first) = pages.first_mut() {
        first.annotations.push(Annotation::Stroke {
            pts: vec![(0.11, 0.27), (0.64, 0.27)],
            color: [250, 204, 21, 255],
            thickness_norm: 0.022,
            is_highlight: true,
        });
        first.annotations.push(Annotation::Note {
            x: 0.58,
            y: 0.54,
            text: "Revisado con AuraPDF v5.0 ✓".into(),
            color: [124, 92, 255, 255],
        });
    }
    let title = sample_pdf
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "AuraPDF_Guia.pdf".into());

    let _ = gpu.render_to_png(1320, 820, out_path, |ctx| {
        let mut hits = Vec::new();
        let mut page_rect = Rect::new(0.0, 0.0, 1.0, 1.0);
        render_pdf_ui(
            ctx,
            1320.0,
            820.0,
            false,
            &title,
            &pages,
            0,
            true,
            0.0,
            false,
            0.0,
            0.0,
            0.0,
            PdfTool::Highlighter,
            0,
            &[],
            None,
            None,
            None,
            None,
            Some("PDF listo · Anotaciones vectoriales y exportación nativa activa"),
            &mut page_rect,
            &mut hits,
        );
    });
}

impl PdfRuntime {
    fn set_toast(&mut self, msg: &str) {
        self.toast = Some((msg.to_string(), Instant::now()));
    }

    fn open_pdf_file(&mut self, path: &Path) {
        let loaded = load_pdf_pages(path, &mut self.rev_counter);
        if loaded.is_empty() {
            self.set_toast("No se pudo abrir el documento PDF.");
            return;
        }
        self.doc_title = path
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "Documento.pdf".into());
        self.doc_path = Some(path.to_path_buf());
        self.pages = loaded;
        self.cur_page = 0;
        self.zoom = 0.0;
        self.pan_x = 0.0;
        self.pan_y = 0.0;
        self.editing_note_idx = None;
        self.set_toast(&format!("Abierto · {} páginas", self.pages.len()));
    }

    fn effective_zoom(&self) -> f32 {
        if self.zoom > 0.0 {
            return self.zoom;
        }
        let Some(p) = self.pages.get(self.cur_page) else {
            return 1.0;
        };
        let side = if self.sidebar_open { SIDEBAR_W } else { 0.0 };
        let vw = (self.width - side - 56.0).max(160.0);
        let vh = (self.height - HEADER_TOTAL_H - 44.0).max(160.0);
        (vw / p.width.max(1) as f32).min(vh / p.height.max(1) as f32)
    }

    fn screen_to_page_norm(&self, sx: f32, sy: f32) -> Option<(f32, f32)> {
        let r = self.last_page_screen_rect;
        if r.w <= 4.0 || r.h <= 4.0 || !r.contains(sx, sy) {
            return None;
        }
        Some((((sx - r.x) / r.w).clamp(0.0, 1.0), ((sy - r.y) / r.h).clamp(0.0, 1.0)))
    }

    fn execute(&mut self, act: PdfAct) {
        match act {
            PdfAct::WinClose => unsafe {
                let _ = ShowWindow(self.hwnd, SW_HIDE);
                PostQuitMessage(0);
                return;
            },
            PdfAct::WinMin => unsafe {
                let _ = ShowWindow(self.hwnd, SW_MINIMIZE);
            },
            PdfAct::WinMax => unsafe {
                self.maximized = !self.maximized;
                let _ = ShowWindow(
                    self.hwnd,
                    if self.maximized { SW_MAXIMIZE } else { SW_RESTORE },
                );
            },
            PdfAct::OpenPdf => {
                if let Some(p) = win32_shell::pick_file_dialog(
                    "Abrir documento PDF en AuraPDF",
                    "Documentos PDF (*.pdf)",
                    "*.pdf",
                ) {
                    self.open_pdf_file(Path::new(&p));
                }
            }
            PdfAct::MergePdf => {
                if let Some(p) = win32_shell::pick_file_dialog(
                    "Combinar otro PDF al final del documento",
                    "Documentos PDF (*.pdf)",
                    "*.pdf",
                ) {
                    let extra = load_pdf_pages(Path::new(&p), &mut self.rev_counter);
                    let added = extra.len();
                    if added > 0 {
                        self.pages.extend(extra);
                        self.set_toast(&format!("Combinadas +{} páginas al documento", added));
                    }
                }
            }
            PdfAct::SavePdf => {
                let default_name = self
                    .doc_path
                    .as_ref()
                    .and_then(|p| p.file_stem())
                    .map(|s| format!("{}_editado.pdf", s.to_string_lossy()))
                    .unwrap_or_else(|| "Documento_AuraPDF.pdf".into());
                if let Some(out) = win32_shell::save_file_dialog(
                    "Guardar PDF editado",
                    "Documento PDF (*.pdf)",
                    "*.pdf",
                    "pdf",
                    &default_name,
                ) {
                    let baked: Vec<(u32, u32, Vec<u8>)> =
                        self.pages.iter().map(bake_page_with_annotations).collect();
                    match pdf_engine::export_edited_pdf(Path::new(&out), &baked) {
                        Ok(()) => self.set_toast("PDF guardado correctamente ✓"),
                        Err(e) => self.set_toast(&e),
                    }
                }
            }
            PdfAct::PrevPage => {
                if self.cur_page > 0 {
                    self.cur_page -= 1;
                    self.editing_note_idx = None;
                }
            }
            PdfAct::NextPage => {
                if self.cur_page + 1 < self.pages.len() {
                    self.cur_page += 1;
                    self.editing_note_idx = None;
                }
            }
            PdfAct::SelectPage(i) => {
                if i < self.pages.len() {
                    self.cur_page = i;
                    self.editing_note_idx = None;
                }
            }
            PdfAct::ZoomIn => {
                let z = self.effective_zoom();
                self.zoom = (z * 1.22).clamp(0.2, 4.5);
            }
            PdfAct::ZoomOut => {
                let z = self.effective_zoom();
                self.zoom = (z / 1.22).clamp(0.2, 4.5);
            }
            PdfAct::ZoomFit => {
                self.zoom = 0.0;
                self.pan_x = 0.0;
                self.pan_y = 0.0;
            }
            PdfAct::RotatePageCw => {
                if let Some(p) = self.pages.get_mut(self.cur_page) {
                    let (nw, nh, nbgra) =
                        pdf_engine::rotate_bgra_90_cw(p.width, p.height, &p.base_bgra);
                    let (tw, th, tbgra) = downscale_bgra(nw, nh, &nbgra, 220);
                    p.width = nw;
                    p.height = nh;
                    p.base_bgra = nbgra;
                    p.thumb_w = tw;
                    p.thumb_h = th;
                    p.thumb_bgra = tbgra;
                    self.rev_counter = self.rev_counter.wrapping_add(1);
                    p.rev = self.rev_counter;
                    self.set_toast("Página rotada 90°");
                }
            }
            PdfAct::MovePageUp => {
                if self.cur_page > 0 && self.cur_page < self.pages.len() {
                    self.pages.swap(self.cur_page, self.cur_page - 1);
                    self.cur_page -= 1;
                    self.set_toast("Página movida hacia arriba");
                }
            }
            PdfAct::MovePageDown => {
                if self.cur_page + 1 < self.pages.len() {
                    self.pages.swap(self.cur_page, self.cur_page + 1);
                    self.cur_page += 1;
                    self.set_toast("Página movida hacia abajo");
                }
            }
            PdfAct::DeletePage => {
                if self.pages.len() > 1 {
                    self.pages.remove(self.cur_page);
                    if self.cur_page >= self.pages.len() {
                        self.cur_page = self.pages.len() - 1;
                    }
                    self.editing_note_idx = None;
                    self.set_toast("Página eliminada del documento");
                } else {
                    self.set_toast("El documento debe conservar al menos 1 página");
                }
            }
            PdfAct::ToggleDarkRead => {
                self.dark_reading = !self.dark_reading;
                self.set_toast(if self.dark_reading {
                    "Modo lectura nocturna activado"
                } else {
                    "Modo papel original restaurado"
                });
            }
            PdfAct::ToggleSidebar => {
                self.sidebar_open = !self.sidebar_open;
            }
            PdfAct::SelectTool(t) => {
                self.tool = t;
                self.editing_note_idx = None;
            }
            PdfAct::SelectColor(c) => {
                self.color_idx = c % PALETTE.len();
            }
            PdfAct::UndoAnnot => {
                if let Some(p) = self.pages.get_mut(self.cur_page) {
                    if p.annotations.pop().is_some() {
                        self.editing_note_idx = None;
                        self.set_toast("Anotación deshecha");
                    }
                }
            }
        }
        self.redraw();
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
        let title = &self.doc_title;
        let pages = &self.pages;
        let cur = self.cur_page;
        let sb = self.sidebar_open;
        let sb_scroll = self.sidebar_scroll;
        let dark = self.dark_reading;
        let zoom = self.zoom;
        let px = self.pan_x;
        let py = self.pan_y;
        let tool = self.tool;
        let col_idx = self.color_idx;
        let stroke = &self.active_stroke;
        let rdrag = self.rect_drag;
        let hov = self.hovered_id.as_deref();
        let prs = self.pressed_id.as_deref();
        let edit_note = self.editing_note_idx;
        let toast = self.toast.as_ref().map(|(m, _)| m.as_str());
        let page_rect = &mut self.last_page_screen_rect;
        let hits = &mut self.hits;

        self.surf.draw(&mut self.gpu, |ctx| {
            render_pdf_ui(
                ctx, w, h, maxd, title, pages, cur, sb, sb_scroll, dark, zoom, px, py, tool,
                col_idx, stroke, rdrag, edit_note, hov, prs, toast, page_rect, hits,
            );
        });

        let raw_nodes: Vec<(String, String, f32, f32, f32, f32)> = self
            .hits
            .iter()
            .map(|h| (h.id.clone(), h.label.clone(), h.rect.x, h.rect.y, h.rect.w, h.rect.h))
            .collect();
        agent_bridge::publish_ui_tree("AuraPDF", self.hwnd, &raw_nodes);
    }
}

#[allow(clippy::too_many_arguments)]
fn render_pdf_ui(
    ctx: &mut DrawCtx<'_>,
    w: f32,
    h: f32,
    maximized: bool,
    doc_title: &str,
    pages: &[PdfPageEntry],
    cur_page: usize,
    sidebar_open: bool,
    sidebar_scroll: f32,
    dark_reading: bool,
    zoom: f32,
    pan_x: f32,
    pan_y: f32,
    tool: PdfTool,
    color_idx: usize,
    active_stroke: &[(f32, f32)],
    rect_drag: Option<((f32, f32), (f32, f32))>,
    editing_note_idx: Option<usize>,
    hovered_id: Option<&str>,
    pressed_id: Option<&str>,
    toast: Option<&str>,
    out_page_rect: &mut Rect,
    hits: &mut Vec<Hit>,
) {
    hits.clear();
    let corner = if maximized { 0.0 } else { 18.0 };
    let bounds = Rect::new(0.0, 0.0, w, h);
    let _mask = if corner > 0.0 {
        ctx.push_rrect_mask_layer(bounds, bounds, corner, 1.0)
    } else {
        None
    };

    ctx.fill_rrect(bounds, corner, theme::CANVAS);

    // ==================== ROW 1: TITLEBAR & DOCUMENT ACTIONS ====================
    ctx.fill_rrect(Rect::new(0.0, 0.0, w, TOP_BAR_H), 0.0, theme::SURFACE);
    ctx.draw_line(0.0, TOP_BAR_H, w, TOP_BAR_H, theme::BORDER, 1.0);

    let lights = [
        ("pdf_close", "Cerrar", 20.0, theme::CORAL, "×", PdfAct::WinClose),
        ("pdf_min", "Minimizar", 40.0, theme::AMBER, "−", PdfAct::WinMin),
        ("pdf_max", "Maximizar", 60.0, theme::EMERALD, "+", PdfAct::WinMax),
    ];
    let light_hov = matches!(hovered_id, Some("pdf_close") | Some("pdf_min") | Some("pdf_max"));
    for (id, lbl, cx, col, gl, act) in lights {
        ctx.fill_circle(cx, 25.0, if light_hov { 6.3 } else { 5.8 }, col);
        if light_hov {
            ctx.draw_text_center(gl, Rect::new(cx - 6.0, 18.0, 12.0, 13.0), TextStyle::Small9Reg, theme::DARK_INK);
        }
        hits.push(Hit {
            id: id.into(),
            label: lbl.into(),
            rect: Rect::new(cx - 8.0, 15.0, 16.0, 20.0),
            act,
        });
    }

    // Sidebar Toggle + Title + Page Counter Pill
    let sb_r = Rect::new(82.0, 11.0, 32.0, 28.0);
    draw_btn(ctx, sb_r, "pdf_sb", "", Some(VectorIcon::AppGrid), sidebar_open, hovered_id, pressed_id);
    hits.push(Hit { id: "pdf_sb".into(), label: "Miniaturas".into(), rect: sb_r, act: PdfAct::ToggleSidebar });

    ctx.draw_vector_icon(VectorIcon::FileText, 128.0, 25.0, 14.0, theme::CORAL);
    ctx.draw_text(
        doc_title,
        Rect::new(142.0, 14.0, (w * 0.25).clamp(140.0, 320.0), 22.0),
        TextStyle::Body12Semi,
        theme::TEXT_PRIMARY,
    );

    // Center Page Stepper (`◀  Pág 1 / 2  ▶`)
    let nav_x = (w * 0.5 - 92.0).max(330.0);
    let prev_r = Rect::new(nav_x, 11.0, 28.0, 28.0);
    draw_btn(ctx, prev_r, "pdf_prev", "", Some(VectorIcon::ChevronLeft), false, hovered_id, pressed_id);
    hits.push(Hit { id: "pdf_prev".into(), label: "Página anterior".into(), rect: prev_r, act: PdfAct::PrevPage });

    let pg_pill = Rect::new(nav_x + 32.0, 11.0, 116.0, 28.0);
    ctx.fill_card(pg_pill, 9.0, theme::CARD_BG, theme::BORDER);
    let pg_txt = if pages.is_empty() {
        "0 / 0".to_string()
    } else {
        format!("Pág {} / {}", cur_page + 1, pages.len())
    };
    ctx.draw_text_center(&pg_txt, pg_pill, TextStyle::Mono11Med, theme::TEXT_PRIMARY);

    let next_r = Rect::new(nav_x + 152.0, 11.0, 28.0, 28.0);
    draw_btn(ctx, next_r, "pdf_next", "", Some(VectorIcon::ChevronRight), false, hovered_id, pressed_id);
    hits.push(Hit { id: "pdf_next".into(), label: "Página siguiente".into(), rect: next_r, act: PdfAct::NextPage });

    // Right Document File Buttons: Abrir | + Combinar | Guardar PDF
    let mut rx = w - 14.0;
    let save_r = Rect::new(rx - 114.0, 10.0, 114.0, 30.0);
    draw_primary_btn(ctx, save_r, "pdf_save", "Guardar PDF", hovered_id, pressed_id);
    hits.push(Hit { id: "pdf_save".into(), label: "Guardar PDF".into(), rect: save_r, act: PdfAct::SavePdf });
    rx -= 122.0;

    let merge_r = Rect::new(rx - 98.0, 10.0, 98.0, 30.0);
    draw_btn(ctx, merge_r, "pdf_merge", "+ Combinar", None, false, hovered_id, pressed_id);
    hits.push(Hit { id: "pdf_merge".into(), label: "Combinar PDF".into(), rect: merge_r, act: PdfAct::MergePdf });
    rx -= 106.0;

    let open_r = Rect::new(rx - 76.0, 10.0, 76.0, 30.0);
    draw_btn(ctx, open_r, "pdf_open", "Abrir...", None, false, hovered_id, pressed_id);
    hits.push(Hit { id: "pdf_open".into(), label: "Abrir PDF".into(), rect: open_r, act: PdfAct::OpenPdf });

    // ==================== ROW 2: STUDIO MARKUP & PAGE TOOLS ====================
    ctx.fill_rrect(Rect::new(0.0, TOP_BAR_H, w, TOOL_BAR_H), 0.0, theme::rgb(17, 17, 24));
    ctx.draw_line(0.0, HEADER_TOTAL_H, w, HEADER_TOTAL_H, theme::BORDER, 1.0);

    let mut tx = 14.0;
    let ty = TOP_BAR_H + 7.0;
    let tools = [
        ("tl_pan", "Mover", PdfTool::PanSelect),
        ("tl_hl", "Resaltar", PdfTool::Highlighter),
        ("tl_pen", "Pluma / Firma", PdfTool::PenSign),
        ("tl_rect", "Recuadro", PdfTool::RectBox),
        ("tl_note", "Nota texto", PdfTool::TextNote),
    ];
    for (id, lbl, t) in tools {
        let bw = (lbl.len() as f32 * 6.8 + 22.0).max(66.0);
        let br = Rect::new(tx, ty, bw, 28.0);
        draw_btn(ctx, br, id, lbl, None, tool == t, hovered_id, pressed_id);
        hits.push(Hit { id: id.into(), label: lbl.into(), rect: br, act: PdfAct::SelectTool(t) });
        tx += bw + 6.0;
    }

    // Color swatches
    tx += 6.0;
    for (ci, (rgba_c, cname)) in PALETTE.iter().enumerate() {
        let cx = tx + 11.0;
        let cy = TOP_BAR_H + 21.0;
        let col = theme::rgb(rgba_c[0], rgba_c[1], rgba_c[2]);
        ctx.fill_circle(cx, cy, if color_idx == ci { 8.5 } else { 6.8 }, col);
        if color_idx == ci {
            ctx.stroke_circle(cx, cy, 10.5, theme::CRISP_WHITE, 1.8);
        }
        let id = format!("pdf_col_{}", ci);
        hits.push(Hit {
            id,
            label: format!("Color {}", cname),
            rect: Rect::new(tx, ty, 22.0, 28.0),
            act: PdfAct::SelectColor(ci),
        });
        tx += 25.0;
    }

    let undo_r = Rect::new(tx + 6.0, ty, 74.0, 28.0);
    draw_btn(ctx, undo_r, "pdf_undo", "Deshacer", None, false, hovered_id, pressed_id);
    hits.push(Hit { id: "pdf_undo".into(), label: "Deshacer anotación".into(), rect: undo_r, act: PdfAct::UndoAnnot });

    // Right side of Row 2: Page Mutations + Dark Reading + Zoom
    let mut r2 = w - 14.0;
    let zplus = Rect::new(r2 - 30.0, ty, 30.0, 28.0);
    draw_btn(ctx, zplus, "pdf_zin", "", Some(VectorIcon::ZoomIn), false, hovered_id, pressed_id);
    hits.push(Hit { id: "pdf_zin".into(), label: "Acercar".into(), rect: zplus, act: PdfAct::ZoomIn });
    r2 -= 34.0;

    let zfit = Rect::new(r2 - 64.0, ty, 64.0, 28.0);
    draw_btn(ctx, zfit, "pdf_zfit", "Ajustar", None, zoom == 0.0, hovered_id, pressed_id);
    hits.push(Hit { id: "pdf_zfit".into(), label: "Ajustar página".into(), rect: zfit, act: PdfAct::ZoomFit });
    r2 -= 68.0;

    let zmin = Rect::new(r2 - 30.0, ty, 30.0, 28.0);
    draw_btn(ctx, zmin, "pdf_zout", "", Some(VectorIcon::ZoomOut), false, hovered_id, pressed_id);
    hits.push(Hit { id: "pdf_zout".into(), label: "Alejar".into(), rect: zmin, act: PdfAct::ZoomOut });
    r2 -= 40.0;

    let dark_r = Rect::new(r2 - 96.0, ty, 96.0, 28.0);
    draw_btn(ctx, dark_r, "pdf_dark", "Modo Noche", None, dark_reading, hovered_id, pressed_id);
    hits.push(Hit { id: "pdf_dark".into(), label: "Modo lectura nocturna".into(), rect: dark_r, act: PdfAct::ToggleDarkRead });
    r2 -= 104.0;

    let del_r = Rect::new(r2 - 30.0, ty, 30.0, 28.0);
    draw_btn(ctx, del_r, "pdf_delp", "", Some(VectorIcon::Trash), false, hovered_id, pressed_id);
    hits.push(Hit { id: "pdf_delp".into(), label: "Eliminar página".into(), rect: del_r, act: PdfAct::DeletePage });
    r2 -= 35.0;

    let dn_r = Rect::new(r2 - 34.0, ty, 34.0, 28.0);
    draw_btn(ctx, dn_r, "pdf_dn", "↓", None, false, hovered_id, pressed_id);
    hits.push(Hit { id: "pdf_dn".into(), label: "Mover página abajo".into(), rect: dn_r, act: PdfAct::MovePageDown });
    r2 -= 38.0;

    let up_r = Rect::new(r2 - 34.0, ty, 34.0, 28.0);
    draw_btn(ctx, up_r, "pdf_up", "↑", None, false, hovered_id, pressed_id);
    hits.push(Hit { id: "pdf_up".into(), label: "Mover página arriba".into(), rect: up_r, act: PdfAct::MovePageUp });
    r2 -= 38.0;

    let rot_r = Rect::new(r2 - 74.0, ty, 74.0, 28.0);
    draw_btn(ctx, rot_r, "pdf_rot", "Rotar 90°", None, false, hovered_id, pressed_id);
    hits.push(Hit { id: "pdf_rot".into(), label: "Rotar página 90 grados".into(), rect: rot_r, act: PdfAct::RotatePageCw });

    // ==================== LEFT THUMBNAIL SIDEBAR ====================
    let side_w = if sidebar_open { SIDEBAR_W } else { 0.0 };
    if sidebar_open {
        let sr = Rect::new(0.0, HEADER_TOTAL_H, SIDEBAR_W, h - HEADER_TOTAL_H);
        ctx.fill_rrect(sr, 0.0, theme::SURFACE);
        ctx.draw_line(SIDEBAR_W, HEADER_TOTAL_H, SIDEBAR_W, h, theme::BORDER, 1.0);
        ctx.push_clip(sr);

        let mut py_thumb = HEADER_TOTAL_H + 14.0 - sidebar_scroll;
        for (idx, pg) in pages.iter().enumerate() {
            let card_h = 158.0;
            let card = Rect::new(14.0, py_thumb, SIDEBAR_W - 28.0, card_h);
            if card.y + card.h >= HEADER_TOTAL_H && card.y <= h {
                let active = idx == cur_page;
                let id = format!("pdf_thumb_{}", idx);
                let hov = hovered_id == Some(id.as_str());
                ctx.fill_card(
                    card,
                    12.0,
                    if active { theme::VIOLET_TINT } else if hov { theme::ELEVATED } else { theme::CARD_BG },
                    if active { theme::VIOLET_LIGHT } else { theme::BORDER },
                );

                let ar = pg.thumb_w.max(1) as f32 / pg.thumb_h.max(1) as f32;
                let max_tw = card.w - 24.0;
                let max_th = card.h - 34.0;
                let (dw, dh) = if max_tw / max_th > ar {
                    (max_th * ar, max_th)
                } else {
                    (max_tw, max_tw / ar)
                };
                let tr = Rect::new(card.x + (card.w - dw) * 0.5, card.y + 8.0 + (max_th - dh) * 0.5, dw, dh);
                let key = format!("pdf_th_{}_{}", idx, pg.rev);
                ctx.draw_bgra_live(&key, pg.thumb_w, pg.thumb_h, &pg.thumb_bgra, tr);

                let lbl_txt = if pg.annotations.is_empty() {
                    format!("Página {}", idx + 1)
                } else {
                    format!("Página {} · {} anot.", idx + 1, pg.annotations.len())
                };
                ctx.draw_text_center(
                    &lbl_txt,
                    Rect::new(card.x, card.y + card.h - 22.0, card.w, 18.0),
                    TextStyle::Body11Med,
                    if active { theme::VIOLET_LIGHT } else { theme::TEXT_SECONDARY },
                );
                hits.push(Hit {
                    id,
                    label: format!("Ir a página {}", idx + 1),
                    rect: card,
                    act: PdfAct::SelectPage(idx),
                });
            }
            py_thumb += card_h + 10.0;
        }
        ctx.pop_clip();
    }

    // ==================== MAIN PDF CANVAS ====================
    let vp = Rect::new(side_w, HEADER_TOTAL_H, (w - side_w).max(120.0), (h - HEADER_TOTAL_H).max(120.0));
    ctx.push_clip(vp);

    if let Some(pg) = pages.get(cur_page) {
        let fit = ((vp.w - 56.0) / pg.width.max(1) as f32).min((vp.h - 44.0) / pg.height.max(1) as f32);
        let eff = if zoom > 0.0 { zoom } else { fit };
        let dw = pg.width as f32 * eff;
        let dh = pg.height as f32 * eff;
        let dx = vp.x + (vp.w - dw) * 0.5 + pan_x;
        let dy = vp.y + (vp.h - dh) * 0.5 + pan_y;
        let pr = Rect::new(dx, dy, dw, dh);
        *out_page_rect = pr;

        // Soft paper drop shadow
        ctx.fill_rrect(Rect::new(dx - 3.0, dy + 4.0, dw + 6.0, dh + 6.0), 6.0, theme::rgba(0, 0, 0, 0.52));

        if dark_reading {
            let mut inv = pg.base_bgra.clone();
            for px in inv.chunks_exact_mut(4) {
                px[0] = 248u8.saturating_sub(px[0]).max(22);
                px[1] = 244u8.saturating_sub(px[1]).max(18);
                px[2] = 242u8.saturating_sub(px[2]).max(18);
            }
            let key = format!("pdf_pg_dark_{}_{}", cur_page, pg.rev);
            ctx.draw_bgra_live(&key, pg.width, pg.height, &inv, pr);
        } else {
            let key = format!("pdf_pg_{}_{}", cur_page, pg.rev);
            ctx.draw_bgra_live(&key, pg.width, pg.height, &pg.base_bgra, pr);
        }

        // Render Vector Annotations cleanly at native screen resolution!
        for (ai, ann) in pg.annotations.iter().enumerate() {
            draw_annotation(ctx, pr, ann, editing_note_idx == Some(ai));
        }

        // Live in-progress stroke / rectangle
        let cur_rgb = PALETTE[color_idx % PALETTE.len()].0;
        if active_stroke.len() >= 2 {
            let temp = Annotation::Stroke {
                pts: active_stroke.to_vec(),
                color: cur_rgb,
                thickness_norm: if tool == PdfTool::Highlighter { 0.024 } else { 0.0045 },
                is_highlight: tool == PdfTool::Highlighter,
            };
            draw_annotation(ctx, pr, &temp, false);
        }
        if let Some(((x0, y0), (x1, y1))) = rect_drag {
            let temp = Annotation::Rect {
                x0,
                y0,
                x1,
                y1,
                color: cur_rgb,
            };
            draw_annotation(ctx, pr, &temp, false);
        }

        // Zoom badge bottom-right
        let zb = Rect::new(w - 78.0, h - 38.0, 62.0, 24.0);
        ctx.fill_card(zb, 8.0, theme::rgba(20, 20, 28, 0.88), theme::BORDER);
        ctx.draw_text_center(&format!("{}%", (eff * 100.0).round() as i32), zb, TextStyle::Mono11Med, theme::TEXT_SECONDARY);
    }

    if let Some(msg) = toast {
        let tw = (msg.len() as f32 * 6.8 + 38.0).clamp(220.0, vp.w - 40.0);
        let tr = Rect::new(vp.x + (vp.w - tw) * 0.5, vp.y + 14.0, tw, 32.0);
        ctx.fill_card(tr, 10.0, theme::rgba(24, 22, 42, 0.95), theme::VIOLET_LIGHT);
        ctx.draw_text_center(msg, tr, TextStyle::Body11Med, theme::TEXT_PRIMARY);
    }

    ctx.pop_clip();

    if corner > 0.0 {
        ctx.stroke_rrect(bounds.inset(0.5, 0.5), corner, theme::BORDER, 1.0);
        ctx.pop_layer();
    }
}

fn draw_annotation(ctx: &mut DrawCtx<'_>, pr: Rect, ann: &Annotation, is_editing: bool) {
    match ann {
        Annotation::Stroke {
            pts,
            color,
            thickness_norm,
            is_highlight,
        } => {
            let col = theme::rgba(
                color[0],
                color[1],
                color[2],
                if *is_highlight { 0.36 } else { 0.95 },
            );
            let sw = (thickness_norm * pr.w).clamp(2.0, 32.0);
            for pair in pts.windows(2) {
                ctx.draw_line(
                    pr.x + pair[0].0 * pr.w,
                    pr.y + pair[0].1 * pr.h,
                    pr.x + pair[1].0 * pr.w,
                    pr.y + pair[1].1 * pr.h,
                    col,
                    sw,
                );
            }
        }
        Annotation::Rect {
            x0,
            y0,
            x1,
            y1,
            color,
        } => {
            let rx = pr.x + x0.min(*x1) * pr.w;
            let ry = pr.y + y0.min(*y1) * pr.h;
            let rw = (x1 - x0).abs() * pr.w;
            let rh = (y1 - y0).abs() * pr.h;
            let col = theme::rgb(color[0], color[1], color[2]);
            ctx.stroke_rrect(Rect::new(rx, ry, rw.max(2.0), rh.max(2.0)), 4.0, col, 2.4);
        }
        Annotation::Note { x, y, text, color } => {
            let nx = pr.x + *x * pr.w;
            let ny = pr.y + *y * pr.h;
            let disp = if is_editing {
                format!("{}|", text)
            } else if text.is_empty() {
                "Nota...".into()
            } else {
                text.clone()
            };
            let nw = (disp.len() as f32 * 6.8 + 24.0).clamp(110.0, 320.0);
            let nr = Rect::new(nx, ny, nw, 30.0);
            ctx.fill_card(
                nr,
                8.0,
                theme::rgba(18, 18, 26, 0.92),
                theme::rgb(color[0], color[1], color[2]),
            );
            ctx.draw_text(&disp, nr.inset(10.0, 6.0), TextStyle::Body11Med, theme::CRISP_WHITE);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_btn(
    ctx: &mut DrawCtx<'_>,
    r: Rect,
    id: &str,
    label: &str,
    icon: Option<VectorIcon>,
    active: bool,
    hovered_id: Option<&str>,
    pressed_id: Option<&str>,
) {
    let hov = hovered_id == Some(id);
    let prs = pressed_id == Some(id);
    let draw_r = if prs { r.inset(1.0, 1.0) } else { r };
    let bg = if active {
        theme::VIOLET_TINT
    } else if hov {
        theme::ELEVATED_HOVER
    } else {
        theme::ELEVATED
    };
    let border = if active { theme::VIOLET_LIGHT } else { theme::BORDER };
    ctx.fill_card(draw_r, 8.0, bg, border);
    if let Some(ic) = icon {
        ctx.draw_vector_icon(
            ic,
            draw_r.x + draw_r.w * 0.5,
            draw_r.y + draw_r.h * 0.5,
            13.0,
            if active { theme::VIOLET_LIGHT } else { theme::TEXT_PRIMARY },
        );
    } else {
        ctx.draw_text_center(
            label,
            draw_r,
            TextStyle::Body11Med,
            if active { theme::VIOLET_LIGHT } else { theme::TEXT_PRIMARY },
        );
    }
}

fn draw_primary_btn(
    ctx: &mut DrawCtx<'_>,
    r: Rect,
    id: &str,
    label: &str,
    hovered_id: Option<&str>,
    pressed_id: Option<&str>,
) {
    let hov = hovered_id == Some(id);
    let prs = pressed_id == Some(id);
    let draw_r = if prs { r.inset(1.0, 1.0) } else { r };
    ctx.fill_card(
        draw_r,
        9.0,
        if hov { theme::VIOLET_LIGHT } else { theme::VIOLET },
        theme::VIOLET_BORDER,
    );
    ctx.draw_text_center(label, draw_r, TextStyle::Body11Med, theme::CRISP_WHITE);
}

unsafe extern "system" fn pdf_wnd_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
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
        return aura_window::handle_getminmaxinfo(hwnd, lparam, 760, 480);
    }
    if msg == WM_NCHITTEST {
        return aura_window::handle_nchittest(hwnd, lparam, TOP_BAR_H, |lx, ly| {
            PDF_RT.with(|c| {
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

    let mut start_drag = false;
    let mut deferred_max: Option<bool> = None;
    let mut deferred_min = false;
    let mut deferred_snap = false;

    let handled = PDF_RT.with(|cell| {
        let Ok(mut opt) = cell.try_borrow_mut() else { return false };
        let Some(rt) = opt.as_mut() else { return false };

        match msg {
            WM_TIMER if wparam.0 == TIMER_IDLE => {
                if rt.toast.as_ref().map(|(_, t)| t.elapsed().as_secs_f32() > 3.2).unwrap_or(false) {
                    rt.redraw();
                }
                true
            }
            WM_SIZE => {
                let mut rc = RECT::default();
                if GetClientRect(hwnd, &mut rc).is_ok() {
                    let rw = (rc.right - rc.left).max(400) as u32;
                    let rh = (rc.bottom - rc.top).max(300) as u32;
                    rt.maximized = IsZoomed(hwnd).as_bool();
                    rt.width = rw as f32;
                    rt.height = rh as f32;
                    rt.surf.resize(&rt.gpu, rw, rh);
                    rt.redraw();
                }
                true
            }
            WM_MOUSEMOVE => {
                let px = (lparam.0 & 0xFFFF) as i16 as f32;
                let py = ((lparam.0 >> 16) & 0xFFFF) as i16 as f32;

                if !rt.active_stroke.is_empty() {
                    if let Some(pt) = rt.screen_to_page_norm(px, py) {
                        rt.active_stroke.push(pt);
                        rt.redraw();
                    }
                    return true;
                }
                if let Some((start, _)) = rt.rect_drag {
                    if let Some(end) = rt.screen_to_page_norm(px, py) {
                        rt.rect_drag = Some((start, end));
                        rt.redraw();
                    }
                    return true;
                }
                if rt.panning {
                    rt.pan_x += px - rt.pan_last.0;
                    rt.pan_y += py - rt.pan_last.1;
                    rt.pan_last = (px, py);
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
                    rt.pressed_id = Some(hit.id.clone());
                    match hit.act {
                        PdfAct::WinMax => {
                            rt.maximized = !rt.maximized;
                            deferred_max = Some(rt.maximized);
                        }
                        PdfAct::WinMin => {
                            deferred_min = true;
                        }
                        other => rt.execute(other),
                    }
                    rt.pressed_id = None;
                    return true;
                }
                if py < TOP_BAR_H {
                    start_drag = true;
                    return true;
                }

                if let Some(norm) = rt.screen_to_page_norm(px, py) {
                    match rt.tool {
                        PdfTool::Highlighter | PdfTool::PenSign => {
                            rt.active_stroke.clear();
                            rt.active_stroke.push(norm);
                            SetCapture(hwnd);
                        }
                        PdfTool::RectBox => {
                            rt.rect_drag = Some((norm, norm));
                            SetCapture(hwnd);
                        }
                        PdfTool::TextNote => {
                            let col = PALETTE[rt.color_idx % PALETTE.len()].0;
                            if let Some(pg) = rt.pages.get_mut(rt.cur_page) {
                                pg.annotations.push(Annotation::Note {
                                    x: norm.0,
                                    y: norm.1,
                                    text: String::new(),
                                    color: col,
                                });
                                rt.editing_note_idx = Some(pg.annotations.len() - 1);
                                rt.set_toast("Escribe tu nota y pulsa Enter");
                                rt.redraw();
                            }
                        }
                        PdfTool::PanSelect => {
                            rt.editing_note_idx = None;
                            rt.panning = true;
                            rt.pan_last = (px, py);
                            SetCapture(hwnd);
                        }
                    }
                }
                true
            }
            WM_LBUTTONUP => {
                if !rt.active_stroke.is_empty() {
                    let pts = std::mem::take(&mut rt.active_stroke);
                    if pts.len() >= 2 {
                        let col = PALETTE[rt.color_idx % PALETTE.len()].0;
                        let is_hl = rt.tool == PdfTool::Highlighter;
                        if let Some(pg) = rt.pages.get_mut(rt.cur_page) {
                            pg.annotations.push(Annotation::Stroke {
                                pts,
                                color: col,
                                thickness_norm: if is_hl { 0.024 } else { 0.0045 },
                                is_highlight: is_hl,
                            });
                        }
                    }
                    let _ = ReleaseCapture();
                    rt.redraw();
                } else if let Some(((x0, y0), (x1, y1))) = rt.rect_drag.take() {
                    if (x1 - x0).abs() > 0.01 && (y1 - y0).abs() > 0.01 {
                        let col = PALETTE[rt.color_idx % PALETTE.len()].0;
                        if let Some(pg) = rt.pages.get_mut(rt.cur_page) {
                            pg.annotations.push(Annotation::Rect {
                                x0,
                                y0,
                                x1,
                                y1,
                                color: col,
                            });
                        }
                    }
                    let _ = ReleaseCapture();
                    rt.redraw();
                } else if rt.panning {
                    rt.panning = false;
                    let _ = ReleaseCapture();
                }
                true
            }
            WM_RBUTTONDOWN => {
                let px = (lparam.0 & 0xFFFF) as i16 as f32;
                let py = ((lparam.0 >> 16) & 0xFFFF) as i16 as f32;
                if let Some(hit) = rt.hits.iter().rev().find(|h| h.rect.contains(px, py)) {
                    if matches!(hit.act, PdfAct::WinMax) {
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
                }
                true
            }
            WM_MOUSEWHEEL => {
                let px = (lparam.0 & 0xFFFF) as i16 as f32;
                let delta = ((wparam.0 >> 16) as i16) as f32 / 120.0;
                if rt.sidebar_open && px < SIDEBAR_W {
                    let max_s = ((rt.pages.len() as f32 * 168.0) - (rt.height - HEADER_TOTAL_H)).max(0.0);
                    rt.sidebar_scroll = (rt.sidebar_scroll - delta * 64.0).clamp(0.0, max_s);
                    rt.redraw();
                } else if delta > 0.0 {
                    rt.execute(PdfAct::ZoomIn);
                } else {
                    rt.execute(PdfAct::ZoomOut);
                }
                true
            }
            WM_CHAR => {
                if let Some(nidx) = rt.editing_note_idx {
                    let ch = char::from_u32(wparam.0 as u32).unwrap_or('\0');
                    if !ch.is_control() {
                        if let Some(pg) = rt.pages.get_mut(rt.cur_page) {
                            if let Some(Annotation::Note { text, .. }) = pg.annotations.get_mut(nidx) {
                                text.push(ch);
                                rt.redraw();
                            }
                        }
                    }
                    return true;
                }
                false
            }
            WM_KEYDOWN => {
                let vk = wparam.0 as u16;
                if let Some(nidx) = rt.editing_note_idx {
                    if vk == VK_RETURN.0 || vk == VK_ESCAPE.0 {
                        rt.editing_note_idx = None;
                        rt.redraw();
                    } else if vk == VK_BACK.0 {
                        if let Some(pg) = rt.pages.get_mut(rt.cur_page) {
                            if let Some(Annotation::Note { text, .. }) = pg.annotations.get_mut(nidx) {
                                text.pop();
                                rt.redraw();
                            }
                        }
                    }
                    return true;
                }
                if vk == VK_PRIOR.0 || vk == VK_LEFT.0 {
                    rt.execute(PdfAct::PrevPage);
                } else if vk == VK_NEXT.0 || vk == VK_RIGHT.0 {
                    rt.execute(PdfAct::NextPage);
                } else if vk == VK_OEM_PLUS.0 {
                    rt.execute(PdfAct::ZoomIn);
                } else if vk == VK_OEM_MINUS.0 {
                    rt.execute(PdfAct::ZoomOut);
                } else if vk == VK_DELETE.0 {
                    rt.execute(PdfAct::UndoAnnot);
                } else if vk == VK_ESCAPE.0 {
                    rt.execute(PdfAct::WinClose);
                }
                true
            }
            _ => false,
        }
    });

    if start_drag {
        let _ = ReleaseCapture();
        let _ = SendMessageW(hwnd, WM_NCLBUTTONDOWN, WPARAM(HTCAPTION as usize), LPARAM(0));
    } else if deferred_min {
        if !win32_shell::request_genie_minimize(hwnd) {
            let _ = ShowWindow(hwnd, SW_MINIMIZE);
        }
    } else if deferred_snap {
        aura_window::cycle_snap_window(hwnd);
    } else if let Some(m) = deferred_max {
        let _ = ShowWindow(hwnd, if m { SW_MAXIMIZE } else { SW_RESTORE });
    }

    if handled {
        LRESULT(0)
    } else {
        DefWindowProcW(hwnd, msg, wparam, lparam)
    }
}
