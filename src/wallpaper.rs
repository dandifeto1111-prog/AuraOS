//! AuraOS v5.2 Modular Process-Isolated Wallpaper Engine
//! - 4 Built-in Direct2D GPU animated/static AuraOS wallpapers (lazy swapchain trimmed when unused!)
//! - Native Wallpaper Engine Steam Workshop scanner (ALL `video` + `web` items from 431960/project.json!)
//! - Video Wallpapers run 100% ISOLATED in `AuraWallpaper.exe` (DXVA/D3D11 Hardware Decode, Native 1080p/1440p
//!   at true 60 FPS, zero pixelation, zero MF DLLs or virtual memory leaks inside `AuraShell.exe`!)
//! - True Embedded Web Wallpaper (`index.html`) reparented DIRECTLY into `WorkerW` behind desktop icons
//! - Smart Auto-Pause via instant Win32 IPC (`WM_AURA_WP_PAUSE` / `WM_AURA_WP_RESUME`) -> 0.0% CPU!

use crate::models::{app_data_dir, CustomItemsStore, WallpaperItem, WallpaperKind};
use crate::renderer::{DrawCtx, GpuEngine, Rect, WindowSurface};
use crate::theme;
use crate::win32_shell::to_wide;
use std::collections::HashSet;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use windows::core::PCWSTR;
use windows::Win32::Foundation::{BOOL, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::{OpenProcess, TerminateProcess, PROCESS_TERMINATE};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, EnumWindows, FindWindowExW, FindWindowW,
    GetClassNameW, GetWindowLongW, GetWindowTextW, GetWindowThreadProcessId, IsWindow,
    IsWindowVisible, PostMessageW, RegisterClassW, SendMessageTimeoutW, SetParent, SetWindowLongW,
    SetWindowPos, ShowWindow, GWL_EXSTYLE, GWL_STYLE, HWND_BOTTOM, HWND_MESSAGE, HWND_TOP,
    SMTO_NORMAL, SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_SHOWWINDOW,
    SW_HIDE, SW_SHOWNOACTIVATE, WM_APP, WM_CLOSE, WNDCLASSW, WS_CHILD, WS_EX_APPWINDOW,
    WS_EX_NOACTIVATE, WS_EX_NOREDIRECTIONBITMAP, WS_EX_TOOLWINDOW, WS_EX_TRANSPARENT, WS_POPUP,
    WS_VISIBLE,
};

pub const WEB_WP_TITLE_MARKER: &str = "AURA_WEB_WP_HOST_312";
pub const WP_WORKER_CLASS: &str = "AuraWallpaperWorker_v52";
pub const WP_IPC_CLASS: &str = "AuraWallpaperIPC_v52";
pub const WM_AURA_WP_FRAME: u32 = WM_APP + 1;
pub const WM_AURA_WP_PAUSE: u32 = WM_APP + 10;
pub const WM_AURA_WP_RESUME: u32 = WM_APP + 11;
pub const WM_AURA_WP_QUIT: u32 = WM_APP + 12;

/// Locates the active isolated `AuraWallpaper.exe` IPC control window (`HWND_MESSAGE` — immune to WorkerW parenting!)
pub fn find_video_worker_ipc_hwnd() -> Option<HWND> {
    unsafe {
        let cls = to_wide(WP_IPC_CLASS);
        if let Ok(hw) = FindWindowExW(HWND_MESSAGE, HWND::default(), PCWSTR(cls.as_ptr()), None) {
            if !hw.is_invalid() {
                return Some(hw);
            }
        }
        let wcls = to_wide(WP_WORKER_CLASS);
        if let Ok(hw) = FindWindowW(PCWSTR(wcls.as_ptr()), None) {
            if !hw.is_invalid() {
                return Some(hw);
            }
        }
        None
    }
}

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

pub fn is_video_ext(path: &str) -> bool {
    let low = path.to_lowercase();
    low.ends_with(".mp4")
        || low.ends_with(".webm")
        || low.ends_with(".mkv")
        || low.ends_with(".mov")
        || low.ends_with(".m4v")
        || low.ends_with(".wmv")
        || low.ends_with(".avi")
}

fn resolve_wallpaper_worker_exe() -> Option<PathBuf> {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let cand = dir.join("AuraWallpaper.exe");
            if cand.exists() {
                return Some(cand);
            }
        }
    }
    let inst = app_data_dir().join("AuraWallpaper.exe");
    if inst.exists() {
        return Some(inst);
    }
    None
}

/// Deterministic cached thumbnail path (matches `video_decoder::video_thumbnail_cache_path`)
pub fn video_thumb_cache_path(video_path: &str) -> PathBuf {
    let thumb_dir = app_data_dir().join("wp_thumbs");
    let _ = std::fs::create_dir_all(&thumb_dir);
    let mut h: u64 = 0xcbf29ce484222325;
    for b in video_path.as_bytes() {
        h = (h ^ (*b as u64)).wrapping_mul(0x100000001b3);
    }
    thumb_dir.join(format!("vid_{:016x}.bmp", h))
}

/// Extracts a video thumbnail via isolated `AuraWallpaper.exe --thumb` so `AuraShell.exe` NEVER loads Media Foundation!
pub fn extract_video_thumbnail_isolated(video_path: &str) -> Option<String> {
    if !Path::new(video_path).exists() {
        return None;
    }
    let out_bmp = video_thumb_cache_path(video_path);
    if out_bmp.exists() {
        return Some(out_bmp.to_string_lossy().to_string());
    }
    let out_jpg = out_bmp.with_extension("jpg");
    if out_jpg.exists() {
        return Some(out_jpg.to_string_lossy().to_string());
    }
    if let Some(worker) = resolve_wallpaper_worker_exe() {
        let _ = Command::new(worker)
            .args(["--thumb", video_path])
            .creation_flags(CREATE_NO_WINDOW)
            .status();
        if out_bmp.exists() {
            return Some(out_bmp.to_string_lossy().to_string());
        }
        if out_jpg.exists() {
            return Some(out_jpg.to_string_lossy().to_string());
        }
    }
    None
}

/// Scan built-in AuraOS presets + Steam Workshop Wallpaper Engine (video & web) + custom local wallpapers
pub fn scan_wallpapers(custom: &CustomItemsStore) -> Vec<WallpaperItem> {
    let mut items = vec![
        WallpaperItem {
            id: "external".into(),
            title: "Fondo actual de Windows".into(),
            subtitle: "Windows / Wallpaper Engine externo".into(),
            kind: WallpaperKind::External,
            file_path: String::new(),
            preview_path: None,
            accent_rgb: (92, 92, 114),
        },
        WallpaperItem {
            id: "preset:aura_obsidian_flow".into(),
            title: "Aura Obsidian Flow".into(),
            subtitle: "Predefinido AuraOS · Fluido GPU".into(),
            kind: WallpaperKind::Preset,
            file_path: "aura_obsidian_flow".into(),
            preview_path: None,
            accent_rgb: (124, 92, 255),
        },
        WallpaperItem {
            id: "preset:midnight_aurora".into(),
            title: "Midnight Aurora".into(),
            subtitle: "Predefinido AuraOS · Aurora Boreal".into(),
            kind: WallpaperKind::Preset,
            file_path: "midnight_aurora".into(),
            preview_path: None,
            accent_rgb: (34, 197, 94),
        },
        WallpaperItem {
            id: "preset:amber_eclipse".into(),
            title: "Amber Eclipse".into(),
            subtitle: "Predefinido AuraOS · Horizonte Cálido".into(),
            kind: WallpaperKind::Preset,
            file_path: "amber_eclipse".into(),
            preview_path: None,
            accent_rgb: (245, 183, 49),
        },
        WallpaperItem {
            id: "preset:matte_grid".into(),
            title: "Obsidian Matte Grid".into(),
            subtitle: "Predefinido AuraOS · Ultra Ligero (0% GPU)".into(),
            kind: WallpaperKind::Preset,
            file_path: "matte_grid".into(),
            preview_path: None,
            accent_rgb: (74, 74, 102),
        },
    ];

    for cw in &custom.custom_wallpapers {
        let low = cw.target.to_lowercase();
        let is_vid = is_video_ext(&low);
        let is_web = low.ends_with(".html") || low.ends_with(".htm");
        let kind = if is_vid {
            WallpaperKind::Video
        } else if is_web {
            WallpaperKind::Web
        } else {
            WallpaperKind::Image
        };
        let preview_path = if !is_vid && !is_web {
            Some(cw.target.clone())
        } else {
            cw.cover_path.clone()
        };
        items.push(WallpaperItem {
            id: format!("{}:{}", if is_vid { "video" } else if is_web { "web" } else { "image" }, cw.target),
            title: cw.name.clone(),
            subtitle: format!("Personalizado · {}", if is_vid { "Video HD" } else if is_web { "Web" } else { "Imagen" }),
            kind,
            file_path: cw.target.clone(),
            preview_path,
            accent_rgb: (124, 92, 255),
        });
    }

    let workshop_roots = [
        PathBuf::from(r"D:\SteamLibrary\steamapps\workshop\content\431960"),
        PathBuf::from(r"C:\Program Files (x86)\Steam\steamapps\workshop\content\431960"),
    ];
    for root in workshop_roots {
        let Ok(rd) = std::fs::read_dir(&root) else { continue };
        for entry in rd.flatten() {
            let dir = entry.path();
            if !dir.is_dir() {
                continue;
            }
            let pj = dir.join("project.json");
            let Ok(txt) = std::fs::read_to_string(&pj) else { continue };
            let Ok(v) = serde_json::from_str::<serde_json::Value>(&txt) else { continue };
            let wtype = v.get("type").and_then(|x| x.as_str()).unwrap_or("").to_lowercase();
            if wtype != "video" && wtype != "web" {
                continue;
            }
            let file_rel = v.get("file").and_then(|x| x.as_str()).unwrap_or("");
            if file_rel.is_empty() {
                continue;
            }
            let full_file = dir.join(file_rel);
            if !full_file.exists() {
                continue;
            }
            let title = v
                .get("title")
                .and_then(|x| x.as_str())
                .filter(|s| !s.trim().is_empty())
                .unwrap_or("Wallpaper Workshop")
                .to_string();
            let preview_path = v
                .get("preview")
                .and_then(|x| x.as_str())
                .map(|p| dir.join(p))
                .filter(|p| p.exists())
                .map(|p| p.to_string_lossy().to_string());
            let path_str = full_file.to_string_lossy().to_string();

            if wtype == "video" {
                items.push(WallpaperItem {
                    id: format!("video:{}", path_str),
                    title,
                    subtitle: "Workshop · Video 60 FPS HD".into(),
                    kind: WallpaperKind::Video,
                    file_path: path_str,
                    preview_path,
                    accent_rgb: (96, 165, 250),
                });
            } else {
                items.push(WallpaperItem {
                    id: format!("web:{}", path_str),
                    title,
                    subtitle: "Workshop · Web Integrado".into(),
                    kind: WallpaperKind::Web,
                    file_path: path_str,
                    preview_path,
                    accent_rgb: (34, 197, 94),
                });
            }
        }
    }

    items
}

fn percent_encode_file_url(path: &str) -> String {
    let slash = path.replace('\\', "/");
    let mut out = String::from("file:///");
    for b in slash.as_bytes() {
        let c = *b as char;
        if c.is_ascii_alphanumeric() || matches!(c, '/' | ':' | '-' | '_' | '.' | '~') {
            out.push(c);
        } else {
            out.push_str(&format!("%{:02X}", b));
        }
    }
    out
}

/// Find Windows desktop WorkerW layer cleanly (supports Win10/11 classic WorkerW AND Win11 24H2/26200 Progman child!)
pub fn find_desktop_workerw() -> Option<HWND> {
    unsafe {
        let progman_cls = to_wide("Progman");
        let progman = FindWindowW(PCWSTR(progman_cls.as_ptr()), None).ok()?;
        if progman.is_invalid() {
            return None;
        }
        let mut res = 0usize;
        let _ = SendMessageTimeoutW(
            progman,
            0x052C,
            WPARAM(0x0000_000D),
            LPARAM(1),
            SMTO_NORMAL,
            800,
            Some(&mut res),
        );

        struct FindCtx {
            workerw: HWND,
        }
        unsafe extern "system" fn enum_cb(top: HWND, lparam: LPARAM) -> BOOL {
            let ctx = &mut *(lparam.0 as *mut FindCtx);
            let def_cls = to_wide("SHELLDLL_DefView");
            if let Ok(def) = FindWindowExW(top, HWND::default(), PCWSTR(def_cls.as_ptr()), None) {
                if !def.is_invalid() {
                    let ww_cls = to_wide("WorkerW");
                    if let Ok(sib) = FindWindowExW(HWND::default(), top, PCWSTR(ww_cls.as_ptr()), None) {
                        if !sib.is_invalid() {
                            ctx.workerw = sib;
                            return BOOL(0);
                        }
                    }
                    if let Ok(child_ww) = FindWindowExW(top, def, PCWSTR(ww_cls.as_ptr()), None) {
                        if !child_ww.is_invalid() {
                            ctx.workerw = child_ww;
                            return BOOL(0);
                        }
                    }
                }
            }
            BOOL(1)
        }

        let mut ctx = FindCtx { workerw: HWND::default() };
        let _ = EnumWindows(Some(enum_cb), LPARAM(&mut ctx as *mut FindCtx as isize));
        if !ctx.workerw.is_invalid() {
            Some(ctx.workerw)
        } else {
            None
        }
    }
}

fn snapshot_chrome_windows() -> HashSet<isize> {
    let mut set = HashSet::new();
    unsafe extern "system" fn cb(hwnd: HWND, lp: LPARAM) -> BOOL {
        let s = &mut *(lp.0 as *mut HashSet<isize>);
        let mut cbuf = [0u16; 64];
        let clen = GetClassNameW(hwnd, &mut cbuf);
        let cls = String::from_utf16_lossy(&cbuf[..clen as usize]);
        if cls == "Chrome_WidgetWin_1" {
            s.insert(hwnd.0 as isize);
        }
        BOOL(1)
    }
    unsafe {
        let _ = EnumWindows(Some(cb), LPARAM(&mut set as *mut HashSet<isize> as isize));
    }
    set
}

unsafe extern "system" fn wp_wnd_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    DefWindowProcW(hwnd, msg, wparam, lparam)
}

fn terminate_pid(pid: u32) {
    if pid == 0 {
        return;
    }
    unsafe {
        if let Ok(h) = OpenProcess(PROCESS_TERMINATE, false, pid) {
            let _ = TerminateProcess(h, 0);
            let _ = windows::Win32::Foundation::CloseHandle(h);
        }
    }
}

pub struct WallpaperEngine {
    pub active_id: String,
    pub active_kind: WallpaperKind,
    pub active_path: String,
    pub paused: bool,
    pub performance_mode: bool,
    pub anim_time: f32,
    preset_accum_dt: f32,
    screen_w: i32,
    screen_h: i32,
    workerw_hwnd: HWND,
    hwnd_host: HWND,
    surf_host: Option<WindowSurface>,
    video_child: Option<Child>,
    video_paused_synced: bool,
    video_html_fallback_active: bool,
    web_child: Option<Child>,
    web_hwnd: Option<HWND>,
    web_owner_pid: Option<u32>,
    web_pre_snapshot: HashSet<isize>,
    web_recheck_frames: u8,
    mounted_on_workerw: bool,
    dirty_static: bool,
}

impl WallpaperEngine {
    pub fn new(gpu: &GpuEngine, sw: i32, sh: i32) -> Self {
        let mut eng = Self {
            active_id: "external".into(),
            active_kind: WallpaperKind::External,
            active_path: String::new(),
            paused: false,
            performance_mode: false,
            anim_time: 0.0,
            preset_accum_dt: 0.0,
            screen_w: sw,
            screen_h: sh,
            workerw_hwnd: HWND::default(),
            hwnd_host: HWND::default(),
            surf_host: None,
            video_child: None,
            video_paused_synced: false,
            video_html_fallback_active: false,
            web_child: None,
            web_hwnd: None,
            web_owner_pid: None,
            web_pre_snapshot: HashSet::new(),
            web_recheck_frames: 0,
            mounted_on_workerw: false,
            dirty_static: true,
        };
        let _ = eng.init_host_window(gpu);
        eng
    }

    pub fn set_performance_mode(&mut self, low: bool) {
        self.performance_mode = low;
    }

    pub fn needs_fast_tick(&self) -> bool {
        if self.paused {
            return false;
        }
        match self.active_kind {
            // Video runs at 60 FPS completely isolated inside AuraWallpaper.exe (0 AuraShell ticks!)
            WallpaperKind::Video => self.video_html_fallback_active && (self.web_hwnd.is_none() || self.web_recheck_frames > 0),
            WallpaperKind::Preset => !self.performance_mode && self.active_path != "matte_grid",
            WallpaperKind::Web => self.web_hwnd.is_none() || self.web_recheck_frames > 0,
            _ => false,
        }
    }

    fn init_host_window(&mut self, gpu: &GpuEngine) -> windows::core::Result<()> {
        unsafe {
            let hinst = GetModuleHandleW(None)?;
            let cls = to_wide("AuraWallpaper_Rust_v31_Host");
            let wc = WNDCLASSW {
                lpfnWndProc: Some(wp_wnd_proc),
                hInstance: hinst.into(),
                lpszClassName: PCWSTR(cls.as_ptr()),
                ..Default::default()
            };
            RegisterClassW(&wc);

            let title = to_wide("AuraWallpaperLayer");
            let host = CreateWindowExW(
                WS_EX_NOREDIRECTIONBITMAP | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_TRANSPARENT,
                PCWSTR(cls.as_ptr()),
                PCWSTR(title.as_ptr()),
                WS_POPUP,
                0,
                0,
                self.screen_w,
                self.screen_h,
                None,
                None,
                hinst,
                None,
            )?;

            if let Some(workerw) = find_desktop_workerw() {
                let _ = SetParent(host, workerw);
                let _ = SetWindowPos(host, HWND_TOP, 0, 0, self.screen_w, self.screen_h, SWP_NOACTIVATE);
                self.workerw_hwnd = workerw;
                self.mounted_on_workerw = true;
            } else {
                let _ = SetWindowPos(host, HWND_BOTTOM, 0, 0, self.screen_w, self.screen_h, SWP_NOACTIVATE);
                self.mounted_on_workerw = false;
            }

            // Start lazily trimmed at 2x2 (~16 KB!) — expands only when Preset/Image actually paints!
            let surf = WindowSurface::new(gpu, host, 2, 2)?;
            self.hwnd_host = host;
            self.surf_host = Some(surf);
            Ok(())
        }
    }

    fn hide_and_trim_host(&mut self, gpu: &GpuEngine) {
        unsafe {
            if !self.hwnd_host.is_invalid() {
                let _ = ShowWindow(self.hwnd_host, SW_HIDE);
            }
        }
        if let Some(surf) = &mut self.surf_host {
            surf.trim_hidden(gpu);
        }
    }

    pub fn set_wallpaper(&mut self, gpu: &mut GpuEngine, id: &str) {
        self.stop_video();
        self.stop_web();
        self.active_id = id.to_string();
        self.dirty_static = true;

        if id.is_empty() || id == "external" {
            self.active_kind = WallpaperKind::External;
            self.active_path.clear();
            self.hide_and_trim_host(gpu);
            return;
        }

        if let Some(preset) = id.strip_prefix("preset:") {
            self.active_kind = WallpaperKind::Preset;
            self.active_path = preset.to_string();
            if let Some(surf) = &mut self.surf_host {
                surf.ensure_size(gpu, self.screen_w as u32, self.screen_h as u32);
            }
            self.show_host_window();
            self.render_preset_or_image(gpu);
        } else if let Some(path) = id.strip_prefix("image:") {
            self.active_kind = WallpaperKind::Image;
            self.active_path = path.to_string();
            if let Some(surf) = &mut self.surf_host {
                surf.ensure_size(gpu, self.screen_w as u32, self.screen_h as u32);
            }
            self.show_host_window();
            self.render_preset_or_image(gpu);
        } else if let Some(path) = id.strip_prefix("video:") {
            self.active_kind = WallpaperKind::Video;
            self.active_path = path.to_string();
            // AuraWallpaper.exe owns its own WorkerW window + GPU surface — trim AuraShell's host to 2x2!
            self.hide_and_trim_host(gpu);
            self.spawn_video_worker(path);
        } else if let Some(path) = id.strip_prefix("web:") {
            self.active_kind = WallpaperKind::Web;
            self.active_path = path.to_string();
            self.hide_and_trim_host(gpu);
            self.start_web(path);
        }
    }

    fn show_host_window(&self) {
        unsafe {
            if !self.hwnd_host.is_invalid() {
                let _ = ShowWindow(self.hwnd_host, SW_SHOWNOACTIVATE);
                if !self.mounted_on_workerw {
                    let _ = SetWindowPos(self.hwnd_host, HWND_BOTTOM, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
                }
            }
        }
    }

    fn find_video_worker_hwnd() -> Option<HWND> {
        find_video_worker_ipc_hwnd()
    }

    fn spawn_video_worker(&mut self, path: &str) {
        self.video_html_fallback_active = false;
        self.video_paused_synced = false;
        if !Path::new(path).exists() {
            return;
        }
        let Some(worker_exe) = resolve_wallpaper_worker_exe() else {
            // Graceful fallback if AuraWallpaper.exe is missing during dev
            self.start_video_html_fallback(path);
            return;
        };
        let max_fps = if self.performance_mode { 30 } else { 60 };
        let max_w = if self.performance_mode {
            1280
        } else {
            (self.screen_w as u32).clamp(1280, 2560)
        };
        let max_h = if self.performance_mode {
            720
        } else {
            (self.screen_h as u32).clamp(720, 1440)
        };
        let parent_pid = std::process::id();

        match Command::new(worker_exe)
            .args([
                "--video",
                path,
                "--max-w",
                &max_w.to_string(),
                "--max-h",
                &max_h.to_string(),
                "--max-fps",
                &max_fps.to_string(),
                "--perf",
                if self.performance_mode { "1" } else { "0" },
                "--paused",
                if self.paused { "1" } else { "0" },
                "--parent-pid",
                &parent_pid.to_string(),
            ])
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
        {
            Ok(child) => {
                self.video_child = Some(child);
                self.video_paused_synced = self.paused;
            }
            Err(_) => {
                self.start_video_html_fallback(path);
            }
        }
    }

    fn start_video_html_fallback(&mut self, video_path: &str) {
        if self.video_html_fallback_active || !Path::new(video_path).exists() {
            return;
        }
        self.video_html_fallback_active = true;
        let file_url = percent_encode_file_url(video_path);
        let html = format!(
            "<!doctype html><html><head><meta charset=\"utf-8\"><title>{}</title>\
            <style>html,body{{margin:0;padding:0;width:100%;height:100%;overflow:hidden;background:#0b0b11;}}\
            video{{width:100vw;height:100vh;object-fit:cover;display:block;}}</style></head>\
            <body><video src=\"{}\" autoplay loop muted playsinline></video></body></html>",
            WEB_WP_TITLE_MARKER, file_url
        );
        let dir = app_data_dir();
        let _ = std::fs::create_dir_all(&dir);
        let wrapper = dir.join("wp_video_fallback.html");
        if std::fs::write(&wrapper, html).is_ok() {
            self.start_web(&wrapper.to_string_lossy());
        }
    }

    fn stop_video(&mut self) {
        if let Some(hw) = Self::find_video_worker_hwnd() {
            unsafe {
                let _ = PostMessageW(hw, WM_AURA_WP_QUIT, WPARAM(0), LPARAM(0));
            }
        }
        if let Some(mut child) = self.video_child.take() {
            for _ in 0..8 {
                if let Ok(Some(_)) = child.try_wait() {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            let _ = child.kill();
            let _ = child.wait();
        }
        self.video_html_fallback_active = false;
        self.video_paused_synced = false;
    }

    fn start_web(&mut self, html_path: &str) {
        if !Path::new(html_path).exists() {
            return;
        }
        let edge_candidates = [
            r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe",
            r"C:\Program Files\Microsoft\Edge\Application\msedge.exe",
        ];
        let Some(edge) = edge_candidates.into_iter().find(|p| Path::new(p).exists()) else {
            return;
        };

        // Load index.html DIRECTLY (no cross-directory file:/// iframe CORS block!)
        // using an isolated --user-data-dir so it ALWAYS spawns its own dedicated HWND offscreen!
        let target_url = percent_encode_file_url(html_path);
        let profile = app_data_dir().join("WebWallpaperProfile");
        let _ = std::fs::create_dir_all(&profile);

        // Snapshot existing Edge windows BEFORE launching so we never touch the user's browser!
        self.web_pre_snapshot = snapshot_chrome_windows();

        if let Ok(child) = Command::new(edge)
            .args([
                &format!("--app={}", target_url),
                &format!("--user-data-dir={}", profile.to_string_lossy()),
                "--allow-file-access-from-files",
                "--window-position=-32000,-32000", // Start offscreen -> ZERO popup window flash!
                &format!("--window-size={},{}", self.screen_w, self.screen_h),
                "--autoplay-policy=no-user-gesture-required",
                "--no-first-run",
                "--no-default-browser-check",
                "--disable-features=Translate",
                "--disable-extensions",
            ])
            .spawn()
        {
            self.web_child = Some(child);
            self.web_hwnd = None;
            self.web_owner_pid = None;
        }
    }

    fn try_attach_web_hwnd(&mut self) {
        if self.web_hwnd.is_some() || self.web_child.is_none() {
            return;
        }
        struct WebFind<'a> {
            pre: &'a HashSet<isize>,
            found: HWND,
        }
        unsafe extern "system" fn cb(hwnd: HWND, lp: LPARAM) -> BOOL {
            let st = &mut *(lp.0 as *mut WebFind<'_>);
            if !IsWindowVisible(hwnd).as_bool() {
                return BOOL(1);
            }
            let mut cbuf = [0u16; 64];
            let clen = GetClassNameW(hwnd, &mut cbuf);
            let cls = String::from_utf16_lossy(&cbuf[..clen as usize]);
            if cls != "Chrome_WidgetWin_1" {
                return BOOL(1);
            }
            if !st.pre.contains(&(hwnd.0 as isize)) {
                let mut tbuf = [0u16; 256];
                let tlen = GetWindowTextW(hwnd, &mut tbuf);
                let title = String::from_utf16_lossy(&tbuf[..tlen as usize]);
                if !title.trim().is_empty() {
                    st.found = hwnd;
                    return BOOL(0);
                }
            }
            BOOL(1)
        }

        unsafe {
            let mut wf = WebFind {
                pre: &self.web_pre_snapshot,
                found: HWND::default(),
            };
            let _ = EnumWindows(Some(cb), LPARAM(&mut wf as *mut WebFind<'_> as isize));
            if !wf.found.is_invalid() {
                let mut owner_pid = 0u32;
                GetWindowThreadProcessId(wf.found, Some(&mut owner_pid));
                self.web_owner_pid = if owner_pid != 0 { Some(owner_pid) } else { None };

                // Strip ALL Windows titlebar/border chrome & remove from Alt+Tab/Taskbar!
                let style = GetWindowLongW(wf.found, GWL_STYLE);
                let chrome_mask: i32 = 0x00CF_0000u32 as i32 | (WS_POPUP.0 as i32);
                SetWindowLongW(
                    wf.found,
                    GWL_STYLE,
                    (style & !chrome_mask) | (WS_CHILD.0 as i32) | (WS_VISIBLE.0 as i32),
                );
                let ex = GetWindowLongW(wf.found, GWL_EXSTYLE);
                SetWindowLongW(
                    wf.found,
                    GWL_EXSTYLE,
                    (ex & !(WS_EX_APPWINDOW.0 as i32))
                        | (WS_EX_TOOLWINDOW.0 as i32)
                        | (WS_EX_NOACTIVATE.0 as i32),
                );

                // Reparent DIRECTLY into desktop WorkerW behind desktop icons (NOT inside WS_EX_NOREDIRECTIONBITMAP!)
                if !self.workerw_hwnd.is_invalid() {
                    let _ = SetParent(wf.found, self.workerw_hwnd);
                    let _ = SetWindowPos(
                        wf.found,
                        HWND_TOP,
                        0,
                        0,
                        self.screen_w,
                        self.screen_h,
                        SWP_NOACTIVATE | SWP_FRAMECHANGED | SWP_SHOWWINDOW,
                    );
                } else {
                    let _ = SetWindowPos(
                        wf.found,
                        HWND_BOTTOM,
                        0,
                        0,
                        self.screen_w,
                        self.screen_h,
                        SWP_NOACTIVATE | SWP_FRAMECHANGED | SWP_SHOWWINDOW,
                    );
                }

                // Hide our Direct2D host so the live interactive Web wallpaper shines directly behind desktop icons!
                if !self.hwnd_host.is_invalid() {
                    let _ = ShowWindow(self.hwnd_host, SW_HIDE);
                }
                self.web_hwnd = Some(wf.found);
                self.web_recheck_frames = 35;
            }
        }
    }

    fn enforce_web_embedded(&mut self) {
        if self.web_recheck_frames == 0 {
            return;
        }
        self.web_recheck_frames -= 1;
        if self.web_recheck_frames == 15 || self.web_recheck_frames == 0 {
            if let Some(hw) = self.web_hwnd {
                unsafe {
                    if IsWindow(hw).as_bool() {
                        let style = GetWindowLongW(hw, GWL_STYLE);
                        let chrome_mask: i32 = 0x00CF_0000u32 as i32 | (WS_POPUP.0 as i32);
                        SetWindowLongW(hw, GWL_STYLE, (style & !chrome_mask) | (WS_CHILD.0 as i32) | (WS_VISIBLE.0 as i32));
                        if !self.workerw_hwnd.is_invalid() {
                            let _ = SetParent(hw, self.workerw_hwnd);
                            let _ = SetWindowPos(hw, HWND_TOP, 0, 0, self.screen_w, self.screen_h, SWP_NOACTIVATE | SWP_FRAMECHANGED | SWP_SHOWWINDOW);
                        }
                    }
                }
            }
        }
    }

    fn stop_web(&mut self) {
        if let Some(hw) = self.web_hwnd.take() {
            unsafe {
                if IsWindow(hw).as_bool() {
                    let _ = ShowWindow(hw, SW_HIDE);
                    let _ = SetParent(hw, HWND::default());
                    let _ = PostMessageW(hw, WM_CLOSE, WPARAM(0), LPARAM(0));
                }
            }
        }
        if let Some(pid) = self.web_owner_pid.take() {
            terminate_pid(pid);
        }
        if let Some(mut c) = self.web_child.take() {
            let _ = c.kill();
        }
    }

    /// Pause/Resume automatically via instant Win32 IPC to isolated `AuraWallpaper.exe`!
    /// (0.0% CPU while paused, ZERO Media Foundation tear-down/re-open virtual memory leaks!)
    pub fn set_paused(&mut self, paused: bool) {
        if self.paused == paused {
            return;
        }
        self.paused = paused;
        if self.active_kind == WallpaperKind::Video && !self.video_html_fallback_active {
            if let Some(hw) = Self::find_video_worker_hwnd() {
                let msg = if paused { WM_AURA_WP_PAUSE } else { WM_AURA_WP_RESUME };
                unsafe {
                    let _ = PostMessageW(hw, msg, WPARAM(0), LPARAM(0));
                }
                self.video_paused_synced = paused;
            }
        }
    }

    pub fn tick(&mut self, gpu: &mut GpuEngine, dt_sec: f32) {
        if self.active_kind == WallpaperKind::Web || self.video_html_fallback_active {
            self.try_attach_web_hwnd();
            self.enforce_web_embedded();
            return;
        }
        if self.active_kind == WallpaperKind::Video {
            // Deliver deferred pause/resume if AuraWallpaperWorker finished initializing just after set_paused
            if self.video_paused_synced != self.paused {
                if let Some(hw) = Self::find_video_worker_hwnd() {
                    let msg = if self.paused { WM_AURA_WP_PAUSE } else { WM_AURA_WP_RESUME };
                    unsafe {
                        let _ = PostMessageW(hw, msg, WPARAM(0), LPARAM(0));
                    }
                    self.video_paused_synced = self.paused;
                }
            }
            // If isolated worker exited with failure (unsupported exotic codec), trigger HTML5 fallback cleanly
            if let Some(child) = &mut self.video_child {
                if let Ok(Some(_)) = child.try_wait() {
                    self.video_child = None;
                    if !self.active_path.is_empty() && !self.video_html_fallback_active {
                        let p = self.active_path.clone();
                        self.start_video_html_fallback(&p);
                    }
                }
            }
            return;
        }
        if self.paused || self.active_kind == WallpaperKind::External {
            return;
        }
        if self.active_kind == WallpaperKind::Image || self.active_path == "matte_grid" || self.performance_mode {
            if self.dirty_static {
                self.render_preset_or_image(gpu);
                self.dirty_static = false;
            }
            return;
        }
        // Throttle procedural GPU gradients to ~20 FPS (visually silky-smooth, -67% iGPU usage on modest PCs!)
        self.anim_time += dt_sec;
        self.preset_accum_dt += dt_sec;
        if self.dirty_static || self.preset_accum_dt >= 0.048 {
            self.preset_accum_dt = 0.0;
            self.dirty_static = false;
            self.render_preset_or_image(gpu);
        }
    }

    fn render_preset_or_image(&mut self, gpu: &mut GpuEngine) {
        let Some(surf) = &mut self.surf_host else { return };
        let w = self.screen_w as f32;
        let h = self.screen_h as f32;
        let kind = self.active_kind;
        let path = self.active_path.clone();
        let t = self.anim_time;
        surf.draw(gpu, |ctx| {
            draw_wallpaper_frame(ctx, kind, &path, w, h, t);
        });
    }

    pub fn shutdown(&mut self) {
        self.stop_video();
        self.stop_web();
        unsafe {
            if !self.hwnd_host.is_invalid() {
                let _ = ShowWindow(self.hwnd_host, SW_HIDE);
                let _ = SetParent(self.hwnd_host, HWND::default());
                let _ = DestroyWindow(self.hwnd_host);
                self.hwnd_host = HWND::default();
            }
        }
    }
}

/// Transform-safe preset thumbnail preview inside any target rectangle
pub fn draw_wallpaper_preview_in_rect(ctx: &mut DrawCtx<'_>, preset: &str, r: Rect) {
    match preset {
        "midnight_aurora" => {
            ctx.fill_gradient_rrect(r, 10.0, theme::rgb(7, 11, 18), theme::rgb(11, 14, 24));
            ctx.fill_radial_glow(r.x + r.w * 0.30, r.y + r.h * 0.32, r.w * 0.50, theme::rgba(34, 197, 94, 0.34), theme::rgba(34, 197, 94, 0.0));
            ctx.fill_radial_glow(r.x + r.w * 0.72, r.y + r.h * 0.52, r.w * 0.48, theme::rgba(124, 92, 255, 0.36), theme::rgba(124, 92, 255, 0.0));
        }
        "amber_eclipse" => {
            ctx.fill_gradient_rrect(r, 10.0, theme::rgb(12, 10, 16), theme::rgb(18, 12, 20));
            ctx.fill_radial_glow(r.x + r.w * 0.54, r.y + r.h * 0.58, r.w * 0.46, theme::rgba(245, 183, 49, 0.38), theme::rgba(245, 183, 49, 0.0));
            ctx.fill_circle(r.x + r.w * 0.54, r.y + r.h * 0.52, r.h * 0.22, theme::rgba(10, 10, 15, 0.92));
            ctx.stroke_circle(r.x + r.w * 0.54, r.y + r.h * 0.52, r.h * 0.22, theme::rgba(245, 183, 49, 0.42), 1.5);
        }
        "matte_grid" => {
            ctx.fill_rrect(r, 10.0, theme::CANVAS);
            ctx.fill_radial_glow(r.x + r.w * 0.5, r.y + r.h * 0.5, r.w * 0.55, theme::rgba(124, 92, 255, 0.16), theme::rgba(124, 92, 255, 0.0));
            let mut gx = r.x + 14.0;
            while gx < r.x + r.w - 8.0 {
                let mut gy = r.y + 14.0;
                while gy < r.y + r.h - 8.0 {
                    ctx.fill_circle(gx, gy, 1.1, theme::rgba(255, 255, 255, 0.12));
                    gy += 16.0;
                }
                gx += 16.0;
            }
        }
        _ => {
            ctx.fill_gradient_rrect(r, 10.0, theme::rgb(9, 9, 15), theme::rgb(14, 12, 24));
            ctx.fill_radial_glow(r.x + r.w * 0.30, r.y + r.h * 0.38, r.w * 0.52, theme::rgba(124, 92, 255, 0.42), theme::rgba(124, 92, 255, 0.0));
            ctx.fill_radial_glow(r.x + r.w * 0.74, r.y + r.h * 0.68, r.w * 0.45, theme::rgba(242, 109, 125, 0.20), theme::rgba(242, 109, 125, 0.0));
            for l in 0..3 {
                let by = r.y + r.h * (0.58 + l as f32 * 0.11);
                ctx.draw_line(r.x + 8.0, by, r.x + r.w - 8.0, by - 6.0 + l as f32 * 3.0, theme::rgba(155, 132, 255, 0.25), 1.3);
            }
        }
    }
}

/// Shared Direct2D procedural & image wallpaper renderer
pub fn draw_wallpaper_frame(ctx: &mut DrawCtx<'_>, kind: WallpaperKind, key_or_path: &str, w: f32, h: f32, t: f32) {
    let full = Rect::new(0.0, 0.0, w, h);
    if kind == WallpaperKind::Image {
        ctx.fill_rrect(full, 0.0, theme::CANVAS);
        // Load full-resolution image directly from WIC with aspect-fill center-crop (ZERO stretching!)
        if let Some(wic_src) = ctx.gpu.load_wic_image_file(key_or_path) {
            unsafe {
                if let Ok(bmp) = ctx.rt.CreateBitmapFromWicBitmap(&wic_src, None) {
                    let sz = bmp.GetSize();
                    let bw = sz.width.max(1.0);
                    let bh = sz.height.max(1.0);
                    let bmp_ar = bw / bh;
                    let dst_ar = w.max(1.0) / h.max(1.0);
                    let src = if bmp_ar > dst_ar {
                        let sw = bh * dst_ar;
                        let sx = (bw - sw) * 0.5;
                        windows::Win32::Graphics::Direct2D::Common::D2D_RECT_F {
                            left: sx,
                            top: 0.0,
                            right: sx + sw,
                            bottom: bh,
                        }
                    } else {
                        let sh = bw / dst_ar;
                        let sy = (bh - sh) * 0.5;
                        windows::Win32::Graphics::Direct2D::Common::D2D_RECT_F {
                            left: 0.0,
                            top: sy,
                            right: bw,
                            bottom: sy + sh,
                        }
                    };
                    ctx.rt.DrawBitmap(
                        &bmp,
                        Some(&full.to_d2d()),
                        1.0,
                        windows::Win32::Graphics::Direct2D::D2D1_BITMAP_INTERPOLATION_MODE_LINEAR,
                        Some(&src),
                    );
                }
            }
        }
        return;
    }

    match key_or_path {
        "midnight_aurora" => {
            ctx.fill_gradient_rrect(full, 0.0, theme::rgb(7, 11, 18), theme::rgb(11, 14, 24));
            let x1 = w * (0.30 + 0.14 * (t * 0.25).sin());
            let y1 = h * (0.28 + 0.08 * (t * 0.35).cos());
            ctx.fill_radial_glow(x1, y1, w * 0.44, theme::rgba(34, 197, 94, 0.26), theme::rgba(34, 197, 94, 0.0));

            let x2 = w * (0.72 + 0.12 * (t * 0.22).cos());
            let y2 = h * (0.42 + 0.10 * (t * 0.30).sin());
            ctx.fill_radial_glow(x2, y2, w * 0.46, theme::rgba(124, 92, 255, 0.30), theme::rgba(124, 92, 255, 0.0));

            let x3 = w * (0.50 + 0.18 * (t * 0.18).sin());
            let y3 = h * (0.78 + 0.06 * (t * 0.27).cos());
            ctx.fill_radial_glow(x3, y3, w * 0.38, theme::rgba(56, 189, 248, 0.20), theme::rgba(56, 189, 248, 0.0));
        }
        "amber_eclipse" => {
            ctx.fill_gradient_rrect(full, 0.0, theme::rgb(12, 10, 16), theme::rgb(18, 12, 20));
            let ex = w * (0.52 + 0.06 * (t * 0.20).sin());
            let ey = h * (0.56 + 0.04 * (t * 0.17).cos());
            ctx.fill_radial_glow(ex, ey, w * 0.38, theme::rgba(245, 183, 49, 0.30), theme::rgba(245, 183, 49, 0.0));
            ctx.fill_radial_glow(w * 0.26, h * 0.30, w * 0.36, theme::rgba(124, 92, 255, 0.24), theme::rgba(124, 92, 255, 0.0));
            ctx.fill_circle(ex, ey - h * 0.04, h * 0.16, theme::rgba(10, 10, 15, 0.92));
            ctx.stroke_circle(ex, ey - h * 0.04, h * 0.16, theme::rgba(245, 183, 49, 0.35), 2.0);
        }
        "matte_grid" => {
            ctx.fill_rrect(full, 0.0, theme::CANVAS);
            ctx.fill_radial_glow(w * 0.5, h * 0.45, w * 0.52, theme::rgba(124, 92, 255, 0.12), theme::rgba(124, 92, 255, 0.0));
            let step = (w / 32.0).clamp(24.0, 48.0);
            let mut gx = step * 0.5;
            while gx < w {
                let mut gy = step * 0.5;
                while gy < h {
                    ctx.fill_circle(gx, gy, 1.3, theme::rgba(255, 255, 255, 0.075));
                    gy += step;
                }
                gx += step;
            }
        }
        _ => {
            ctx.fill_gradient_rrect(full, 0.0, theme::rgb(9, 9, 15), theme::rgb(14, 12, 24));
            let x1 = w * (0.28 + 0.12 * (t * 0.24).sin());
            let y1 = h * (0.34 + 0.10 * (t * 0.19).cos());
            ctx.fill_radial_glow(x1, y1, w * 0.46, theme::rgba(124, 92, 255, 0.34), theme::rgba(124, 92, 255, 0.0));

            let x2 = w * (0.74 + 0.10 * (t * 0.21).cos());
            let y2 = h * (0.64 + 0.12 * (t * 0.26).sin());
            ctx.fill_radial_glow(x2, y2, w * 0.42, theme::rgba(91, 61, 245, 0.28), theme::rgba(91, 61, 245, 0.0));

            let x3 = w * (0.56 + 0.14 * (t * 0.15).sin());
            let y3 = h * (0.20 + 0.07 * (t * 0.30).cos());
            ctx.fill_radial_glow(x3, y3, w * 0.30, theme::rgba(242, 109, 125, 0.14), theme::rgba(242, 109, 125, 0.0));

            let lines = 6;
            for l in 0..lines {
                let lf = l as f32;
                let base_y = h * (0.58 + lf * 0.055);
                let steps = 28;
                let mut prev = (0.0f32, base_y);
                for s in 0..=steps {
                    let u = s as f32 / steps as f32;
                    let px = u * w;
                    let wave = (u * 5.2 + t * (0.55 + lf * 0.08) + lf * 0.9).sin() * (h * 0.032)
                        + (u * 2.4 - t * 0.32 + lf * 0.5).cos() * (h * 0.018);
                    let py = base_y + wave;
                    if s > 0 {
                        ctx.draw_line(
                            prev.0,
                            prev.1,
                            px,
                            py,
                            theme::rgba(155, 132, 255, 0.14 - lf * 0.015),
                            1.5,
                        );
                    }
                    prev = (px, py);
                }
            }
        }
    }
}
