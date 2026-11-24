#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#![allow(dead_code)]

//! AuraLock v4.3 — Native Pure-Rust Direct2D/DirectComposition GPU Lock Screen for AuraOS
//! - Custom Static HD Images, Steam Workshop Wallpapers, Live FFmpeg Video Wallpapers & Animated GPU Presets
//! - macOS Sonoma-inspired Hero Clock + Spanish Date + Live Weather Pill + Async MTA Now Playing Card
//! - Custom AuraLock SHA-256 PIN OR Real Windows Password Verification (LogonUserW) OR Instant Unlock
//! - Unified macOS Traffic Lights & Frosted Glass Power / Wallpaper Controls
//! - 100% User-Space, Zero LogonUI/Vanguard Interference, Supports `--capture <out.png>` Visual QA!

use aura_os::audio;
use aura_os::aura_window;
use aura_os::models::{
    hash_lock_pin, load_custom_items, load_settings, save_settings, NowPlaying, ShellSettings,
    WallpaperItem, WallpaperKind, WeatherInfo,
};
use aura_os::renderer::{GpuEngine, Rect, TextStyle, WindowSurface};
use aura_os::system;
use aura_os::theme::{self, VectorIcon};
use aura_os::video_decoder::{self, NativeVideoStream};
use aura_os::wallpaper;
use aura_os::win32_shell;
use std::cell::RefCell;
use std::path::Path;
use std::sync::mpsc::{self, Receiver};
use std::time::Instant;
use windows::core::PCWSTR;
use windows::Win32::Foundation::{BOOL, HANDLE, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::{
    SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    SetFocus, VK_BACK, VK_ESCAPE, VK_RETURN, VK_SPACE,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, GetMessageW, KillTimer, LoadCursorW,
    PostQuitMessage, RegisterClassW, SetTimer, ShowWindow, TranslateMessage, IDC_ARROW, MSG,
    SW_HIDE, SW_SHOW, WM_CHAR, WM_CLOSE, WM_DESTROY, WM_KEYDOWN, WM_LBUTTONDOWN, WM_MOUSEMOVE,
    WM_TIMER, WNDCLASSW, WS_EX_NOREDIRECTIONBITMAP, WS_EX_TOPMOST, WS_POPUP,
};

const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const TIMER_FRAME: usize = 1;
const TIMER_SLOW: usize = 2;

#[link(name = "advapi32")]
extern "system" {
    fn LogonUserW(
        lpszUsername: *const u16,
        lpszDomain: *const u16,
        lpszPassword: *const u16,
        dwLogonType: u32,
        dwLogonProvider: u32,
        phToken: *mut HANDLE,
    ) -> BOOL;
}

fn verify_windows_password(username: &str, password: &str) -> bool {
    use windows::Win32::Foundation::CloseHandle;
    let u = win32_shell::to_wide(username);
    let d = win32_shell::to_wide(".");
    let p = win32_shell::to_wide(password);
    let mut tok = HANDLE::default();
    unsafe {
        let ok = LogonUserW(u.as_ptr(), d.as_ptr(), p.as_ptr(), 3, 0, &mut tok).as_bool();
        if ok && !tok.is_invalid() {
            let _ = CloseHandle(tok);
        }
        ok
    }
}

#[derive(Clone)]
enum LockAct {
    UnlockAttempt,
    CycleWallpaper,
    PickLocalWallpaper,
    TogglePickerDrawer,
    SelectWallpaper(String),
    MediaPrev,
    MediaPlayPause,
    MediaNext,
    WinLock,
    Sleep,
    Restart,
    Shutdown,
    QuickExitIfUnprotected,
}

#[derive(Clone)]
struct Hit {
    id: String,
    rect: Rect,
    act: LockAct,
}

struct LockRuntime {
    hwnd: HWND,
    gpu: GpuEngine,
    surf: WindowSurface,
    width: f32,
    height: f32,
    settings: ShellSettings,
    wallpapers: Vec<WallpaperItem>,
    active_wp_id: String,
    active_img_path: Option<String>,
    video: Option<NativeVideoStream>,
    video_seq: u64,
    video_dims: (u32, u32),
    video_buf: Vec<u8>,
    username: String,
    display_name: String,
    input: String,
    error_msg: String,
    shake_t: f32,
    phase: f32,
    fade_in: f32,
    picker_open: bool,
    now_playing: NowPlaying,
    weather: WeatherInfo,
    bg_rx: Receiver<LockBgMsg>,
    hits: Vec<Hit>,
    hovered_id: Option<String>,
    last_frame: Instant,
    confirm_pwr: Option<&'static str>,
}

enum LockBgMsg {
    Weather(WeatherInfo),
    Media(NowPlaying),
}

thread_local! {
    static LOCK_RT: RefCell<Option<LockRuntime>> = RefCell::new(None);
}

impl LockRuntime {
    fn is_protected(&self) -> bool {
        self.settings.lock_use_windows_pwd || !self.settings.lock_pin_hash.trim().is_empty()
    }

    fn apply_wallpaper(&mut self, wp_id: &str) {
        if let Some(mut v) = self.video.take() {
            v.stop_now();
        }
        self.video_seq = 0;
        self.video_dims = (0, 0);
        self.video_buf.clear();
        self.active_img_path = None;
        self.active_wp_id = wp_id.to_string();

        let effective = if wp_id.trim().is_empty() || wp_id == "external" {
            self.settings.wallpaper_id.as_str()
        } else {
            wp_id
        };

        if let Some(p) = effective.strip_prefix("image:") {
            if Path::new(p).exists() {
                self.active_img_path = Some(p.to_string());
                return;
            }
        }
        if let Some(p) = effective.strip_prefix("video:") {
            if Path::new(p).exists() {
                self.video = NativeVideoStream::spawn(p, 1280, 720, 24);
                if let Some(item) = self.wallpapers.iter().find(|w| w.id == effective) {
                    self.active_img_path = item.preview_path.clone();
                }
                return;
            }
        }
        if let Some(item) = self.wallpapers.iter().find(|w| w.id == effective) {
            if item.kind == WallpaperKind::Video && !item.file_path.is_empty() && Path::new(&item.file_path).exists() {
                self.video = NativeVideoStream::spawn(&item.file_path, 1280, 720, 24);
            }
            if let Some(prev) = &item.preview_path {
                if Path::new(prev).exists() {
                    self.active_img_path = Some(prev.clone());
                }
            }
        }
    }

    fn save_lock_wallpaper(&mut self, wp_id: String) {
        self.settings.lock_wallpaper_id = wp_id.clone();
        save_settings(&self.settings);
        win32_shell::notify_shell_settings_changed();
        self.apply_wallpaper(&wp_id);
        self.redraw();
    }

    fn try_unlock(&mut self) {
        if !self.is_protected() {
            self.unlock_and_exit();
            return;
        }
        let has_custom = !self.settings.lock_pin_hash.trim().is_empty();
        let custom_ok = has_custom && hash_lock_pin(&self.input) == self.settings.lock_pin_hash;
        let win_ok = self.settings.lock_use_windows_pwd
            && verify_windows_password(&self.username, &self.input);
        if custom_ok || win_ok {
            self.unlock_and_exit();
        } else {
            self.input.clear();
            self.error_msg = if self.settings.lock_use_windows_pwd && !has_custom {
                "Contraseña de Windows incorrecta".to_string()
            } else {
                "Contraseña incorrecta · Intenta de nuevo".to_string()
            };
            self.shake_t = 1.0;
            self.redraw();
        }
    }

    fn unlock_and_exit(&mut self) {
        if let Some(mut v) = self.video.take() {
            v.stop_now();
        }
        unsafe {
            let _ = ShowWindow(self.hwnd, SW_HIDE);
            PostQuitMessage(0);
        }
    }

    fn execute(&mut self, act: LockAct) {
        match act {
            LockAct::UnlockAttempt => self.try_unlock(),
            LockAct::QuickExitIfUnprotected => {
                if !self.is_protected() {
                    self.unlock_and_exit();
                } else {
                    self.error_msg = "Introduce tu PIN para desbloquear".to_string();
                    self.shake_t = 0.7;
                    self.redraw();
                }
            }
            LockAct::TogglePickerDrawer => {
                self.picker_open = !self.picker_open;
                self.redraw();
            }
            LockAct::CycleWallpaper => {
                let candidates: Vec<String> = self
                    .wallpapers
                    .iter()
                    .filter(|w| w.id != "external")
                    .map(|w| w.id.clone())
                    .collect();
                if !candidates.is_empty() {
                    let cur = candidates
                        .iter()
                        .position(|id| id == &self.active_wp_id)
                        .unwrap_or(0);
                    let next = candidates[(cur + 1) % candidates.len()].clone();
                    self.save_lock_wallpaper(next);
                }
            }
            LockAct::PickLocalWallpaper => {
                if let Some(path) = win32_shell::pick_file_dialog(
                    "Elegir fondo personalizado para AuraLock (Imagen o Video)",
                    "Imágenes y Videos (*.png;*.jpg;*.jpeg;*.webp;*.mp4;*.webm;*.mkv;*.mov;*.m4v;*.wmv;*.avi)",
                    "*.png;*.jpg;*.jpeg;*.webp;*.bmp;*.mp4;*.webm;*.mkv;*.mov;*.m4v;*.wmv;*.avi",
                ) {
                    let id = if video_decoder::is_video_ext(&path) {
                        format!("video:{}", path)
                    } else {
                        format!("image:{}", path)
                    };
                    self.picker_open = false;
                    self.save_lock_wallpaper(id);
                }
            }
            LockAct::SelectWallpaper(id) => {
                self.picker_open = false;
                self.save_lock_wallpaper(id);
            }
            LockAct::MediaPrev => audio::media_prev(),
            LockAct::MediaPlayPause => {
                audio::media_play_pause();
                if self.now_playing.active {
                    self.now_playing.is_playing = !self.now_playing.is_playing;
                    self.redraw();
                }
            }
            LockAct::MediaNext => audio::media_next(),
            LockAct::WinLock => {
                self.unlock_and_exit();
                win32_shell::power_lock();
            }
            LockAct::Sleep => {
                win32_shell::power_sleep();
            }
            LockAct::Restart => {
                if self.confirm_pwr == Some("restart") {
                    win32_shell::power_restart();
                } else {
                    self.confirm_pwr = Some("restart");
                    self.redraw();
                }
            }
            LockAct::Shutdown => {
                if self.confirm_pwr == Some("shutdown") {
                    win32_shell::power_shutdown();
                } else {
                    self.confirm_pwr = Some("shutdown");
                    self.redraw();
                }
            }
        }
    }

    fn tick(&mut self) {
        let now = Instant::now();
        let dt = now.duration_since(self.last_frame).as_secs_f32().clamp(0.004, 0.06);
        self.last_frame = now;
        self.phase += dt;
        let mut dirty = false;

        if self.fade_in < 1.0 {
            self.fade_in = (self.fade_in + dt * 4.5).min(1.0);
            dirty = true;
        }
        if self.shake_t > 0.0 {
            self.shake_t = (self.shake_t - dt * 2.8).max(0.0);
            dirty = true;
        }
        while let Ok(msg) = self.bg_rx.try_recv() {
            match msg {
                LockBgMsg::Weather(w) => {
                    self.weather = w;
                    dirty = true;
                }
                LockBgMsg::Media(m) => {
                    self.now_playing = m;
                    dirty = true;
                }
            }
        }
        if let Some(vp) = &self.video {
            if let Some((fw, fh)) = vp.swap_latest_frame(&mut self.video_seq, &mut self.video_buf) {
                self.video_dims = (fw, fh);
                dirty = true;
            }
        } else if self.active_img_path.is_none() {
            // Smooth procedural GPU gradient ambient motion
            dirty = true;
        }
        if self.gpu.drain_decoded_assets() {
            dirty = true;
        }
        if dirty {
            self.redraw();
        }
    }

    fn redraw(&mut self) {
        let w = self.width;
        let h = self.height;
        let mut hits = Vec::new();
        let protected = self.is_protected();
        let use_win_pwd = self.settings.lock_use_windows_pwd;
        let has_custom_pwd = !self.settings.lock_pin_hash.trim().is_empty();
        let show_media = self.settings.lock_show_media;
        let active_wp_id = self.active_wp_id.clone();
        let active_img = self.active_img_path.clone();
        let avatar_path = if !self.settings.lock_avatar_path.trim().is_empty()
            && Path::new(&self.settings.lock_avatar_path).exists()
        {
            Some(self.settings.lock_avatar_path.clone())
        } else {
            None
        };
        let vf: Option<(u32, u32, &[u8])> = if self.video_dims.0 > 0 && !self.video_buf.is_empty() {
            Some((self.video_dims.0, self.video_dims.1, self.video_buf.as_slice()))
        } else {
            None
        };
        let display_name = self.display_name.clone();
        let input_len = self.input.chars().count();
        let error_msg = self.error_msg.clone();
        let shake_t = self.shake_t;
        let phase = self.phase;
        let picker_open = self.picker_open;
        let wallpapers = &self.wallpapers;
        let weather = &self.weather;
        let np = &self.now_playing;
        let hovered = self.hovered_id.as_deref();
        let confirm_pwr = self.confirm_pwr;

        self.surf.draw(&mut self.gpu, |ctx| {
            render_lock_screen(
                ctx,
                w,
                h,
                &active_wp_id,
                active_img.as_deref(),
                avatar_path.as_deref(),
                vf,
                &display_name,
                protected,
                use_win_pwd,
                has_custom_pwd,
                input_len,
                &error_msg,
                shake_t,
                phase,
                picker_open,
                wallpapers,
                weather,
                np,
                show_media,
                hovered,
                confirm_pwr,
                &mut hits,
            );
        });
        self.hits = hits;
    }
}

#[allow(clippy::too_many_arguments)]
fn render_lock_screen(
    ctx: &mut aura_os::renderer::DrawCtx<'_>,
    w: f32,
    h: f32,
    active_wp_id: &str,
    active_img: Option<&str>,
    avatar_path: Option<&str>,
    video_frame: Option<(u32, u32, &[u8])>,
    username: &str,
    protected: bool,
    use_win_pwd: bool,
    has_custom_pwd: bool,
    input_len: usize,
    error_msg: &str,
    shake_t: f32,
    phase: f32,
    picker_open: bool,
    wallpapers: &[WallpaperItem],
    weather: &WeatherInfo,
    np: &NowPlaying,
    show_media: bool,
    hovered: Option<&str>,
    confirm_pwr: Option<&'static str>,
    hits: &mut Vec<Hit>,
) {
    let full = Rect::new(0.0, 0.0, w, h);

    // 1. FULLSCREEN WALLPAPER BACKDROP (Video / HD Custom Image / Animated GPU Preset)
    let mut drew_media = false;
    if let Some((fw, fh, bgra)) = video_frame {
        ctx.draw_bgra_live("lock_vid", fw, fh, bgra, full);
        drew_media = true;
    } else if let Some(img_p) = active_img {
        drew_media = ctx.draw_image_file(img_p, full, 1.0);
    }

    if !drew_media {
        // Rich 60 FPS GPU animated AuraOS Lock Screen canvas!
        let (c1, c2, glow_a, glow_b) = if active_wp_id.contains("emerald") {
            (theme::rgb(8, 16, 18), theme::rgb(14, 34, 32), theme::rgba(52, 211, 153, 0.24), theme::rgba(96, 165, 250, 0.16))
        } else if active_wp_id.contains("midnight") {
            (theme::rgb(9, 10, 18), theme::rgb(20, 16, 38), theme::rgba(96, 165, 250, 0.24), theme::rgba(124, 92, 255, 0.20))
        } else {
            (theme::rgb(10, 9, 18), theme::rgb(24, 15, 44), theme::rgba(124, 92, 255, 0.30), theme::rgba(242, 109, 125, 0.16))
        };
        ctx.fill_gradient_rrect(full, 0.0, c1, c2);
        let gx1 = w * (0.28 + 0.08 * (phase * 0.28).sin());
        let gy1 = h * (0.30 + 0.06 * (phase * 0.22).cos());
        ctx.fill_radial_glow(gx1, gy1, w * 0.45, glow_a, theme::rgba(0, 0, 0, 0.0));
        let gx2 = w * (0.74 + 0.07 * (phase * 0.24).cos());
        let gy2 = h * (0.68 + 0.07 * (phase * 0.31).sin());
        ctx.fill_radial_glow(gx2, gy2, w * 0.42, glow_b, theme::rgba(0, 0, 0, 0.0));
    }

    // Cinematic dark scrim for effortless readability on bright wallpapers
    ctx.fill_rrect(full, 0.0, theme::rgba(8, 8, 14, 0.36));

    // 2. TOP-LEFT UNIFIED macOS TRAFFIC LIGHTS (Cohesive AuraOS chrome!)
    let tl_pill = Rect::new(18.0, 16.0, 68.0, 24.0);
    ctx.fill_card(tl_pill, 12.0, theme::rgba(14, 14, 22, 0.62), theme::rgba(255, 255, 255, 0.12));
    let tl_hov = matches!(hovered, Some("lk_tl_x") | Some("lk_tl_min") | Some("lk_tl_wp"));
    let lights = [
        ("lk_tl_x", 32.0f32, theme::CORAL, "×", LockAct::QuickExitIfUnprotected),
        ("lk_tl_min", 52.0f32, theme::AMBER, "−", LockAct::Sleep),
        ("lk_tl_wp", 72.0f32, theme::EMERALD, "+", LockAct::TogglePickerDrawer),
    ];
    for (id, cx, col, glyph, act) in lights {
        ctx.fill_circle(cx, 28.0, if tl_hov { 5.8 } else { 5.2 }, col);
        if tl_hov {
            ctx.draw_text_center(glyph, Rect::new(cx - 5.5, 21.5, 11.0, 12.0), TextStyle::Small9Reg, theme::DARK_INK);
        }
        hits.push(Hit { id: id.into(), rect: Rect::new(cx - 8.0, 20.0, 16.0, 16.0), act });
    }

    // Top-right subtle brand & status pill
    let top_r = Rect::new(w - 178.0, 16.0, 160.0, 24.0);
    ctx.fill_card(top_r, 12.0, theme::rgba(14, 14, 22, 0.62), theme::rgba(255, 255, 255, 0.12));
    ctx.draw_vector_icon(VectorIcon::Lock, top_r.x + 16.0, top_r.y + 12.0, 10.5, theme::VIOLET_LIGHT);
    ctx.draw_text("AuraLock · v4.5", Rect::new(top_r.x + 28.0, top_r.y, 126.0, 24.0), TextStyle::Small10Bold, theme::TEXT_PRIMARY);

    // 3. MACOS SONOMA HERO CLOCK & DATE AT TOP CENTER
    let (time_str, date_str) = current_clock_and_date();
    let clock_y = (h * 0.12).clamp(64.0, 150.0);
    ctx.draw_text_center(
        &date_str,
        Rect::new(0.0, clock_y, w, 28.0),
        TextStyle::Title16Semi,
        theme::rgba(240, 240, 252, 0.90),
    );
    ctx.draw_text_center(
        &time_str,
        Rect::new(2.0, clock_y + 24.0, w, 92.0),
        TextStyle::Clock72Light,
        theme::rgba(0, 0, 0, 0.38),
    );
    ctx.draw_text_center(
        &time_str,
        Rect::new(0.0, clock_y + 22.0, w, 92.0),
        TextStyle::Clock72Light,
        theme::rgba(255, 255, 255, 0.98),
    );

    if weather.loaded {
        let wp_w = 248.0;
        let wp_r = Rect::new((w - wp_w) * 0.5, clock_y + 122.0, wp_w, 30.0);
        ctx.fill_card(wp_r, 15.0, theme::rgba(16, 16, 26, 0.68), theme::rgba(255, 255, 255, 0.14));
        let w_txt = format!("{}°C  ·  {}  ·  {}", weather.temp_c, weather.condition, weather.city);
        ctx.draw_text_center(&w_txt, wp_r, TextStyle::Body11Med, theme::TEXT_PRIMARY);
    }

    // 4. OPTIONAL LIVE NOW PLAYING GLASS PILL (when music active)
    if show_media && np.active {
        let mw = 340.0;
        let mr = Rect::new((w - mw) * 0.5, clock_y + 166.0, mw, 58.0);
        ctx.fill_card(mr, 18.0, theme::rgba(18, 18, 28, 0.78), theme::VIOLET_BORDER);
        let ic = Rect::new(mr.x + 12.0, mr.y + 11.0, 36.0, 36.0);
        ctx.fill_gradient_rrect(ic, 11.0, theme::VIOLET_LIGHT, theme::VIOLET_DEEP);
        ctx.draw_vector_icon(VectorIcon::MusicNote, ic.x + 18.0, ic.y + 18.0, 15.0, theme::TEXT_PRIMARY);
        ctx.draw_text(&np.title, Rect::new(mr.x + 58.0, mr.y + 10.0, 174.0, 19.0), TextStyle::Body12Semi, theme::TEXT_PRIMARY);
        ctx.draw_text(
            if np.artist.is_empty() { &np.source_app } else { &np.artist },
            Rect::new(mr.x + 58.0, mr.y + 30.0, 174.0, 16.0),
            TextStyle::Small10Med,
            theme::TEXT_SECONDARY,
        );
        let mbtns = [
            ("lk_m_prev", VectorIcon::Prev, false, LockAct::MediaPrev),
            ("lk_m_play", VectorIcon::PlayPause, true, LockAct::MediaPlayPause),
            ("lk_m_next", VectorIcon::Next, false, LockAct::MediaNext),
        ];
        let mut bx = mr.x + mr.w - 98.0;
        for (id, icon, hero, act) in mbtns {
            let sz = if hero { 30.0 } else { 24.0 };
            let br = Rect::new(bx, mr.y + (58.0 - sz) * 0.5, sz, sz);
            ctx.fill_rrect(br, sz * 0.5, if hero { theme::CRISP_WHITE } else { theme::ELEVATED });
            ctx.draw_vector_icon(icon, br.x + sz * 0.5, br.y + sz * 0.5, if hero { 12.0 } else { 10.0 }, if hero { theme::DARK_INK } else { theme::TEXT_PRIMARY });
            hits.push(Hit { id: id.into(), rect: br, act });
            bx += sz + 6.0;
        }
    }

    // 5. BOTTOM-CENTER USER AVATAR + GLASS PIN / UNLOCK PILL
    let shake_dx = if shake_t > 0.0 {
        (shake_t * 28.0).sin() * 14.0 * shake_t
    } else {
        0.0
    };
    let card_w = 328.0;
    let base_y = h - 226.0;

    // Avatar circle (Custom Circular Profile Photo OR Violet Initial Badge!)
    let av_cx = w * 0.5 + shake_dx;
    let av_cy = base_y - 4.0;
    let av_r = 38.0f32;
    ctx.fill_circle(av_cx, av_cy, av_r + 5.0, theme::rgba(124, 92, 255, 0.28));
    let av_rect = Rect::new(av_cx - av_r, av_cy - av_r, av_r * 2.0, av_r * 2.0);
    let mut drew_avatar = false;
    if let Some(av_p) = avatar_path {
        let _l = ctx.push_rrect_mask_layer(av_rect, av_rect, av_r, 1.0);
        drew_avatar = ctx.draw_image_file(av_p, av_rect, 1.0);
        if _l.is_some() { ctx.pop_layer(); }
    }
    if !drew_avatar {
        ctx.fill_circle(av_cx, av_cy, av_r, theme::VIOLET);
        let initial = username.chars().next().unwrap_or('A').to_uppercase().to_string();
        ctx.draw_text_center(&initial, Rect::new(av_cx - av_r, av_cy - 18.0, av_r * 2.0, 36.0), TextStyle::Title22Bold, theme::TEXT_PRIMARY);
    }
    ctx.stroke_circle(av_cx, av_cy, av_r + 1.0, theme::rgba(255, 255, 255, 0.42), 2.0);

    ctx.draw_text_center(
        username,
        Rect::new(0.0, base_y + 44.0, w, 24.0),
        TextStyle::Title16Semi,
        theme::TEXT_PRIMARY,
    );

    if protected {
        let pin_r = Rect::new((w - card_w) * 0.5 + shake_dx, base_y + 76.0, card_w, 42.0);
        ctx.fill_card(
            pin_r,
            21.0,
            theme::rgba(18, 18, 28, 0.84),
            if shake_t > 0.0 { theme::CORAL } else { theme::VIOLET_LIGHT },
        );
        let display = if input_len == 0 {
            if use_win_pwd && !has_custom_pwd {
                "Contraseña de Windows · Enter".to_string()
            } else {
                "Contraseña o PIN de AuraLock · Enter".to_string()
            }
        } else {
            "● ".repeat(input_len.min(18))
        };
        ctx.draw_text(
            &display,
            Rect::new(pin_r.x + 18.0, pin_r.y, pin_r.w - 64.0, 42.0),
            TextStyle::Body12Med,
            if input_len == 0 { theme::TEXT_TERTIARY } else { theme::TEXT_PRIMARY },
        );
        let go_r = Rect::new(pin_r.x + pin_r.w - 36.0, pin_r.y + 6.0, 30.0, 30.0);
        ctx.fill_rrect(go_r, 15.0, theme::VIOLET);
        ctx.draw_vector_icon(VectorIcon::ChevronRight, go_r.x + 15.0, go_r.y + 15.0, 12.0, theme::TEXT_PRIMARY);
        hits.push(Hit { id: "lk_go".into(), rect: go_r, act: LockAct::UnlockAttempt });
    } else {
        let btn_r = Rect::new((w - 220.0) * 0.5, base_y + 74.0, 220.0, 40.0);
        let hov = hovered == Some("lk_unlock");
        ctx.fill_card(
            btn_r,
            20.0,
            if hov { theme::VIOLET_LIGHT } else { theme::VIOLET },
            theme::rgba(255, 255, 255, 0.24),
        );
        ctx.draw_text_center("Desbloquear AuraOS", btn_r, TextStyle::Body12Semi, theme::TEXT_PRIMARY);
        hits.push(Hit { id: "lk_unlock".into(), rect: btn_r, act: LockAct::UnlockAttempt });
    }

    let hint = if !error_msg.is_empty() {
        error_msg
    } else if protected {
        "Configura tu PIN o fondo en Configuración AuraOS"
    } else {
        "Pulsa Enter, Espacio o haz clic para volver al escritorio"
    };
    ctx.draw_text_center(
        hint,
        Rect::new(0.0, base_y + 124.0, w, 18.0),
        TextStyle::Small10Med,
        if !error_msg.is_empty() { theme::CORAL } else { theme::rgba(230, 230, 245, 0.72) },
    );

    // 6. BOTTOM-RIGHT FROSTED WALLPAPER & POWER CONTROLS
    let pwr_items = [
        ("lk_wp_pick", "Fondos", VectorIcon::Image, LockAct::TogglePickerDrawer),
        ("lk_winlock", "Win+L", VectorIcon::Lock, LockAct::WinLock),
        ("lk_sleep", "Suspender", VectorIcon::Sleep, LockAct::Sleep),
        ("lk_restart", if confirm_pwr == Some("restart") { "Confirmar" } else { "Reiniciar" }, VectorIcon::Restart, LockAct::Restart),
        ("lk_shutdown", if confirm_pwr == Some("shutdown") { "Confirmar" } else { "Apagar" }, VectorIcon::Power, LockAct::Shutdown),
    ];
    let mut px = w - 20.0;
    let py = h - 54.0;
    for (id, lbl, icon, act) in pwr_items.into_iter().rev() {
        let bw = (lbl.len() as f32 * 6.5 + 38.0).max(86.0);
        px -= bw;
        let br = Rect::new(px, py, bw, 34.0);
        let hov = hovered == Some(id);
        let is_danger = (id == "lk_shutdown" && confirm_pwr == Some("shutdown"))
            || (id == "lk_restart" && confirm_pwr == Some("restart"));
        ctx.fill_card(
            br,
            17.0,
            if is_danger { theme::CORAL_BG } else if hov { theme::ELEVATED } else { theme::rgba(16, 16, 24, 0.72) },
            if is_danger { theme::CORAL } else if hov { theme::VIOLET_LIGHT } else { theme::rgba(255, 255, 255, 0.14) },
        );
        ctx.draw_vector_icon(icon, br.x + 16.0, br.y + 17.0, 12.0, if is_danger { theme::CORAL } else { theme::TEXT_PRIMARY });
        ctx.draw_text(lbl, Rect::new(br.x + 28.0, br.y, br.w - 34.0, 34.0), TextStyle::Small10Bold, if is_danger { theme::CORAL } else { theme::TEXT_PRIMARY });
        hits.push(Hit { id: id.into(), rect: br, act });
        px -= 8.0;
    }

    // 7. FLOATING WALLPAPER PICKER DRAWER (when user clicks "Fondos" or green + button!)
    if picker_open {
        let dw = 540.0f32.min(w - 48.0);
        let dh = 264.0;
        let dr = Rect::new(w - dw - 20.0, py - dh - 12.0, dw, dh);
        ctx.fill_card(dr, 20.0, theme::rgba(15, 15, 23, 0.94), theme::VIOLET_BORDER);
        ctx.draw_text("Fondos de AuraLock", Rect::new(dr.x + 18.0, dr.y + 14.0, 200.0, 20.0), TextStyle::Title14Semi, theme::TEXT_PRIMARY);

        let browse_r = Rect::new(dr.x + dr.w - 176.0, dr.y + 11.0, 160.0, 26.0);
        ctx.fill_rrect(browse_r, 10.0, theme::VIOLET);
        ctx.draw_text_center("+ Elegir imagen/video", browse_r, TextStyle::Small10Bold, theme::TEXT_PRIMARY);
        hits.push(Hit { id: "lk_pick_file".into(), rect: browse_r, act: LockAct::PickLocalWallpaper });

        let items: Vec<&WallpaperItem> = wallpapers.iter().filter(|wp| wp.id != "external").take(8).collect();
        let cols = 4usize;
        let cw = (dr.w - 36.0 - 24.0) / cols as f32;
        let ch = 92.0;
        for (i, wp) in items.iter().enumerate() {
            let col = i % cols;
            let row = i / cols;
            let cr = Rect::new(dr.x + 18.0 + col as f32 * (cw + 8.0), dr.y + 48.0 + row as f32 * (ch + 10.0), cw, ch);
            let active = wp.id == active_wp_id;
            ctx.fill_card(cr, 12.0, theme::CARD_BG, if active { theme::VIOLET_LIGHT } else { theme::BORDER });
            let img_r = Rect::new(cr.x + 4.0, cr.y + 4.0, cr.w - 8.0, cr.h - 26.0);
            ctx.push_clip(img_r);
            let drew = wp.preview_path.as_deref().map(|p| ctx.draw_image_file(p, img_r, 1.0)).unwrap_or(false);
            if !drew {
                ctx.fill_gradient_rrect(img_r, 8.0, theme::VIOLET_DEEP, theme::CARD_BG);
            }
            ctx.pop_clip();
            ctx.draw_text(&wp.title, Rect::new(cr.x + 8.0, cr.y + cr.h - 20.0, cr.w - 16.0, 16.0), TextStyle::Small9Reg, theme::TEXT_PRIMARY);
            hits.push(Hit { id: format!("lk_wp_{}", i), rect: cr, act: LockAct::SelectWallpaper(wp.id.clone()) });
        }
    }
}

fn current_clock_and_date() -> (String, String) {
    use windows::Win32::System::SystemInformation::GetLocalTime;
    let st = unsafe { GetLocalTime() };
    let time = format!("{:02}:{:02}", st.wHour, st.wMinute);
    let dow = match st.wDayOfWeek {
        0 => "domingo",
        1 => "lunes",
        2 => "martes",
        3 => "miércoles",
        4 => "jueves",
        5 => "viernes",
        _ => "sábado",
    };
    let mon = match st.wMonth {
        1 => "enero",
        2 => "febrero",
        3 => "marzo",
        4 => "abril",
        5 => "mayo",
        6 => "junio",
        7 => "julio",
        8 => "agosto",
        9 => "septiembre",
        10 => "octubre",
        11 => "noviembre",
        _ => "diciembre",
    };
    (time, format!("{}, {} de {}", dow, st.wDay, mon))
}

unsafe extern "system" fn lock_wnd_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if msg == WM_CLOSE {
        let should_exit = LOCK_RT.with(|c| {
            c.try_borrow()
                .ok()
                .and_then(|g| g.as_ref().map(|r| !r.is_protected()))
                .unwrap_or(true)
        });
        if should_exit {
            let _ = ShowWindow(hwnd, SW_HIDE);
            PostQuitMessage(0);
        }
        return LRESULT(0);
    }
    if msg == WM_DESTROY {
        PostQuitMessage(0);
        return LRESULT(0);
    }

    let handled = LOCK_RT.with(|cell| {
        let Ok(mut opt) = cell.try_borrow_mut() else { return false };
        let Some(rt) = opt.as_mut() else { return false };

        match msg {
            WM_TIMER if wparam.0 == TIMER_FRAME => {
                rt.tick();
                true
            }
            WM_MOUSEMOVE => {
                let px = (lparam.0 & 0xFFFF) as i16 as f32;
                let py = ((lparam.0 >> 16) & 0xFFFF) as i16 as f32;
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
                } else if rt.picker_open {
                    rt.picker_open = false;
                    rt.redraw();
                } else {
                    rt.confirm_pwr = None;
                    rt.redraw();
                }
                true
            }
            WM_KEYDOWN => {
                let vk = wparam.0 as u16;
                if vk == VK_RETURN.0 {
                    rt.try_unlock();
                } else if vk == VK_BACK.0 {
                    rt.input.pop();
                    rt.error_msg.clear();
                    rt.redraw();
                } else if vk == VK_ESCAPE.0 {
                    if rt.picker_open {
                        rt.picker_open = false;
                        rt.redraw();
                    } else if !rt.is_protected() {
                        rt.unlock_and_exit();
                    } else {
                        rt.input.clear();
                        rt.error_msg.clear();
                        rt.redraw();
                    }
                } else if vk == VK_SPACE.0 && !rt.is_protected() {
                    rt.unlock_and_exit();
                }
                true
            }
            WM_CHAR => {
                if let Some(ch) = char::from_u32(wparam.0 as u32) {
                    if !ch.is_control() && rt.is_protected() && rt.input.len() < 32 {
                        rt.input.push(ch);
                        rt.error_msg.clear();
                        rt.redraw();
                    }
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

fn capture_lock_png(out_png: &Path) {
    let Ok(mut gpu) = GpuEngine::new() else { return };
    let settings = load_settings();
    let custom = load_custom_items();
    let wallpapers = wallpaper::scan_wallpapers(&custom);
    let active_wp = if settings.lock_wallpaper_id.trim().is_empty() {
        settings.wallpaper_id.clone()
    } else {
        settings.lock_wallpaper_id.clone()
    };
    let active_img = active_wp
        .strip_prefix("image:")
        .map(|s| s.to_string())
        .or_else(|| wallpapers.iter().find(|w| w.id == active_wp).and_then(|w| w.preview_path.clone()));
    let username = if !settings.lock_display_name.trim().is_empty() {
        settings.lock_display_name.clone()
    } else {
        std::env::var("USERNAME").unwrap_or_else(|_| "Usuario".into())
    };
    let avatar_path = if !settings.lock_avatar_path.trim().is_empty() && Path::new(&settings.lock_avatar_path).exists() {
        Some(settings.lock_avatar_path.clone())
    } else {
        None
    };
    let weather = system::load_cached_weather().unwrap_or_else(system::fetch_weather_blocking);
    let np = NowPlaying {
        title: "Midnight City".into(),
        artist: "M83".into(),
        source_app: "Spotify".into(),
        is_playing: true,
        active: true,
        position_sec: 92.0,
        duration_sec: 243.0,
        has_timeline: true,
        sampled_at_unix_ms: 0,
    };

    let _ = gpu.render_to_png(1440, 900, out_png, |ctx| {
        let mut hits = Vec::new();
        render_lock_screen(
            ctx,
            1440.0,
            900.0,
            &active_wp,
            active_img.as_deref(),
            avatar_path.as_deref(),
            None,
            &username,
            true,
            false,
            true,
            4,
            "",
            0.0,
            1.4,
            false,
            &wallpapers,
            &weather,
            &np,
            true,
            None,
            None,
            &mut hits,
        );
    });
}

fn main() {
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }

    let args: Vec<String> = std::env::args().collect();
    if args.len() >= 3 && args[1].eq_ignore_ascii_case("--capture") {
        capture_lock_png(Path::new(&args[2]));
        return;
    }

    if !win32_shell::acquire_single_instance_mutex("AuraLock_SingleInstance_v43") {
        return;
    }

    let Ok(mut gpu) = GpuEngine::new() else { return };
    gpu.enable_async_loader();
    let (sw, sh) = win32_shell::screen_size();
    let settings = load_settings();
    let custom = load_custom_items();
    let wallpapers = wallpaper::scan_wallpapers(&custom);
    gpu.prewarm_images(wallpapers.iter().filter_map(|w| w.preview_path.clone()));

    let hinst = unsafe { GetModuleHandleW(None).unwrap_or_default() };
    let cls_name = win32_shell::to_wide("AuraLock_GPU_v43_Wnd");
    unsafe {
        let wc = WNDCLASSW {
            lpfnWndProc: Some(lock_wnd_proc),
            hInstance: hinst.into(),
            hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
            lpszClassName: PCWSTR(cls_name.as_ptr()),
            ..Default::default()
        };
        RegisterClassW(&wc);
    }

    let title = win32_shell::to_wide("AuraLock");
    let hwnd = unsafe {
        CreateWindowExW(
            WS_EX_NOREDIRECTIONBITMAP | WS_EX_TOPMOST,
            PCWSTR(cls_name.as_ptr()),
            PCWSTR(title.as_ptr()),
            WS_POPUP,
            0,
            0,
            sw,
            sh,
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
    let Ok(surf) = WindowSurface::new(&gpu, hwnd, sw as u32, sh as u32) else { return };

    // Async MTA weather & now-playing poller (0ms UI block!)
    let (bg_tx, bg_rx) = mpsc::channel();
    let tx_w = bg_tx.clone();
    std::thread::spawn(move || {
        let _ = tx_w.send(LockBgMsg::Weather(system::fetch_weather_blocking()));
    });
    std::thread::spawn(move || {
        unsafe {
            let _ = windows::Win32::System::Com::CoInitializeEx(
                None,
                windows::Win32::System::Com::COINIT_MULTITHREADED,
            );
        }
        loop {
            let np = audio::get_now_playing();
            if bg_tx.send(LockBgMsg::Media(np)).is_err() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(1200));
        }
    });

    let username = std::env::var("USERNAME").unwrap_or_else(|_| "Usuario".into());
    let display_name = if !settings.lock_display_name.trim().is_empty() {
        settings.lock_display_name.clone()
    } else {
        username.clone()
    };
    let init_wp = if settings.lock_wallpaper_id.trim().is_empty() {
        settings.wallpaper_id.clone()
    } else {
        settings.lock_wallpaper_id.clone()
    };

    let mut rt = LockRuntime {
        hwnd,
        gpu,
        surf,
        width: sw as f32,
        height: sh as f32,
        settings,
        wallpapers,
        active_wp_id: init_wp.clone(),
        active_img_path: None,
        video: None,
        video_seq: 0,
        video_dims: (0, 0),
        video_buf: Vec::new(),
        username,
        display_name,
        input: String::new(),
        error_msg: String::new(),
        shake_t: 0.0,
        phase: 0.0,
        fade_in: 0.0,
        picker_open: false,
        now_playing: NowPlaying::default(),
        weather: system::load_cached_weather().unwrap_or_default(),
        bg_rx,
        hits: Vec::new(),
        hovered_id: None,
        last_frame: Instant::now(),
        confirm_pwr: None,
    };
    rt.apply_wallpaper(&init_wp);
    rt.redraw();

    LOCK_RT.with(|c| *c.borrow_mut() = Some(rt));

    unsafe {
        let _ = ShowWindow(hwnd, SW_SHOW);
        win32_shell::bring_to_front(hwnd);
        let _ = SetFocus(hwnd);
        SetTimer(hwnd, TIMER_FRAME, 33, None);

        let mut msg = MSG::default();
        while GetMessageW(&mut msg, HWND::default(), 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        let _ = KillTimer(hwnd, TIMER_FRAME);
    }

    LOCK_RT.with(|c| {
        if let Ok(mut g) = c.try_borrow_mut() {
            *g = None;
        }
    });
}
