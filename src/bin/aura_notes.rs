//! AuraNotes v4.0 — Pure Rust + Direct2D / DirectComposition Instant Text & Code Editor
//! - Launches in <35ms, replaces slow Windows Notepad with Drool Dark Mode precision
//! - Line numbers, subtle syntax tinting, live editing, Ctrl+S save, Ctrl+O open, Ctrl+N new,
//!   Ctrl+F inline search with match highlighting, and instant clean exit!

#![windows_subsystem = "windows"]

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
    GetKeyState, ReleaseCapture, VK_BACK, VK_CONTROL, VK_DELETE, VK_DOWN, VK_END, VK_ESCAPE,
    VK_HOME, VK_LEFT, VK_NEXT, VK_PRIOR, VK_RETURN, VK_RIGHT, VK_TAB, VK_UP,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, GetClientRect, GetMessageW, GetWindowRect,
    IsZoomed, LoadCursorW, PostQuitMessage, RegisterClassW, SendMessageW, SetTimer, ShowWindow,
    TranslateMessage, HTBOTTOM, HTBOTTOMLEFT, HTBOTTOMRIGHT, HTCAPTION, HTCLIENT, HTLEFT, HTRIGHT,
    HTTOP, HTTOPLEFT, HTTOPRIGHT, IDC_ARROW, MSG, SW_HIDE, SW_MAXIMIZE, SW_MINIMIZE, SW_RESTORE,
    SW_SHOW, WM_CHAR, WM_CLOSE, WM_DESTROY, WM_KEYDOWN, WM_LBUTTONDBLCLK, WM_LBUTTONDOWN,
    WM_MOUSEMOVE, WM_MOUSEWHEEL, WM_NCHITTEST, WM_NCLBUTTONDOWN, WM_SIZE, WM_TIMER, WNDCLASSW,
    WS_EX_NOREDIRECTIONBITMAP, WS_MAXIMIZEBOX, WS_MINIMIZEBOX, WS_POPUP, WS_SYSMENU,
};

const TOP_BAR_H: f32 = 44.0;
const STATUS_BAR_H: f32 = 28.0;
const GUTTER_W: f32 = 54.0;
const LINE_H: f32 = 21.0;
const CHAR_W: f32 = 6.75; // Consolas 12pt monospace cell width
const TIMER_BLINK: usize = 40;

#[derive(Clone)]
enum NoteAct {
    WinClose,
    WinMin,
    WinMax,
    NewFile,
    OpenFile,
    SaveFile,
    ToggleFind,
    FindNext,
}

#[derive(Clone)]
struct Hit {
    id: String,
    rect: Rect,
    act: NoteAct,
}

struct NotesRuntime {
    hwnd: HWND,
    gpu: GpuEngine,
    surf: WindowSurface,
    width: f32,
    height: f32,
    maximized: bool,

    file_path: Option<PathBuf>,
    lines: Vec<String>,
    cur_line: usize,
    cur_col: usize, // char index
    scroll_line: usize,
    modified: bool,

    find_open: bool,
    find_query: String,
    caret_on: bool,
    hovered_id: Option<String>,
    hits: Vec<Hit>,
    toast: Option<(String, Instant)>,
}

thread_local! {
    static NOTES_RT: RefCell<Option<NotesRuntime>> = const { RefCell::new(None) };
}

fn ctrl_down() -> bool {
    unsafe { (GetKeyState(VK_CONTROL.0 as i32) as u16 & 0x8000) != 0 }
}

fn char_to_byte(s: &str, ch_idx: usize) -> usize {
    s.char_indices().nth(ch_idx).map(|(b, _)| b).unwrap_or(s.len())
}

fn lang_badge(path: Option<&PathBuf>) -> &'static str {
    let ext = path
        .and_then(|p| p.extension())
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();
    match ext.as_str() {
        "rs" => "RUST",
        "json" => "JSON",
        "md" => "MARKDOWN",
        "ps1" => "POWERSHELL",
        "py" => "PYTHON",
        "js" | "jsx" => "JAVASCRIPT",
        "ts" | "tsx" => "TYPESCRIPT",
        "cs" => "C#",
        "html" => "HTML",
        "css" => "CSS",
        "toml" => "TOML",
        "yaml" | "yml" => "YAML",
        "ini" | "cfg" => "CONFIG",
        "log" => "LOG",
        "csv" => "CSV",
        _ => "TEXTO",
    }
}

fn line_syntax_color(line: &str) -> windows::Win32::Graphics::Direct2D::Common::D2D1_COLOR_F {
    let t = line.trim_start();
    if t.starts_with("//") || t.starts_with('#') && !t.starts_with("# ") && !t.starts_with("##") || t.starts_with("--") {
        return theme::TEXT_TERTIARY;
    }
    if t.starts_with("# ") || t.starts_with("##") || t.starts_with('[') && t.ends_with(']') {
        return theme::VIOLET_LIGHT;
    }
    if t.starts_with('"') && t.contains("\":") {
        return theme::rgba(165, 214, 255, 1.0);
    }
    theme::TEXT_PRIMARY
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() >= 3 && args[1].eq_ignore_ascii_case("--capture") {
        let out_path = PathBuf::from(&args[2]);
        let sample = args.get(3).map(PathBuf::from).or_else(find_sample_text);
        run_capture(&out_path, sample);
        return;
    }

    let initial_file = args.get(1).map(PathBuf::from).filter(|p| p.exists());

    let gpu = match GpuEngine::new() {
        Ok(g) => g,
        Err(_) => return,
    };

    let (sw, sh) = win32_shell::screen_size();
    let w = (sw as f32 * 0.56).clamp(800.0, 1140.0) as i32;
    let h = (sh as f32 * 0.72).clamp(580.0, 820.0) as i32;
    let x = (sw - w) / 2;
    let y = (sh - h) / 2;

    let cls = win32_shell::to_wide("AuraNotes_Rust_v4_Wnd");
    let title = win32_shell::to_wide("AuraNotes");
    let hwnd = unsafe {
        let hinst = GetModuleHandleW(None).unwrap_or_default();
        let wc = WNDCLASSW {
            lpfnWndProc: Some(notes_wnd_proc),
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
            WS_POPUP | WS_MINIMIZEBOX | WS_MAXIMIZEBOX | WS_SYSMENU,
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

    let Ok(surf) = WindowSurface::new(&gpu, hwnd, w as u32, h as u32) else {
        return;
    };

    let mut rt = NotesRuntime {
        hwnd,
        gpu,
        surf,
        width: w as f32,
        height: h as f32,
        maximized: false,
        file_path: None,
        lines: vec![String::new()],
        cur_line: 0,
        cur_col: 0,
        scroll_line: 0,
        modified: false,
        find_open: false,
        find_query: String::new(),
        caret_on: true,
        hovered_id: None,
        hits: Vec::new(),
        toast: None,
    };

    if let Some(p) = initial_file {
        rt.load_file(&p);
    }
    rt.redraw();

    unsafe {
        let _ = ShowWindow(hwnd, SW_SHOW);
        SetTimer(hwnd, TIMER_BLINK, 540, None);
    }

    NOTES_RT.with(|c| *c.borrow_mut() = Some(rt));

    unsafe {
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, HWND::default(), 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
    NOTES_RT.with(|c| {
        if let Ok(mut g) = c.try_borrow_mut() {
            *g = None;
        }
    });
}

fn find_sample_text() -> Option<PathBuf> {
    let p1 = aura_os::models::app_data_dir().join("settings_v3.json");
    if p1.exists() {
        return Some(p1);
    }
    None
}

fn run_capture(out_path: &Path, sample: Option<PathBuf>) {
    let Ok(mut gpu) = GpuEngine::new() else { return };
    let lines: Vec<String> = sample
        .as_ref()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .map(|s| s.trim_start_matches('\u{FEFF}').lines().map(|l| l.to_string()).collect())
        .unwrap_or_else(|| vec!["// AuraNotes v4.0 — Editor nativo en Rust + Direct2D".into()]);

    let _ = gpu.render_to_png(1040, 700, out_path, |ctx| {
        let mut hits = Vec::new();
        render_notes_ui(
            ctx,
            1040.0,
            700.0,
            false,
            sample.as_ref(),
            &lines,
            2.min(lines.len().saturating_sub(1)),
            4,
            0,
            false,
            false,
            "",
            true,
            None,
            None,
            &mut hits,
        );
    });
}

impl NotesRuntime {
    fn load_file(&mut self, path: &Path) {
        let raw = std::fs::read(path).unwrap_or_default();
        // Cap single view smoothly at 2 MB to stay instant even on giant log files
        let slice = if raw.len() > 2_000_000 { &raw[..2_000_000] } else { &raw[..] };
        let text = String::from_utf8_lossy(slice).trim_start_matches('\u{FEFF}').replace('\r', "");
        let mut lines: Vec<String> = text.split('\n').map(|s| s.to_string()).collect();
        if lines.is_empty() {
            lines.push(String::new());
        }
        self.file_path = Some(path.to_path_buf());
        self.lines = lines;
        self.cur_line = 0;
        self.cur_col = 0;
        self.scroll_line = 0;
        self.modified = false;
    }

    fn save_current(&mut self) {
        let target = if let Some(p) = self.file_path.clone() {
            p
        } else {
            let docs = std::env::var("USERPROFILE")
                .map(|u| PathBuf::from(u).join("Documents"))
                .unwrap_or_else(|_| aura_os::models::app_data_dir());
            let ts = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            let p = docs.join(format!("Nota_Aura_{}.txt", ts));
            self.file_path = Some(p.clone());
            p
        };
        let joined = self.lines.join("\r\n");
        if std::fs::write(&target, joined.as_bytes()).is_ok() {
            self.modified = false;
            let fname = target.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
            self.toast = Some((format!("Guardado: {}", fname), Instant::now()));
        } else {
            self.toast = Some(("No se pudo guardar el archivo".into(), Instant::now()));
        }
    }

    fn visible_lines(&self) -> usize {
        let find_h = if self.find_open { 40.0 } else { 0.0 };
        let body_h = (self.height - TOP_BAR_H - STATUS_BAR_H - find_h - 16.0).max(80.0);
        (body_h / LINE_H).floor().max(1.0) as usize
    }

    fn ensure_cursor_visible(&mut self) {
        let vis = self.visible_lines();
        if self.cur_line < self.scroll_line {
            self.scroll_line = self.cur_line;
        } else if self.cur_line >= self.scroll_line + vis {
            self.scroll_line = self.cur_line.saturating_sub(vis - 1);
        }
    }

    fn find_next(&mut self) {
        let q = self.find_query.to_lowercase();
        if q.is_empty() || self.lines.is_empty() {
            return;
        }
        let n = self.lines.len();
        for step in 1..=n {
            let idx = (self.cur_line + step) % n;
            if let Some(pos) = self.lines[idx].to_lowercase().find(&q) {
                self.cur_line = idx;
                self.cur_col = self.lines[idx][..pos].chars().count();
                self.ensure_cursor_visible();
                break;
            }
        }
    }

    fn execute(&mut self, act: NoteAct) {
        match act {
            NoteAct::WinClose => unsafe {
                let _ = ShowWindow(self.hwnd, SW_HIDE);
                PostQuitMessage(0);
                return;
            },
            NoteAct::WinMin => unsafe { let _ = ShowWindow(self.hwnd, SW_MINIMIZE); },
            NoteAct::WinMax => unsafe {
                self.maximized = !self.maximized;
                let _ = ShowWindow(self.hwnd, if self.maximized { SW_MAXIMIZE } else { SW_RESTORE });
            },
            NoteAct::NewFile => {
                self.file_path = None;
                self.lines = vec![String::new()];
                self.cur_line = 0;
                self.cur_col = 0;
                self.scroll_line = 0;
                self.modified = false;
            }
            NoteAct::OpenFile => {
                if let Some(p) = win32_shell::pick_file_dialog(
                    "Abrir archivo de texto o código en AuraNotes",
                    "Texto y Código (*.txt;*.md;*.json;*.log;*.ps1;*.rs;*.py;*.js;*.ini)",
                    "*.txt;*.md;*.json;*.log;*.ini;*.cfg;*.toml;*.yaml;*.ps1;*.rs;*.py;*.js;*.ts;*.cs;*.html;*.css;*.*",
                ) {
                    self.load_file(Path::new(&p));
                }
            }
            NoteAct::SaveFile => self.save_current(),
            NoteAct::ToggleFind => {
                self.find_open = !self.find_open;
            }
            NoteAct::FindNext => self.find_next(),
        }
        self.redraw();
    }

    fn redraw(&mut self) {
        if let Some((_, ts)) = &self.toast {
            if ts.elapsed().as_secs_f32() > 2.8 {
                self.toast = None;
            }
        }
        let w = self.width;
        let h = self.height;
        let maxd = self.maximized;
        let fp = self.file_path.as_ref();
        let lines = &self.lines;
        let cl = self.cur_line;
        let cc = self.cur_col;
        let sl = self.scroll_line;
        let modif = self.modified;
        let fopen = self.find_open;
        let fq = &self.find_query;
        let caret = self.caret_on;
        let hov = self.hovered_id.as_deref();
        let toast = self.toast.as_ref().map(|(s, _)| s.as_str());
        let hits = &mut self.hits;

        self.surf.draw(&mut self.gpu, |ctx| {
            render_notes_ui(
                ctx, w, h, maxd, fp, lines, cl, cc, sl, modif, fopen, fq, caret, hov, toast, hits,
            );
        });
    }
}

#[allow(clippy::too_many_arguments)]
fn render_notes_ui(
    ctx: &mut DrawCtx<'_>,
    w: f32,
    h: f32,
    maximized: bool,
    file_path: Option<&PathBuf>,
    lines: &[String],
    cur_line: usize,
    cur_col: usize,
    scroll_line: usize,
    modified: bool,
    find_open: bool,
    find_query: &str,
    caret_on: bool,
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

    ctx.fill_rrect(bounds, corner, theme::CANVAS);

    // Top Header Bar
    ctx.fill_rrect(Rect::new(0.0, 0.0, w, TOP_BAR_H), 0.0, theme::SURFACE);
    ctx.draw_line(0.0, TOP_BAR_H, w, TOP_BAR_H, theme::BORDER, 1.0);

    let lights = [
        ("nt_close", 20.0, theme::CORAL, "×", NoteAct::WinClose),
        ("nt_min", 40.0, theme::AMBER, "−", NoteAct::WinMin),
        ("nt_max", 60.0, theme::EMERALD, "+", NoteAct::WinMax),
    ];
    let any_light_hov = matches!(hovered_id, Some("nt_close") | Some("nt_min") | Some("nt_max"));
    for (id, cx, col, glyph, act) in lights {
        ctx.fill_circle(cx, 22.0, if any_light_hov { 6.3 } else { 5.8 }, col);
        if any_light_hov {
            ctx.draw_text_center(glyph, Rect::new(cx - 6.0, 15.0, 12.0, 13.0), TextStyle::Small9Reg, theme::DARK_INK);
        }
        hits.push(Hit { id: id.into(), rect: Rect::new(cx - 8.0, 13.0, 16.0, 18.0), act });
    }

    let fname = file_path
        .and_then(|p| p.file_name())
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "Sin título.txt".to_string());
    let title = if modified { format!("● {}", fname) } else { fname };
    ctx.draw_vector_icon(VectorIcon::FileText, 90.0, 22.0, 13.0, theme::VIOLET_LIGHT);
    ctx.draw_text(&title, Rect::new(104.0, 11.0, (w * 0.42).max(180.0), 22.0), TextStyle::Body12Semi, if modified { theme::AMBER } else { theme::TEXT_PRIMARY });

    // Top-Right Toolbar Pills: Guardar · Buscar · Abrir · Nuevo
    let mut rx = w - 14.0;
    let btns = [
        ("nt_save", "Guardar", true, NoteAct::SaveFile),
        ("nt_find", "Buscar", false, NoteAct::ToggleFind),
        ("nt_open", "Abrir...", false, NoteAct::OpenFile),
        ("nt_new", "Nuevo", false, NoteAct::NewFile),
    ];
    for (id, lbl, accent, act) in btns {
        let bw = (lbl.len() as f32 * 7.2 + 22.0).max(66.0);
        rx -= bw;
        let br = Rect::new(rx, 8.0, bw, 28.0);
        let hov = hovered_id == Some(id);
        let bg = if accent {
            if hov { theme::VIOLET_LIGHT } else { theme::VIOLET }
        } else if hov {
            theme::ELEVATED_HOVER
        } else {
            theme::ELEVATED
        };
        ctx.fill_card(br, 9.0, bg, if accent { theme::VIOLET_BORDER } else { theme::BORDER });
        ctx.draw_text_center(lbl, br, TextStyle::Body11Med, theme::TEXT_PRIMARY);
        hits.push(Hit { id: id.into(), rect: br, act });
        rx -= 6.0;
    }

    // Optional Inline Find Bar
    let mut body_y = TOP_BAR_H;
    if find_open {
        let fr = Rect::new(0.0, body_y, w, 40.0);
        ctx.fill_rrect(fr, 0.0, theme::rgba(20, 20, 30, 1.0));
        ctx.draw_line(0.0, body_y + 40.0, w, body_y + 40.0, theme::BORDER, 1.0);
        let box_r = Rect::new(16.0, body_y + 6.0, 300.0, 28.0);
        ctx.fill_card(box_r, 8.0, theme::CARD_BG, theme::VIOLET_BORDER);
        let disp = if find_query.is_empty() { "Escribe para buscar (Enter siguiente)..." } else { find_query };
        ctx.draw_text(
            disp,
            Rect::new(box_r.x + 10.0, box_r.y + 5.0, box_r.w - 20.0, 18.0),
            TextStyle::Body11Reg,
            if find_query.is_empty() { theme::TEXT_TERTIARY } else { theme::TEXT_PRIMARY },
        );
        let next_r = Rect::new(326.0, body_y + 6.0, 82.0, 28.0);
        ctx.fill_card(next_r, 8.0, theme::ELEVATED, theme::BORDER);
        ctx.draw_text_center("Siguiente", next_r, TextStyle::Body11Med, theme::TEXT_PRIMARY);
        hits.push(Hit { id: "nt_fnext".into(), rect: next_r, act: NoteAct::FindNext });
        body_y += 40.0;
    }

    // ==================== MONOSPACE EDITOR BODY ====================
    let body_h = (h - body_y - STATUS_BAR_H).max(80.0);
    let editor_r = Rect::new(0.0, body_y, w, body_h);
    ctx.push_clip(editor_r);

    // Gutter background
    ctx.fill_rrect(Rect::new(0.0, body_y, GUTTER_W, body_h), 0.0, theme::rgba(16, 16, 23, 1.0));
    ctx.draw_line(GUTTER_W, body_y, GUTTER_W, body_y + body_h, theme::BORDER, 1.0);

    let vis = ((body_h - 12.0) / LINE_H).ceil() as usize + 1;
    let end = (scroll_line + vis).min(lines.len());
    let q_low = find_query.to_lowercase();

    for i in scroll_line..end {
        let ly = body_y + 8.0 + (i - scroll_line) as f32 * LINE_H;
        let is_cur = i == cur_line;
        if is_cur {
            ctx.fill_rrect(Rect::new(GUTTER_W + 1.0, ly, w - GUTTER_W - 2.0, LINE_H), 0.0, theme::rgba(124, 92, 255, 0.11));
        }

        // Line number
        let ln_str = format!("{}", i + 1);
        ctx.draw_text_right(
            &ln_str,
            Rect::new(4.0, ly + 2.0, GUTTER_W - 14.0, LINE_H - 2.0),
            TextStyle::Mono11Med,
            if is_cur { theme::VIOLET_LIGHT } else { theme::TEXT_TERTIARY },
        );

        let line_txt = &lines[i];
        // Search match highlight
        if find_open && !q_low.is_empty() {
            if let Some(pos) = line_txt.to_lowercase().find(&q_low) {
                let start_ch = line_txt[..pos].chars().count() as f32;
                let len_ch = find_query.chars().count() as f32;
                ctx.fill_rrect(
                    Rect::new(GUTTER_W + 12.0 + start_ch * CHAR_W, ly + 1.0, (len_ch * CHAR_W).max(8.0), LINE_H - 2.0),
                    3.0,
                    theme::rgba(245, 158, 11, 0.32),
                );
            }
        }

        ctx.draw_text(
            line_txt,
            Rect::new(GUTTER_W + 12.0, ly + 1.0, w - GUTTER_W - 24.0, LINE_H),
            TextStyle::Mono11Med,
            line_syntax_color(line_txt),
        );

        if is_cur && caret_on && !find_open {
            let cx = GUTTER_W + 12.0 + (cur_col as f32 * CHAR_W);
            ctx.fill_rrect(Rect::new(cx, ly + 2.0, 2.0, LINE_H - 4.0), 1.0, theme::VIOLET_LIGHT);
        }
    }

    if let Some(msg) = toast {
        let tw = (msg.len() as f32 * 7.0 + 34.0).clamp(160.0, 420.0);
        let tr = Rect::new(w - tw - 18.0, body_y + 12.0, tw, 30.0);
        ctx.fill_card(tr, 9.0, theme::rgba(22, 34, 28, 0.94), theme::EMERALD);
        ctx.draw_text_center(msg, tr, TextStyle::Body11Med, theme::TEXT_PRIMARY);
    }
    ctx.pop_clip();

    // ==================== BOTTOM STATUS BAR ====================
    let sy = h - STATUS_BAR_H;
    ctx.fill_rrect(Rect::new(0.0, sy, w, STATUS_BAR_H), 0.0, theme::SURFACE);
    ctx.draw_line(0.0, sy, w, sy, theme::BORDER, 1.0);

    let left_st = format!(
        "Ln {}, Col {}  ·  {} líneas",
        cur_line + 1,
        cur_col + 1,
        lines.len()
    );
    ctx.draw_text(&left_st, Rect::new(14.0, sy + 5.0, 320.0, 18.0), TextStyle::Mono11Med, theme::TEXT_SECONDARY);
    let right_st = format!("{}  ·  UTF-8  ·  AuraNotes v4.0", lang_badge(file_path));
    ctx.draw_text_right(&right_st, Rect::new(w - 300.0, sy + 5.0, 286.0, 18.0), TextStyle::Mono11Med, theme::TEXT_TERTIARY);

    if corner > 0.0 {
        ctx.stroke_rrect(bounds.inset(0.5, 0.5), corner, theme::BORDER, 1.0);
        ctx.pop_layer();
    }
}

unsafe extern "system" fn notes_wnd_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if msg == WM_CLOSE {
        let _ = ShowWindow(hwnd, SW_HIDE);
        PostQuitMessage(0);
        return LRESULT(0);
    }
    if msg == WM_DESTROY {
        PostQuitMessage(0);
        return LRESULT(0);
    }
    if msg == WM_NCHITTEST {
        let is_max = NOTES_RT.with(|c| c.try_borrow().ok().and_then(|g| g.as_ref().map(|r| r.maximized)).unwrap_or(false));
        if !is_max {
            let mut wr = RECT::default();
            if GetWindowRect(hwnd, &mut wr).is_ok() {
                let sx = (lparam.0 & 0xFFFF) as i16 as i32;
                let sy = ((lparam.0 >> 16) & 0xFFFF) as i16 as i32;
                let b = 7;
                let l = sx < wr.left + b;
                let r = sx >= wr.right - b;
                let t = sy < wr.top + b;
                let bt = sy >= wr.bottom - b;
                if t && l { return LRESULT(HTTOPLEFT as isize); }
                if t && r { return LRESULT(HTTOPRIGHT as isize); }
                if bt && l { return LRESULT(HTBOTTOMLEFT as isize); }
                if bt && r { return LRESULT(HTBOTTOMRIGHT as isize); }
                if l { return LRESULT(HTLEFT as isize); }
                if r { return LRESULT(HTRIGHT as isize); }
                if t { return LRESULT(HTTOP as isize); }
                if bt { return LRESULT(HTBOTTOM as isize); }
            }
        }
        return LRESULT(HTCLIENT as isize);
    }

    let mut start_title_drag = false;
    let mut deferred_max: Option<bool> = None;
    let mut deferred_min = false;

    let handled = NOTES_RT.with(|cell| {
        let Ok(mut opt) = cell.try_borrow_mut() else { return false };
        let Some(rt) = opt.as_mut() else { return false };

        match msg {
            WM_TIMER if wparam.0 == TIMER_BLINK => {
                rt.caret_on = !rt.caret_on;
                rt.redraw();
                true
            }
            WM_SIZE => {
                let mut rc = RECT::default();
                if GetClientRect(hwnd, &mut rc).is_ok() {
                    let raw_w = rc.right - rc.left;
                    let raw_h = rc.bottom - rc.top;
                    if raw_w > 0 && raw_h > 0 {
                        let nw = raw_w.max(460) as u32;
                        let nh = raw_h.max(320) as u32;
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
                        NoteAct::WinMax => {
                            rt.maximized = !rt.maximized;
                            deferred_max = Some(rt.maximized);
                        }
                        NoteAct::WinMin => {
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
                let body_y = TOP_BAR_H + if rt.find_open { 40.0 } else { 0.0 };
                if py >= body_y && py < rt.height - STATUS_BAR_H {
                    let clicked_ln = rt.scroll_line + (((py - body_y - 8.0).max(0.0) / LINE_H) as usize);
                    rt.cur_line = clicked_ln.min(rt.lines.len().saturating_sub(1));
                    let ch_est = (((px - GUTTER_W - 12.0).max(0.0) / CHAR_W).round()) as usize;
                    let max_c = rt.lines[rt.cur_line].chars().count();
                    rt.cur_col = ch_est.min(max_c);
                    rt.caret_on = true;
                    rt.redraw();
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
                let delta = ((wparam.0 >> 16) as i16) as i32;
                let max_s = rt.lines.len().saturating_sub(rt.visible_lines());
                if delta > 0 {
                    rt.scroll_line = rt.scroll_line.saturating_sub(3);
                } else {
                    rt.scroll_line = (rt.scroll_line + 3).min(max_s);
                }
                rt.redraw();
                true
            }
            WM_KEYDOWN => {
                let vk = wparam.0 as u16;
                let ctrl = ctrl_down();
                if ctrl && vk == b'S' as u16 {
                    rt.execute(NoteAct::SaveFile);
                    return true;
                }
                if ctrl && vk == b'O' as u16 {
                    rt.execute(NoteAct::OpenFile);
                    return true;
                }
                if ctrl && vk == b'N' as u16 {
                    rt.execute(NoteAct::NewFile);
                    return true;
                }
                if ctrl && vk == b'F' as u16 {
                    rt.execute(NoteAct::ToggleFind);
                    return true;
                }
                if vk == VK_ESCAPE.0 {
                    if rt.find_open {
                        rt.find_open = false;
                        rt.redraw();
                    }
                    return true;
                }
                if rt.find_open {
                    if vk == VK_RETURN.0 {
                        rt.find_next();
                        rt.redraw();
                    } else if vk == VK_BACK.0 {
                        rt.find_query.pop();
                        rt.redraw();
                    }
                    return true;
                }

                rt.caret_on = true;
                if vk == VK_UP.0 {
                    rt.cur_line = rt.cur_line.saturating_sub(1);
                    rt.cur_col = rt.cur_col.min(rt.lines[rt.cur_line].chars().count());
                } else if vk == VK_DOWN.0 {
                    rt.cur_line = (rt.cur_line + 1).min(rt.lines.len().saturating_sub(1));
                    rt.cur_col = rt.cur_col.min(rt.lines[rt.cur_line].chars().count());
                } else if vk == VK_LEFT.0 {
                    if rt.cur_col > 0 {
                        rt.cur_col -= 1;
                    } else if rt.cur_line > 0 {
                        rt.cur_line -= 1;
                        rt.cur_col = rt.lines[rt.cur_line].chars().count();
                    }
                } else if vk == VK_RIGHT.0 {
                    let len = rt.lines[rt.cur_line].chars().count();
                    if rt.cur_col < len {
                        rt.cur_col += 1;
                    } else if rt.cur_line + 1 < rt.lines.len() {
                        rt.cur_line += 1;
                        rt.cur_col = 0;
                    }
                } else if vk == VK_HOME.0 {
                    rt.cur_col = 0;
                } else if vk == VK_END.0 {
                    rt.cur_col = rt.lines[rt.cur_line].chars().count();
                } else if vk == VK_PRIOR.0 {
                    let step = rt.visible_lines();
                    rt.cur_line = rt.cur_line.saturating_sub(step);
                    rt.cur_col = rt.cur_col.min(rt.lines[rt.cur_line].chars().count());
                } else if vk == VK_NEXT.0 {
                    let step = rt.visible_lines();
                    rt.cur_line = (rt.cur_line + step).min(rt.lines.len().saturating_sub(1));
                    rt.cur_col = rt.cur_col.min(rt.lines[rt.cur_line].chars().count());
                } else if vk == VK_BACK.0 {
                    if rt.cur_col > 0 {
                        let b0 = char_to_byte(&rt.lines[rt.cur_line], rt.cur_col - 1);
                        let b1 = char_to_byte(&rt.lines[rt.cur_line], rt.cur_col);
                        rt.lines[rt.cur_line].replace_range(b0..b1, "");
                        rt.cur_col -= 1;
                        rt.modified = true;
                    } else if rt.cur_line > 0 {
                        let tail = rt.lines.remove(rt.cur_line);
                        rt.cur_line -= 1;
                        rt.cur_col = rt.lines[rt.cur_line].chars().count();
                        rt.lines[rt.cur_line].push_str(&tail);
                        rt.modified = true;
                    }
                } else if vk == VK_DELETE.0 {
                    let len = rt.lines[rt.cur_line].chars().count();
                    if rt.cur_col < len {
                        let b0 = char_to_byte(&rt.lines[rt.cur_line], rt.cur_col);
                        let b1 = char_to_byte(&rt.lines[rt.cur_line], rt.cur_col + 1);
                        rt.lines[rt.cur_line].replace_range(b0..b1, "");
                        rt.modified = true;
                    } else if rt.cur_line + 1 < rt.lines.len() {
                        let next = rt.lines.remove(rt.cur_line + 1);
                        rt.lines[rt.cur_line].push_str(&next);
                        rt.modified = true;
                    }
                } else if vk == VK_RETURN.0 {
                    let b = char_to_byte(&rt.lines[rt.cur_line], rt.cur_col);
                    let rest = rt.lines[rt.cur_line].split_off(b);
                    rt.cur_line += 1;
                    rt.lines.insert(rt.cur_line, rest);
                    rt.cur_col = 0;
                    rt.modified = true;
                } else if vk == VK_TAB.0 {
                    let b = char_to_byte(&rt.lines[rt.cur_line], rt.cur_col);
                    rt.lines[rt.cur_line].insert_str(b, "    ");
                    rt.cur_col += 4;
                    rt.modified = true;
                }
                rt.ensure_cursor_visible();
                rt.redraw();
                true
            }
            WM_CHAR => {
                if ctrl_down() {
                    return true;
                }
                if let Some(ch) = char::from_u32(wparam.0 as u32) {
                    if !ch.is_control() {
                        if rt.find_open {
                            rt.find_query.push(ch);
                        } else {
                            let b = char_to_byte(&rt.lines[rt.cur_line], rt.cur_col);
                            rt.lines[rt.cur_line].insert(b, ch);
                            rt.cur_col += 1;
                            rt.modified = true;
                            rt.caret_on = true;
                            rt.ensure_cursor_visible();
                        }
                        rt.redraw();
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
        let _ = ShowWindow(hwnd, SW_MINIMIZE);
    } else if let Some(max) = deferred_max {
        let _ = ShowWindow(hwnd, if max { SW_MAXIMIZE } else { SW_RESTORE });
    }

    if handled {
        LRESULT(0)
    } else {
        DefWindowProcW(hwnd, msg, wparam, lparam)
    }
}
