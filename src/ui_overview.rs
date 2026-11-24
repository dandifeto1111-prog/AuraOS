//! 0ms Instant Command Launcher + Impeccable Drool Bento Overview (PC / Dev & AI / Gamer / Settings)
//! - Emil Kowalski Rule #1: 0ms open on keyboard Alt+Space (Raycast feel)
//! - Instant 0ms data swap between PC, Dev & AI, Gamer + smooth 160ms sliding tab pill
//! - Manual "+ Agregar IA Web", "+ App local" in Dev & AI, and "+ Agregar juego" in Gamer!

use crate::models::{ShellMode, WallpaperKind, WidgetKind};
use crate::renderer::{DrawCtx, Rect, TextStyle};
use crate::theme::{self, VectorIcon};
use crate::ui_state::{Action, AppState, HitRegion, SliderDragKind};

pub fn render_overview(ctx: &mut DrawCtx<'_>, state: &AppState, w: f32, h: f32, hits: &mut Vec<HitRegion>) {
    hits.clear();

    let open_t = state.overview_anim.value.clamp(0.0, 1.0);
    if open_t <= 0.01 {
        return;
    }

    // Anchor origin smoothly at the Top Bar trigger anchor!
    let ox = state.overview_origin_x.clamp(80.0, w - 80.0);
    let oy = 12.0;
    let dx_max = ox.max(w - ox);
    let max_r = (dx_max * dx_max + h * h).sqrt() + 28.0;
    let cur_r = 28.0 + (max_r - 28.0) * open_t;

    let full_r = Rect::new(0.0, 0.0, w, h);
    let _outer_layer: Option<windows::Win32::Graphics::Direct2D::ID2D1Layer> = None;

    let alpha = state.settings.glass_opacity.clamp(0.50, 0.96) * open_t;
    ctx.fill_rrect(full_r, 0.0, theme::rgba(10, 10, 15, alpha));

    // Zero-layer GPU origin-aware scale + slide (0.4ms even during 100% GPU games!)
    let scale = 0.96 + 0.04 * open_t;
    let rise_y = (1.0 - open_t) * -8.0;
    ctx.set_transform_scale_origin(scale, ox, oy, 0.0, rise_y);

    // Clicking empty backdrop closes Overview (unless modal is open)
    hits.push(HitRegion {
        id: "ov_backdrop".into(),
        rect: full_r,
        action: if state.add_modal.open { Action::CancelModal } else { Action::CloseAllOverlays },
        right_action: None,
        tooltip: None,
        slider: None,
    });

    // ==================== TOP COMMAND BAR ====================
    let top_y = 20.0;
    let margin_x = ((w - 1180.0) * 0.5).max(84.0);
    let content_w = (w - margin_x * 2.0).min(1220.0);

    // 4-Mode Segmented Pill (Overview / Dev & AI / Gamer / Fondos)
    let seg_r = Rect::new(margin_x, top_y, 356.0, 38.0);
    ctx.fill_card(seg_r, 19.0, theme::SURFACE, theme::BORDER);
    let tab_w = (seg_r.w - 8.0) / 4.0;
    let pill_x = seg_r.x + 4.0 + state.overview_tab_pill_x.value.clamp(0.0, 3.0) * tab_w;
    if !state.overview_settings_view {
        ctx.fill_rrect(Rect::new(pill_x, seg_r.y + 4.0, tab_w, 30.0), 15.0, theme::VIOLET);
    }
    let tabs = [
        ("ov_tab_pc", "Overview", ShellMode::PC),
        ("ov_tab_dev", "Dev & AI", ShellMode::Dev),
        ("ov_tab_gamer", "Gamer", ShellMode::Gamer),
        ("ov_tab_studio", "Fondos", ShellMode::Studio),
    ];
    for (i, (id, lbl, m)) in tabs.into_iter().enumerate() {
        let tr = Rect::new(seg_r.x + 4.0 + i as f32 * tab_w, seg_r.y + 4.0, tab_w, 30.0);
        let active = !state.overview_settings_view && state.mode == m;
        ctx.draw_text_center(
            lbl,
            tr,
            if active { TextStyle::Body11Med } else { TextStyle::Body11Reg },
            if active { theme::TEXT_PRIMARY } else { theme::TEXT_SECONDARY },
        );
        hits.push(HitRegion { id: id.into(), rect: tr, action: Action::ClickModeTab(m), right_action: None, tooltip: None, slider: None });
    }

    // Centered Instant Search Pill (Raycast-style — clickable focus + live caret + clear button!)
    let sw = 340.0f32.min(content_w - 580.0).max(210.0);
    let sx = margin_x + 372.0 + ((content_w - 372.0 - 216.0 - sw) * 0.5).max(0.0);
    let sr = Rect::new(sx, top_y, sw, 38.0);
    let sr_hov = state.hovered_id.as_deref() == Some("ov_search");
    ctx.fill_card(
        sr,
        19.0,
        if sr_hov || !state.search_query.is_empty() { theme::ELEVATED } else { theme::SURFACE },
        if !state.search_query.is_empty() || sr_hov { theme::VIOLET_LIGHT } else { theme::VIOLET_BORDER },
    );
    ctx.draw_vector_icon(
        VectorIcon::Search,
        sr.x + 20.0,
        sr.y + 19.0,
        13.5,
        if !state.search_query.is_empty() || sr_hov { theme::VIOLET_LIGHT } else { theme::TEXT_TERTIARY },
    );
    let q_disp = if state.search_query.is_empty() {
        "Buscar apps, calcular (245*18, 100c a f) o > cmd...".to_string()
    } else {
        format!("{}|", state.search_query)
    };
    ctx.draw_text(
        &q_disp,
        Rect::new(sr.x + 38.0, sr.y, sw - 66.0, 38.0),
        TextStyle::Body12Med,
        if state.search_query.is_empty() { theme::TEXT_TERTIARY } else { theme::TEXT_PRIMARY },
    );
    hits.push(HitRegion {
        id: "ov_search".into(),
        rect: sr,
        action: Action::FocusSearchBar,
        right_action: None,
        tooltip: None,
        slider: None,
    });
    if !state.search_query.is_empty() {
        let clr_r = Rect::new(sr.x + sr.w - 30.0, sr.y + 7.0, 24.0, 24.0);
        ctx.draw_vector_icon(VectorIcon::Close, clr_r.x + 12.0, clr_r.y + 12.0, 9.5, theme::TEXT_SECONDARY);
        hits.push(HitRegion {
            id: "ov_search_clr".into(),
            rect: clr_r,
            action: Action::ClearSearchBar,
            right_action: None,
            tooltip: None,
            slider: None,
        });
    }

    // Right Action Buttons: AuraFiles · Settings · Esc
    let rx = margin_x + content_w;
    let esc_r = Rect::new(rx - 48.0, top_y + 2.0, 48.0, 34.0);
    ctx.fill_card(esc_r, 12.0, theme::SURFACE, theme::BORDER);
    ctx.draw_text_center("Esc", esc_r, TextStyle::Body11Med, theme::TEXT_SECONDARY);
    hits.push(HitRegion { id: "ov_esc".into(), rect: esc_r, action: Action::CloseAllOverlays, right_action: None, tooltip: None, slider: None });

    let set_r = Rect::new(rx - 90.0, top_y + 2.0, 34.0, 34.0);
    ctx.fill_card(
        set_r,
        12.0,
        if state.overview_settings_view { theme::VIOLET } else { theme::SURFACE },
        theme::BORDER,
    );
    ctx.draw_vector_icon(VectorIcon::Settings, set_r.x + 17.0, set_r.y + 17.0, 14.0, theme::TEXT_PRIMARY);
    hits.push(HitRegion { id: "ov_set".into(), rect: set_r, action: Action::OpenSettings, right_action: None, tooltip: None, slider: None });

    let af_r = Rect::new(rx - 200.0, top_y + 2.0, 102.0, 34.0);
    ctx.fill_rrect(af_r, 12.0, theme::VIOLET);
    ctx.draw_vector_icon(VectorIcon::Folder, af_r.x + 18.0, af_r.y + 17.0, 13.0, theme::TEXT_PRIMARY);
    ctx.draw_text("AuraFiles", Rect::new(af_r.x + 31.0, af_r.y, 66.0, 34.0), TextStyle::Body11Med, theme::TEXT_PRIMARY);
    hits.push(HitRegion { id: "ov_files".into(), rect: af_r, action: Action::LaunchAuraFiles(None), right_action: None, tooltip: None, slider: None });

    // ==================== MAIN CONTENT AREA (WITH DIRECTIONAL TAB SLIDE!) ====================
    let body_y = 86.0;
    let body_h = (h - body_y - 36.0).max(460.0);
    let body_r = Rect::new(margin_x, body_y, content_w, body_h);

    let tab_t = state.tab_content_anim.value.clamp(0.0, 1.0);
    let slide_x = (1.0 - tab_t) * 24.0 * state.tab_slide_dir;
    let body_r_anim = Rect::new(body_r.x + slide_x, body_r.y, body_r.w, body_r.h);
    let _tab_layer = if tab_t < 0.995 {
        ctx.push_opacity_layer(Rect::new(0.0, body_y - 10.0, w, body_h + 30.0), tab_t)
    } else {
        None
    };

    if !state.search_query.trim().is_empty() && !state.add_modal.open {
        render_search_results(ctx, state, body_r_anim, hits);
    } else if state.overview_settings_view {
        render_settings_view(ctx, state, body_r_anim, hits);
    } else {
        match state.mode {
            ShellMode::PC => render_pc_bento(ctx, state, body_r_anim, hits),
            ShellMode::Dev => render_dev_bento(ctx, state, body_r_anim, hits),
            ShellMode::Gamer => render_gamer_bento(ctx, state, body_r_anim, hits),
            ShellMode::Studio => render_studio_bento(ctx, state, body_r_anim, hits),
        }
    }

    if _tab_layer.is_some() {
        ctx.pop_layer();
    }

    // ==================== INLINE MODAL: + AGREGAR IA / APP WEB ====================
    if state.add_modal.open {
        render_add_dev_modal(ctx, state, w, h, hits);
    }

    ctx.reset_transform();
    if _outer_layer.is_some() {
        ctx.pop_layer();
    }

    // Subtle expanding Electric Violet halo ring on open/close reveal
    if open_t > 0.03 && open_t < 0.93 {
        ctx.stroke_circle(ox, oy, cur_r, theme::rgba(155, 132, 255, (1.0 - open_t) * 0.50), 1.8);
    }
}

// ==================== 1. PC OVERVIEW BENTO (DROOL DARK MODE) ====================

fn render_pc_bento(ctx: &mut DrawCtx<'_>, state: &AppState, area: Rect, hits: &mut Vec<HitRegion>) {
    let gap = 16.0;
    let col_w = (area.w - gap * 2.0) / 3.0;
    let x0 = area.x;
    let x1 = area.x + col_w + gap;
    let x2 = area.x + (col_w + gap) * 2.0;
    let y0 = area.y + 18.0;

    // --- LEFT COLUMN ---
    let half_w = (col_w - 12.0) * 0.5;
    // Card 1A: Sistema stats
    let sys_card = Rect::new(x0, y0, half_w, 128.0);
    ctx.fill_card(sys_card, 20.0, theme::CARD_BG, theme::BORDER);
    ctx.draw_text("Sistema", Rect::new(sys_card.x + 16.0, sys_card.y + 14.0, 100.0, 20.0), TextStyle::Body12Semi, theme::TEXT_PRIMARY);
    ctx.fill_circle(sys_card.x + sys_card.w - 24.0, sys_card.y + 24.0, 13.0, theme::CRISP_WHITE);
    ctx.draw_vector_icon(VectorIcon::AppGrid, sys_card.x + sys_card.w - 24.0, sys_card.y + 24.0, 11.0, theme::DARK_INK);

    ctx.draw_text(&state.all_apps.len().to_string(), Rect::new(sys_card.x + 16.0, sys_card.y + 56.0, 68.0, 30.0), TextStyle::Title22Bold, theme::TEXT_PRIMARY);
    ctx.draw_text("Apps", Rect::new(sys_card.x + 16.0, sys_card.y + 92.0, 60.0, 16.0), TextStyle::Small10Med, theme::CORAL);

    ctx.draw_text(&state.games.len().to_string(), Rect::new(sys_card.x + 88.0, sys_card.y + 56.0, 68.0, 30.0), TextStyle::Title22Bold, theme::TEXT_PRIMARY);
    ctx.draw_text("Juegos", Rect::new(sys_card.x + 88.0, sys_card.y + 92.0, 60.0, 16.0), TextStyle::Small10Med, theme::VIOLET_LIGHT);

    // Card 1B: AuraFiles shortcut
    let af_card = Rect::new(x0 + half_w + 12.0, y0, half_w, 128.0);
    let af_hov = state.hovered_id.as_deref() == Some("pc_af_card");
    ctx.fill_card(af_card, 20.0, if af_hov { theme::ELEVATED } else { theme::CARD_BG }, theme::BORDER);
    ctx.draw_text("AuraFiles", Rect::new(af_card.x + 16.0, af_card.y + 14.0, 110.0, 20.0), TextStyle::Body12Semi, theme::TEXT_PRIMARY);
    ctx.draw_vector_icon(VectorIcon::External, af_card.x + af_card.w - 22.0, af_card.y + 24.0, 12.0, theme::TEXT_SECONDARY);
    let drv_txt = format!("{} discos listos", state.drives.len());
    ctx.draw_text(&drv_txt, Rect::new(af_card.x + 16.0, af_card.y + 62.0, af_card.w - 28.0, 24.0), TextStyle::Title14Semi, theme::VIOLET_LIGHT);
    ctx.draw_text("Instantáneo · Pestañas · IA", Rect::new(af_card.x + 16.0, af_card.y + 92.0, af_card.w - 28.0, 16.0), TextStyle::Small10Med, theme::TEXT_TERTIARY);
    hits.push(HitRegion { id: "pc_af_card".into(), rect: af_card, action: Action::LaunchAuraFiles(None), right_action: None, tooltip: None, slider: None });

    // Card 2: Drool Hatched Opacity Slider
    let op_card = Rect::new(x0, y0 + 142.0, col_w, 86.0);
    ctx.fill_card(op_card, 20.0, theme::CARD_BG, theme::BORDER);
    let tr = Rect::new(op_card.x + 18.0, op_card.y + 18.0, op_card.w - 36.0, 24.0);
    ctx.fill_hatched_track(tr, 12.0);
    let op_frac = ((state.settings.glass_opacity - 0.50) / 0.46).clamp(0.0, 1.0);
    ctx.fill_rrect(Rect::new(tr.x, tr.y, (op_frac * tr.w).max(24.0), 24.0), 12.0, theme::VIOLET);
    let kx = (tr.x + op_frac * (tr.w - 24.0) + 12.0).clamp(tr.x + 12.0, tr.x + tr.w - 12.0);
    ctx.fill_circle(kx, tr.y + 12.0, 11.0, theme::CRISP_WHITE);
    ctx.stroke_circle(kx, tr.y + 12.0, 11.0, theme::VIOLET, 2.0);
    ctx.draw_text("Transparencia", Rect::new(op_card.x + 18.0, op_card.y + 52.0, 100.0, 18.0), TextStyle::Small10Med, theme::TEXT_TERTIARY);
    ctx.draw_text_right("Sólido", Rect::new(op_card.x + op_card.w - 80.0, op_card.y + 52.0, 62.0, 18.0), TextStyle::Small10Med, theme::TEXT_TERTIARY);
    hits.push(HitRegion {
        id: "pc_opacity".into(),
        rect: tr,
        action: Action::SetGlassOpacity(state.settings.glass_opacity),
        right_action: None,
        tooltip: None,
        slider: Some(SliderDragKind::GlassOpacity { x: tr.x, w: tr.w }),
    });

    // Card 3: Almacenamiento local (Drool vertical pillar bars)
    let st_card = Rect::new(x0, y0 + 242.0, col_w, 250.0);
    ctx.fill_card(st_card, 22.0, theme::CARD_BG, theme::BORDER);
    let ic_s = Rect::new(st_card.x + 16.0, st_card.y + 16.0, 28.0, 28.0);
    ctx.fill_rrect(ic_s, 9.0, theme::VIOLET);
    ctx.draw_vector_icon(VectorIcon::Folder, ic_s.x + 14.0, ic_s.y + 14.0, 13.0, theme::TEXT_PRIMARY);
    ctx.draw_text("Almacenamiento local", Rect::new(st_card.x + 54.0, st_card.y + 14.0, 180.0, 18.0), TextStyle::Body12Semi, theme::TEXT_PRIMARY);
    ctx.draw_text("Clic en cualquier disco para abrir", Rect::new(st_card.x + 54.0, st_card.y + 32.0, 200.0, 14.0), TextStyle::Small9Reg, theme::TEXT_TERTIARY);

    let total_free: f64 = state.drives.iter().map(|d| d.free_gb).sum();
    let free_pill = Rect::new(st_card.x + st_card.w - 94.0, st_card.y + 18.0, 78.0, 22.0);
    ctx.fill_rrect(free_pill, 11.0, theme::ELEVATED);
    ctx.draw_text_center(&format!("{:.0} GB libres", total_free), free_pill, TextStyle::Small9Reg, theme::TEXT_SECONDARY);

    let max_bar_h = 114.0;
    let base_y = st_card.y + 194.0;
    let d_count = state.drives.len().max(1);
    let slot_w = (st_card.w - 36.0) / d_count as f32;
    for (i, d) in state.drives.iter().enumerate() {
        let bx = st_card.x + 18.0 + i as f32 * slot_w + (slot_w - 36.0) * 0.5;
        let bh = ((d.used_pct / 100.0) * max_bar_h).clamp(26.0, max_bar_h);
        let is_high = d.used_pct >= 75.0;
        let br = Rect::new(bx, base_y - bh, 36.0, bh);
        ctx.fill_rrect(br, 10.0, if is_high { theme::CRISP_WHITE } else { theme::rgb(72, 76, 98) });
        if is_high {
            let pct_r = Rect::new(bx + 4.0, base_y - bh * 0.5 - 9.0, 28.0, 18.0);
            ctx.fill_rrect(pct_r, 6.0, theme::VIOLET);
            ctx.draw_text_center(&format!("{:.0}%", d.used_pct), pct_r, TextStyle::Small9Reg, theme::TEXT_PRIMARY);
        }
        ctx.draw_text_center(&d.letter, Rect::new(bx - 12.0, base_y + 8.0, 60.0, 16.0), TextStyle::Body11Med, theme::TEXT_PRIMARY);
        ctx.draw_text_center(&format!("{:.0} GB", d.free_gb), Rect::new(bx - 12.0, base_y + 24.0, 60.0, 14.0), TextStyle::Small9Reg, theme::TEXT_TERTIARY);
        hits.push(HitRegion {
            id: format!("pc_drv_{}", i),
            rect: Rect::new(bx - 10.0, base_y - max_bar_h, 56.0, max_bar_h + 44.0),
            action: Action::LaunchAuraFiles(Some(format!("{}\\", d.letter))),
            right_action: None,
            tooltip: Some(format!("Abrir disco {} en AuraFiles", d.letter)),
            slider: None,
        });
    }

    // --- CENTER COLUMN: VENTANAS Y ESPACIOS ACTIVOS ---
    let mid_card = Rect::new(x1, y0, col_w, 492.0);
    ctx.fill_card(mid_card, 22.0, theme::CARD_BG, theme::BORDER);
    ctx.draw_text("VENTANAS Y ACCIONES RÁPIDAS", Rect::new(mid_card.x + 18.0, mid_card.y + 18.0, col_w - 36.0, 16.0), TextStyle::Small10Bold, theme::TEXT_TERTIARY);

    let mut ry = mid_card.y + 46.0;
    for (idx, win) in state.open_windows.iter().take(6).enumerate() {
        let rr = Rect::new(mid_card.x + 14.0, ry, col_w - 28.0, 44.0);
        let id = format!("pc_win_{}", idx);
        let hov = state.hovered_id.as_deref() == Some(&id);
        let bg = if idx == 0 { theme::VIOLET } else if hov { theme::ELEVATED_HOVER } else { theme::ELEVATED };
        ctx.fill_rrect(rr, 15.0, bg);
        ctx.fill_circle(rr.x + 16.0, rr.y + 22.0, 4.0, if idx == 0 { theme::CRISP_WHITE } else { theme::VIOLET_LIGHT });
        ctx.draw_text(&win.title, Rect::new(rr.x + 28.0, rr.y, rr.w - 110.0, 44.0), TextStyle::Body12Semi, theme::TEXT_PRIMARY);
        let tag = Rect::new(rr.x + rr.w - 76.0, rr.y + 11.0, 64.0, 22.0);
        ctx.fill_rrect(tag, 9.0, theme::rgba(0, 0, 0, 0.22));
        ctx.draw_text_center(if win.is_minimized { "Min" } else { "Activa" }, tag, TextStyle::Small9Reg, theme::TEXT_PRIMARY);
        hits.push(HitRegion { id, rect: rr, action: Action::FocusWindow(win.hwnd), right_action: None, tooltip: None, slider: None });
        ry += 52.0;
    }

    // Quick folders/tools below open windows
    let user_prof = std::env::var("USERPROFILE").unwrap_or_else(|_| "C:\\".to_string());
    let shortcuts = [
        ("pc_sc_dl", "AuraFiles · Descargas", Action::LaunchAuraFiles(Some(format!("{}\\Downloads", user_prof))), theme::VIOLET),
        ("pc_sc_doc", "AuraFiles · Documentos", Action::LaunchAuraFiles(Some(format!("{}\\Documents", user_prof))), theme::BLUE),
        ("pc_sc_term", "Terminal de Windows", Action::LaunchTarget("wt.exe".into()), theme::EMERALD),
        ("pc_sc_tm", "Administrador de Tareas", Action::LaunchTarget("taskmgr.exe".into()), theme::AMBER),
    ];
    for (id, label, act, dot) in shortcuts {
        if ry + 44.0 > mid_card.y + mid_card.h - 12.0 {
            break;
        }
        let rr = Rect::new(mid_card.x + 14.0, ry, col_w - 28.0, 44.0);
        let hov = state.hovered_id.as_deref() == Some(id);
        ctx.fill_rrect(rr, 15.0, if hov { theme::ELEVATED_HOVER } else { theme::ELEVATED });
        ctx.fill_circle(rr.x + 16.0, rr.y + 22.0, 4.0, dot);
        ctx.draw_text(label, Rect::new(rr.x + 28.0, rr.y, rr.w - 92.0, 44.0), TextStyle::Body12Med, theme::TEXT_PRIMARY);
        let pill = Rect::new(rr.x + rr.w - 58.0, rr.y + 11.0, 46.0, 22.0);
        ctx.fill_rrect(pill, 9.0, theme::rgba(255, 255, 255, 0.08));
        ctx.draw_text_center("Abrir", pill, TextStyle::Small9Reg, theme::TEXT_SECONDARY);
        hits.push(HitRegion { id: id.into(), rect: rr, action: act, right_action: None, tooltip: None, slider: None });
        ry += 52.0;
    }

    // --- RIGHT COLUMN ---
    // Card R1: Electric Violet Weather Card (Vector sun/cloud + Feels/Humidity/Wind)
    let w_card = Rect::new(x2, y0, col_w, 148.0);
    ctx.fill_gradient_rrect(w_card, 22.0, theme::VIOLET_LIGHT, theme::VIOLET_DEEP);
    let w_ic = Rect::new(w_card.x + 18.0, w_card.y + 20.0, 42.0, 42.0);
    ctx.fill_rrect(w_ic, 14.0, theme::rgba(255, 255, 255, 0.22));
    ctx.draw_vector_icon(
        if state.weather.weather_code <= 1 { VectorIcon::Sun } else { VectorIcon::Cloud },
        w_ic.x + 21.0,
        w_ic.y + 21.0,
        20.0,
        if state.weather.weather_code <= 1 { theme::AMBER } else { theme::TEXT_PRIMARY },
    );
    ctx.draw_text("Clima en vivo", Rect::new(w_card.x + 70.0, w_card.y + 22.0, 140.0, 16.0), TextStyle::Small10Med, theme::rgba(240, 235, 255, 0.85));
    ctx.draw_text(&format!("{} · {}", state.weather.city, state.weather.condition), Rect::new(w_card.x + 70.0, w_card.y + 40.0, 180.0, 18.0), TextStyle::Body12Semi, theme::TEXT_PRIMARY);
    ctx.draw_text_right(&format!("{}°", state.weather.temp_c), Rect::new(w_card.x + w_card.w - 92.0, w_card.y + 16.0, 74.0, 48.0), TextStyle::HeroTemp34Bold, theme::TEXT_PRIMARY);

    let pw = (col_w - 52.0) / 3.0;
    let w_pills = [
        format!("Sens. {}°", state.weather.feels_like_c),
        format!("Hum. {}%", state.weather.humidity_pct),
        format!("{} km/h", state.weather.wind_kmh),
    ];
    for (i, txt) in w_pills.into_iter().enumerate() {
        let pr = Rect::new(w_card.x + 18.0 + i as f32 * (pw + 8.0), w_card.y + 98.0, pw, 28.0);
        ctx.fill_rrect(pr, 10.0, theme::rgba(0, 0, 0, 0.20));
        ctx.draw_text_center(&txt, pr, TextStyle::Small10Med, theme::TEXT_PRIMARY);
    }

    // Card R2: Crisp Pure White CPU Real Area Graph Card (Editorial Drool Contrast!)
    let c_card = Rect::new(x2, y0 + 164.0, col_w, 148.0);
    ctx.fill_rrect(c_card, 22.0, theme::CRISP_WHITE);
    ctx.draw_text("Uso de CPU", Rect::new(c_card.x + 18.0, c_card.y + 14.0, 140.0, 20.0), TextStyle::Body12Semi, theme::DARK_INK);
    ctx.draw_text("Historial de carga en tiempo real", Rect::new(c_card.x + 18.0, c_card.y + 32.0, 200.0, 14.0), TextStyle::Small9Reg, theme::DARK_MUTED);
    ctx.draw_text_right(&format!("{}%", state.cpu_pct.round() as i32), Rect::new(c_card.x + c_card.w - 86.0, c_card.y + 14.0, 68.0, 32.0), TextStyle::Title22Bold, theme::DARK_INK);

    ctx.draw_area_graph(
        Rect::new(c_card.x + 18.0, c_card.y + 56.0, c_card.w - 36.0, 56.0),
        &state.cpu_history,
        theme::VIOLET,
    );
    ctx.draw_text("Intel Core i3-10100F", Rect::new(c_card.x + 18.0, c_card.y + 120.0, 160.0, 16.0), TextStyle::Small10Med, theme::DARK_MUTED);
    ctx.draw_text_right("3.60 GHz · 4 núcleos", Rect::new(c_card.x + c_card.w - 150.0, c_card.y + 120.0, 132.0, 16.0), TextStyle::Small10Med, theme::VIOLET);

    // Card R3: RAM Arc Gauge Card
    let r_card = Rect::new(x2, y0 + 328.0, col_w, 164.0);
    ctx.fill_card(r_card, 22.0, theme::CARD_BG, theme::BORDER);
    let gcx = r_card.x + r_card.w * 0.5;
    let gcy = r_card.y + 84.0;
    ctx.draw_arc_gauge(gcx, gcy, 54.0, 9.0, state.ram_pct as f32);
    ctx.fill_circle(gcx, gcy, 24.0, theme::CRISP_WHITE);
    ctx.draw_text_center(&format!("{}%", state.ram_pct), Rect::new(gcx - 24.0, gcy - 12.0, 48.0, 24.0), TextStyle::Body12Semi, theme::DARK_INK);

    ctx.draw_text("Memoria RAM", Rect::new(r_card.x + 18.0, r_card.y + 114.0, 120.0, 16.0), TextStyle::Small10Med, theme::TEXT_TERTIARY);
    ctx.draw_text(&format!("{:.1} / {:.1} GB", state.ram_used_gb, state.ram_total_gb), Rect::new(r_card.x + 18.0, r_card.y + 132.0, 160.0, 20.0), TextStyle::Body12Semi, theme::TEXT_PRIMARY);
    let st_pill = Rect::new(r_card.x + r_card.w - 72.0, r_card.y + 128.0, 54.0, 22.0);
    ctx.fill_rrect(st_pill, 10.0, if state.ram_pct > 80 { theme::CORAL } else { theme::VIOLET });
    ctx.draw_text_center(if state.ram_pct > 80 { "Alta" } else { "Ideal" }, st_pill, TextStyle::Small9Reg, theme::TEXT_PRIMARY);
}

// ==================== 2. DEV & AI HUB (+ MANUAL ADD AI / LOCAL APP!) ====================

fn render_dev_bento(ctx: &mut DrawCtx<'_>, state: &AppState, area: Rect, hits: &mut Vec<HitRegion>) {
    let gap = 16.0;
    let left_w = area.w * 0.66 - gap * 0.5;
    let right_w = area.w - left_w - gap;
    let y0 = area.y + 14.0;

    // LEFT: AI & Dev Tools Card with Manual "+ Agregar IA Web" & "+ App local" Buttons!
    let ai_card = Rect::new(area.x, y0, left_w, area.h - 24.0);
    ctx.fill_card(ai_card, 22.0, theme::CARD_BG, theme::BORDER);
    ctx.draw_text("AI & DEV WORKSPACE", Rect::new(ai_card.x + 20.0, ai_card.y + 18.0, 200.0, 20.0), TextStyle::Small10Bold, theme::TEXT_TERTIARY);
    ctx.draw_text("Agrega cualquier IA web o herramienta local", Rect::new(ai_card.x + 20.0, ai_card.y + 34.0, 320.0, 16.0), TextStyle::Body11Reg, theme::TEXT_SECONDARY);

    // Manual "+ Agregar IA Web" button
    let btn_web = Rect::new(ai_card.x + ai_card.w - 266.0, ai_card.y + 18.0, 128.0, 30.0);
    ctx.fill_rrect(btn_web, 11.0, theme::VIOLET);
    ctx.draw_vector_icon(VectorIcon::Plus, btn_web.x + 15.0, btn_web.y + 15.0, 11.0, theme::TEXT_PRIMARY);
    ctx.draw_text("Agregar IA Web", Rect::new(btn_web.x + 26.0, btn_web.y, 98.0, 30.0), TextStyle::Small10Bold, theme::TEXT_PRIMARY);
    hits.push(HitRegion { id: "dev_add_web".into(), rect: btn_web, action: Action::OpenAddDevWebModal, right_action: None, tooltip: None, slider: None });

    // Manual "+ App local" button
    let btn_loc = Rect::new(ai_card.x + ai_card.w - 130.0, ai_card.y + 18.0, 112.0, 30.0);
    ctx.fill_card(btn_loc, 11.0, theme::ELEVATED, theme::BORDER_STRONG);
    ctx.draw_vector_icon(VectorIcon::Plus, btn_loc.x + 15.0, btn_loc.y + 15.0, 11.0, theme::VIOLET_LIGHT);
    ctx.draw_text("App local", Rect::new(btn_loc.x + 26.0, btn_loc.y, 82.0, 30.0), TextStyle::Small10Bold, theme::TEXT_PRIMARY);
    hits.push(HitRegion { id: "dev_add_local".into(), rect: btn_loc, action: Action::AddDevLocalFile, right_action: None, tooltip: None, slider: None });

    // Responsive 3-column grid of Dev & AI tools
    let cols = 3usize;
    let cw = (ai_card.w - 40.0 - 20.0) / cols as f32;
    let ch = 64.0;
    let gx = ai_card.x + 20.0;
    let gy = ai_card.y + 66.0;

    for (idx, item) in state.dev_ai_apps.iter().take(15).enumerate() {
        let c = (idx % cols) as f32;
        let r = (idx / cols) as f32;
        let tr = Rect::new(gx + c * (cw + 10.0), gy + r * (ch + 10.0), cw, ch);
        if tr.y + tr.h > ai_card.y + ai_card.h - 14.0 {
            break;
        }
        let id = format!("dev_item_{}", idx);
        let hov = state.hovered_id.as_deref() == Some(&id);
        ctx.fill_card(
            tr,
            15.0,
            if hov { theme::ELEVATED_HOVER } else { theme::ELEVATED },
            if item.is_custom { theme::VIOLET_BORDER } else { theme::BORDER },
        );

        let ib = Rect::new(tr.x + 12.0, tr.y + 14.0, 36.0, 36.0);
        ctx.fill_rrect(ib, 10.0, if item.is_web { theme::VIOLET_TINT } else { theme::CARD_BG });
        if item.icon_path.is_empty() || !ctx.draw_app_icon(&item.icon_path, Rect::new(ib.x + 6.0, ib.y + 6.0, 24.0, 24.0)) {
            ctx.draw_vector_icon(if item.is_web { VectorIcon::Sparkle } else { VectorIcon::Code }, ib.x + 18.0, ib.y + 18.0, 15.0, theme::VIOLET_LIGHT);
        }

        let tw = if item.is_custom { tr.w - 84.0 } else { tr.w - 60.0 };
        ctx.draw_text(&item.name, Rect::new(tr.x + 56.0, tr.y + 13.0, tw, 18.0), TextStyle::Body12Semi, theme::TEXT_PRIMARY);
        ctx.draw_text(&item.category, Rect::new(tr.x + 56.0, tr.y + 33.0, tw, 15.0), TextStyle::Small9Reg, theme::TEXT_TERTIARY);

        hits.push(HitRegion {
            id,
            rect: tr,
            action: Action::LaunchTarget(item.target.clone()),
            right_action: item.custom_id.as_ref().map(|cid| Action::RemoveCustomDevItem(cid.clone())),
            tooltip: None,
            slider: None,
        });

        // Clean delete badge on custom user-added items
        if let Some(cid) = &item.custom_id {
            let del_r = Rect::new(tr.x + tr.w - 26.0, tr.y + 8.0, 18.0, 18.0);
            ctx.fill_rrect(del_r, 9.0, theme::CORAL_BG);
            ctx.draw_vector_icon(VectorIcon::Close, del_r.x + 9.0, del_r.y + 9.0, 8.0, theme::CORAL);
            hits.push(HitRegion {
                id: format!("dev_del_{}", idx),
                rect: del_r,
                action: Action::RemoveCustomDevItem(cid.clone()),
                right_action: None,
                tooltip: Some("Quitar elemento personalizado".into()),
                slider: None,
            });
        }
    }

    // RIGHT: Local Dev Servers (Ports) + Quick Dev Actions
    let right_x = area.x + left_w + gap;
    let p_card = Rect::new(right_x, y0, right_w, area.h - 24.0);
    ctx.fill_card(p_card, 22.0, theme::CARD_BG, theme::BORDER);
    ctx.draw_text("SERVIDORES LOCALES ACTIVOS", Rect::new(p_card.x + 18.0, p_card.y + 18.0, right_w - 36.0, 18.0), TextStyle::Small10Bold, theme::TEXT_TERTIARY);

    let mut py = p_card.y + 48.0;
    if state.dev_ports.is_empty() {
        ctx.draw_text("Ningún puerto local detectado (3000, 5173, 8000, 11434).", Rect::new(p_card.x + 18.0, py, right_w - 36.0, 36.0), TextStyle::Body11Reg, theme::TEXT_TERTIARY);
        py += 48.0;
    } else {
        for (i, port) in state.dev_ports.iter().enumerate() {
            let pr = Rect::new(p_card.x + 14.0, py, right_w - 28.0, 46.0);
            ctx.fill_card(pr, 13.0, theme::ELEVATED, theme::BORDER);
            ctx.fill_circle(pr.x + 16.0, pr.y + 23.0, 4.5, theme::EMERALD);
            ctx.draw_text(&format!("localhost:{}", port.port), Rect::new(pr.x + 28.0, pr.y + 6.0, pr.w - 90.0, 18.0), TextStyle::Mono12Bold, theme::TEXT_PRIMARY);
            ctx.draw_text(&port.label, Rect::new(pr.x + 28.0, pr.y + 24.0, pr.w - 90.0, 15.0), TextStyle::Small9Reg, theme::TEXT_TERTIARY);
            hits.push(HitRegion { id: format!("dev_port_{}", i), rect: pr, action: Action::LaunchTarget(port.url.clone()), right_action: None, tooltip: None, slider: None });
            py += 54.0;
        }
    }

    py += 12.0;
    ctx.draw_text("ACCESOS DEV", Rect::new(p_card.x + 18.0, py, right_w - 36.0, 18.0), TextStyle::Small10Bold, theme::TEXT_TERTIARY);
    py += 26.0;
    let dev_acts = [
        ("dev_a_wt", "Abrir Windows Terminal", Action::LaunchTarget("wt.exe".into())),
        ("dev_a_proj", "Abrir carpeta AuraOS-Rust", Action::LaunchAuraFiles(Some(r"C:\Users\danil\.gemini\antigravity\scratch\AuraOS-Rust".into()))),
        ("dev_a_code", "Abrir PowerShell", Action::LaunchTarget("powershell.exe".into())),
    ];
    for (id, lbl, act) in dev_acts {
        let ar = Rect::new(p_card.x + 14.0, py, right_w - 28.0, 40.0);
        let hov = state.hovered_id.as_deref() == Some(id);
        ctx.fill_rrect(ar, 12.0, if hov { theme::ELEVATED_HOVER } else { theme::ELEVATED });
        ctx.draw_text(lbl, Rect::new(ar.x + 14.0, ar.y, ar.w - 28.0, 40.0), TextStyle::Body11Med, theme::TEXT_PRIMARY);
        hits.push(HitRegion { id: id.into(), rect: ar, action: act, right_action: None, tooltip: None, slider: None });
        py += 48.0;
    }
}

// ==================== 3. GAMER CONSOLE (+ MANUAL ADD GAME + GAME BOOST!) ====================

fn render_gamer_bento(ctx: &mut DrawCtx<'_>, state: &AppState, area: Rect, hits: &mut Vec<HitRegion>) {
    let y0 = area.y + 14.0;

    // Top Hero Strip: Game Boost + "+ Agregar juego" Button
    let strip = Rect::new(area.x, y0, area.w, 68.0);
    ctx.fill_card(
        strip,
        20.0,
        if state.game_boost_active { theme::VIOLET_TINT } else { theme::CARD_BG },
        if state.game_boost_active { theme::VIOLET } else { theme::BORDER },
    );

    let g_ic = Rect::new(strip.x + 16.0, strip.y + 15.0, 38.0, 38.0);
    ctx.fill_rrect(g_ic, 12.0, theme::VIOLET);
    ctx.draw_vector_icon(VectorIcon::Gamepad, g_ic.x + 19.0, g_ic.y + 19.0, 18.0, theme::TEXT_PRIMARY);
    ctx.draw_text(&format!("Biblioteca Gamer ({} juegos)", state.games.len()), Rect::new(strip.x + 66.0, strip.y + 14.0, 360.0, 20.0), TextStyle::Title14Semi, theme::TEXT_PRIMARY);
    ctx.draw_text(&state.game_boost_status, Rect::new(strip.x + 66.0, strip.y + 36.0, 460.0, 16.0), TextStyle::Body11Reg, theme::TEXT_SECONDARY);

    // "+ Agregar juego" button
    let add_g = Rect::new(strip.x + strip.w - 306.0, strip.y + 17.0, 138.0, 34.0);
    ctx.fill_card(add_g, 12.0, theme::ELEVATED, theme::VIOLET_BORDER);
    ctx.draw_vector_icon(VectorIcon::Plus, add_g.x + 16.0, add_g.y + 17.0, 11.5, theme::VIOLET_LIGHT);
    ctx.draw_text("Agregar juego", Rect::new(add_g.x + 29.0, add_g.y, 102.0, 34.0), TextStyle::Body11Med, theme::TEXT_PRIMARY);
    hits.push(HitRegion { id: "gamer_add".into(), rect: add_g, action: Action::AddCustomGameFile, right_action: None, tooltip: Some("Elegir ejecutable .exe o acceso directo .lnk".into()), slider: None });

    // Game Boost Toggle Button
    let boost_r = Rect::new(strip.x + strip.w - 156.0, strip.y + 17.0, 140.0, 34.0);
    ctx.fill_rrect(boost_r, 12.0, if state.game_boost_active { theme::EMERALD } else { theme::VIOLET });
    ctx.draw_text_center(
        if state.game_boost_active { "Game Boost: ON" } else { "Activar Boost" },
        boost_r,
        TextStyle::Body11Med,
        theme::TEXT_PRIMARY,
    );
    hits.push(HitRegion { id: "gamer_boost".into(), rect: boost_r, action: Action::ToggleGameBoost, right_action: None, tooltip: None, slider: None });

    // Game Cards Grid (5 columns x 2+ rows, clipped cleanly)
    let gy = y0 + 84.0;
    let gh = (area.h - 102.0).max(320.0);
    let cols = 5usize;
    let gap = 14.0;
    let cw = (area.w - gap * (cols as f32 - 1.0)) / cols as f32;
    let ch = 188.0;

    for (idx, g) in state.games.iter().take(15).enumerate() {
        let c = (idx % cols) as f32;
        let r = (idx / cols) as f32;
        let cr = Rect::new(area.x + c * (cw + gap), gy + r * (ch + gap), cw, ch);
        if cr.y + cr.h > gy + gh + 20.0 {
            break;
        }
        let id = format!("game_card_{}", idx);
        let hov = state.hovered_id.as_deref() == Some(&id);
        ctx.fill_card(cr, 18.0, if hov { theme::ELEVATED_HOVER } else { theme::CARD_BG }, if hov { theme::VIOLET } else { theme::BORDER });

        // Top art / icon area
        let art_r = Rect::new(cr.x + 8.0, cr.y + 8.0, cr.w - 16.0, 114.0);
        ctx.fill_rrect(art_r, 12.0, theme::ELEVATED);
        let mut drew_art = false;
        if let Some(cov) = &g.cover_path {
            ctx.push_clip(art_r);
            drew_art = ctx.draw_image_file(cov, art_r, 1.0);
            ctx.pop_clip();
        }
        if !drew_art {
            let ic_r = Rect::new(art_r.x + (art_r.w - 48.0) * 0.5, art_r.y + 33.0, 48.0, 48.0);
            if g.icon_path.is_empty() || !ctx.draw_app_icon(&g.icon_path, ic_r) {
                ctx.draw_vector_icon(VectorIcon::Gamepad, art_r.x + art_r.w * 0.5, art_r.y + 57.0, 28.0, theme::VIOLET_LIGHT);
            }
        }

        // Platform badge
        let plat_r = Rect::new(art_r.x + 8.0, art_r.y + 8.0, 56.0, 19.0);
        ctx.fill_rrect(plat_r, 7.0, theme::rgba(13, 13, 18, 0.78));
        ctx.draw_text_center(&g.platform, plat_r, TextStyle::Small9Reg, theme::TEXT_PRIMARY);

        // Title + Play hint
        ctx.draw_text(&g.name, Rect::new(cr.x + 12.0, cr.y + 130.0, cr.w - 24.0, 20.0), TextStyle::Body12Semi, theme::TEXT_PRIMARY);
        ctx.draw_text("Clic para jugar", Rect::new(cr.x + 12.0, cr.y + 154.0, cr.w - 24.0, 16.0), TextStyle::Small9Reg, if hov { theme::VIOLET_LIGHT } else { theme::TEXT_TERTIARY });

        hits.push(HitRegion {
            id,
            rect: cr,
            action: Action::LaunchTarget(g.launch_target.clone()),
            right_action: g.custom_id.as_ref().map(|cid| Action::RemoveCustomGame(cid.clone())),
            tooltip: None,
            slider: None,
        });

        if let Some(cid) = &g.custom_id {
            let del_r = Rect::new(art_r.x + art_r.w - 24.0, art_r.y + 6.0, 18.0, 18.0);
            ctx.fill_rrect(del_r, 9.0, theme::CORAL_BG);
            ctx.draw_vector_icon(VectorIcon::Close, del_r.x + 9.0, del_r.y + 9.0, 8.0, theme::CORAL);
            hits.push(HitRegion {
                id: format!("game_del_{}", idx),
                rect: del_r,
                action: Action::RemoveCustomGame(cid.clone()),
                right_action: None,
                tooltip: Some("Quitar juego manual".into()),
                slider: None,
            });
        }
    }
}

// ==================== 4. FONDOS & WIDGETS STUDIO (WALLPAPER ENGINE + MODULAR WIDGETS!) ====================

fn render_studio_bento(ctx: &mut DrawCtx<'_>, state: &AppState, area: Rect, hits: &mut Vec<HitRegion>) {
    let gap = 16.0;
    let left_w = 344.0f32.min(area.w * 0.34);
    let right_w = (area.w - left_w - gap).max(460.0);
    let y0 = area.y + 10.0;
    let panel_h = (area.h - 24.0).clamp(480.0, 620.0);

    // --- LEFT PANEL: MODULAR DRAGGABLE DESKTOP WIDGETS ---
    let wg_card = Rect::new(area.x, y0, left_w, panel_h);
    ctx.fill_card(wg_card, 22.0, theme::CARD_BG, theme::BORDER);
    ctx.draw_text("WIDGETS DEL ESCRITORIO", Rect::new(wg_card.x + 18.0, wg_card.y + 16.0, left_w - 36.0, 16.0), TextStyle::Small10Bold, theme::VIOLET_LIGHT);
    ctx.draw_text("Arrastra cualquiera en tu escritorio · Ajusta tamaño", Rect::new(wg_card.x + 18.0, wg_card.y + 34.0, left_w - 36.0, 14.0), TextStyle::Small9Reg, theme::TEXT_TERTIARY);

    let specs = [
        (WidgetKind::Music, "Reproductor Musical", "Onda animada 60 FPS + SMTC", VectorIcon::MusicNote, theme::VIOLET),
        (WidgetKind::Clock, "Reloj y Fecha", "Tipografía ligera + Clima", VectorIcon::Sun, theme::BLUE),
        (WidgetKind::Weather, "Clima en Vivo", "Tarjeta violeta con humedad", VectorIcon::Cloud, theme::AMBER),
        (WidgetKind::Monitor, "Monitor PC", "Gráfica CPU + Barra RAM", VectorIcon::Code, theme::EMERALD),
    ];

    let mut wy = wg_card.y + 62.0;
    for (idx, (kind, title, sub, icon, accent)) in specs.into_iter().enumerate() {
        let cfg = state.widgets.get(kind);
        let enabled = cfg.map(|c| c.enabled).unwrap_or(false);
        let tier = cfg.map(|c| c.size_tier).unwrap_or(1);
        let row_h = if enabled { 90.0 } else { 60.0 };
        let rr = Rect::new(wg_card.x + 14.0, wy, left_w - 28.0, row_h);
        ctx.fill_card(
            rr,
            15.0,
            if enabled { theme::ELEVATED } else { theme::SURFACE },
            if enabled { theme::VIOLET_BORDER } else { theme::BORDER },
        );

        let ib = Rect::new(rr.x + 12.0, rr.y + 14.0, 32.0, 32.0);
        ctx.fill_rrect(ib, 10.0, if enabled { accent } else { theme::CARD_BG });
        ctx.draw_vector_icon(icon, ib.x + 16.0, ib.y + 16.0, 14.0, if enabled { theme::TEXT_PRIMARY } else { theme::TEXT_SECONDARY });

        ctx.draw_text(title, Rect::new(rr.x + 52.0, rr.y + 12.0, rr.w - 110.0, 18.0), TextStyle::Body12Semi, theme::TEXT_PRIMARY);
        ctx.draw_text(sub, Rect::new(rr.x + 52.0, rr.y + 31.0, rr.w - 110.0, 15.0), TextStyle::Small9Reg, theme::TEXT_TERTIARY);

        // Toggle switch pill
        let sw = Rect::new(rr.x + rr.w - 50.0, rr.y + 19.0, 38.0, 22.0);
        ctx.fill_rrect(sw, 11.0, if enabled { theme::VIOLET } else { theme::CARD_BG });
        ctx.fill_circle(if enabled { sw.x + 27.0 } else { sw.x + 11.0 }, sw.y + 11.0, 7.5, theme::CRISP_WHITE);
        hits.push(HitRegion {
            id: format!("st_wg_tog_{}", idx),
            rect: Rect::new(rr.x, rr.y, rr.w, 56.0),
            action: Action::ToggleWidget(kind),
            right_action: None,
            tooltip: None,
            slider: None,
        });

        // Inline Size Selector (S / M / L) when active!
        if enabled {
            ctx.draw_text("Tamaño:", Rect::new(rr.x + 14.0, rr.y + 58.0, 56.0, 22.0), TextStyle::Small9Reg, theme::TEXT_TERTIARY);
            let tiers = [("Compacto", 0u8), ("Normal", 1u8), ("Grande", 2u8)];
            let tw = (rr.w - 86.0) / 3.0;
            for (ti, (tlbl, tv)) in tiers.into_iter().enumerate() {
                let tr = Rect::new(rr.x + 70.0 + ti as f32 * (tw + 4.0), rr.y + 57.0, tw, 23.0);
                let sel = tier == tv;
                ctx.fill_rrect(tr, 8.0, if sel { theme::VIOLET } else { theme::CARD_BG });
                ctx.draw_text_center(tlbl, tr, TextStyle::Small9Reg, if sel { theme::TEXT_PRIMARY } else { theme::TEXT_SECONDARY });
                let delta = tv as i8 - tier as i8;
                if delta != 0 {
                    hits.push(HitRegion {
                        id: format!("st_wg_sz_{}_{}", idx, tv),
                        rect: tr,
                        action: Action::ResizeWidget(kind, delta),
                        right_action: None,
                        tooltip: None,
                        slider: None,
                    });
                }
            }
        }
        wy += row_h + 10.0;
    }

    // Smart Grid Arrange + Reset positions buttons at bottom of Widgets column
    let grid_r = Rect::new(wg_card.x + 14.0, wg_card.y + wg_card.h - 84.0, left_w - 28.0, 32.0);
    let grid_hov = state.hovered_id.as_deref() == Some("st_wg_grid");
    ctx.fill_card(grid_r, 11.0, if grid_hov { theme::VIOLET_LIGHT } else { theme::VIOLET }, theme::VIOLET_BORDER);
    ctx.draw_text_center("Ordenar widgets en cuadrícula", grid_r, TextStyle::Body11Med, theme::TEXT_PRIMARY);
    hits.push(HitRegion {
        id: "st_wg_grid".into(),
        rect: grid_r,
        action: Action::ArrangeWidgetsGrid,
        right_action: None,
        tooltip: Some("Alinea todos los widgets activos en cuadrícula uniforme".into()),
        slider: None,
    });

    let rst_r = Rect::new(wg_card.x + 14.0, wg_card.y + wg_card.h - 46.0, left_w - 28.0, 32.0);
    let rst_hov = state.hovered_id.as_deref() == Some("st_wg_reset");
    ctx.fill_rrect(rst_r, 11.0, if rst_hov { theme::ELEVATED_HOVER } else { theme::ELEVATED });
    ctx.draw_text_center("Restablecer posiciones por defecto", rst_r, TextStyle::Body11Med, theme::TEXT_SECONDARY);
    hits.push(HitRegion {
        id: "st_wg_reset".into(),
        rect: rst_r,
        action: Action::ResetWidgetsLayout,
        right_action: None,
        tooltip: None,
        slider: None,
    });

    // --- RIGHT PANEL: INTEGRATED WALLPAPER ENGINE (PRESETS + WORKSHOP VIDEO & WEB + LOCAL) ---
    let wp_card = Rect::new(area.x + left_w + gap, y0, right_w, panel_h);
    ctx.fill_card(wp_card, 22.0, theme::CARD_BG, theme::BORDER);

    let vid_cnt = state.wallpapers.iter().filter(|w| w.kind == WallpaperKind::Video).count();
    let web_cnt = state.wallpapers.iter().filter(|w| w.kind == WallpaperKind::Web).count();
    let hdr_sub = format!("4 predefinidos AuraOS · {} video + {} web de Steam Workshop · Auto-pausa 0% GPU", vid_cnt, web_cnt);
    ctx.draw_text("MOTOR DE FONDOS AURAOS (COMPATIBLE WALLPAPER ENGINE)", Rect::new(wp_card.x + 18.0, wp_card.y + 15.0, right_w - 220.0, 16.0), TextStyle::Small10Bold, theme::VIOLET_LIGHT);
    ctx.draw_text(&hdr_sub, Rect::new(wp_card.x + 18.0, wp_card.y + 33.0, right_w - 220.0, 14.0), TextStyle::Small9Reg, theme::TEXT_TERTIARY);

    // Top-right action pills: [+ Video / Imagen] + optional [Cerrar Wallpaper Engine]
    let add_wp_r = Rect::new(wp_card.x + right_w - 156.0, wp_card.y + 16.0, 140.0, 30.0);
    ctx.fill_rrect(add_wp_r, 11.0, theme::VIOLET);
    ctx.draw_text_center("+ Video / Imagen", add_wp_r, TextStyle::Body11Med, theme::TEXT_PRIMARY);
    hits.push(HitRegion {
        id: "st_wp_add".into(),
        rect: add_wp_r,
        action: Action::AddCustomWallpaperFile,
        right_action: None,
        tooltip: Some("Elegir archivo .mp4, .webm, .jpg o .png local".into()),
        slider: None,
    });

    let has_ext_we = state.tray_apps.iter().any(|a| a.process_key.eq_ignore_ascii_case("wallpaper64"));
    if has_ext_we {
        let kill_we_r = Rect::new(wp_card.x + right_w - 328.0, wp_card.y + 16.0, 164.0, 30.0);
        ctx.fill_card(kill_we_r, 11.0, theme::CORAL_BG, theme::rgba(242, 109, 125, 0.45));
        ctx.draw_text_center("Ahorrar RAM (cerrar WE)", kill_we_r, TextStyle::Small10Med, theme::CORAL);
        hits.push(HitRegion {
            id: "st_kill_we".into(),
            rect: kill_we_r,
            action: Action::CloseExternalWallpaperEngine,
            right_action: None,
            tooltip: Some("Cierra wallpaper64.exe (~75 MB RAM) para usar solo el motor ligero AuraOS".into()),
            slider: None,
        });
    }

    // Scrollable 3-column Wallpaper Visual Gallery
    let grid_r = Rect::new(wp_card.x + 14.0, wp_card.y + 58.0, right_w - 28.0, panel_h - 72.0);
    ctx.push_clip(grid_r);

    let cols = 3usize;
    let cg = 12.0;
    let cw = (grid_r.w - cg * (cols as f32 - 1.0) - 8.0) / cols as f32;
    let ch = 152.0f32;
    let rows = (state.wallpapers.len() + cols - 1) / cols;
    let total_h = (rows as f32 * (ch + cg)).max(grid_r.h);
    let scroll = state.wallpaper_scroll.clamp(0.0, (total_h - grid_r.h).max(0.0));

    for (idx, wp) in state.wallpapers.iter().enumerate() {
        let c = (idx % cols) as f32;
        let r = (idx / cols) as f32;
        let wx = grid_r.x + c * (cw + cg);
        let wy_item = grid_r.y + r * (ch + cg) - scroll;
        if wy_item + ch < grid_r.y || wy_item > grid_r.y + grid_r.h {
            continue;
        }
        let cr = Rect::new(wx, wy_item, cw, ch);
        let id = format!("st_wp_{}", idx);
        let active = state.settings.wallpaper_id == wp.id;
        let hov = state.hovered_id.as_deref() == Some(&id);

        ctx.fill_card(
            cr,
            15.0,
            if active { theme::VIOLET_TINT } else if hov { theme::ELEVATED_HOVER } else { theme::ELEVATED },
            if active { theme::VIOLET } else if hov { theme::VIOLET_BORDER } else { theme::BORDER },
        );

        // Preview Art Area
        let art = Rect::new(cr.x + 6.0, cr.y + 6.0, cr.w - 12.0, 92.0);
        let _art_clip = ctx.push_rrect_mask_layer(art, art, 10.0, 1.0);
        let mut drew_img = false;
        if let Some(prev) = &wp.preview_path {
            drew_img = ctx.draw_image_file(prev, art, 1.0);
        }
        if !drew_img {
            if wp.kind == WallpaperKind::Preset {
                crate::wallpaper::draw_wallpaper_preview_in_rect(ctx, &wp.file_path, art);
            } else {
                let (ar, ag, ab) = wp.accent_rgb;
                ctx.fill_gradient_rrect(art, 10.0, theme::rgb(ar / 2, ag / 2, ab / 2), theme::CARD_BG);
                ctx.draw_vector_icon(
                    if wp.kind == WallpaperKind::External { VectorIcon::Refresh } else { VectorIcon::Sparkle },
                    art.x + art.w * 0.5,
                    art.y + art.h * 0.5,
                    22.0,
                    theme::TEXT_SECONDARY,
                );
            }
        }
        if _art_clip.is_some() {
            ctx.pop_layer();
        }

        // Badge (Tipo + Activo)
        let kind_lbl = match wp.kind {
            WallpaperKind::External => "Windows",
            WallpaperKind::Preset => "AuraOS",
            WallpaperKind::Video => "Video",
            WallpaperKind::Web => "Web",
            WallpaperKind::Image => "Imagen",
        };
        let badge = Rect::new(art.x + 6.0, art.y + 6.0, 52.0, 18.0);
        ctx.fill_rrect(badge, 6.0, theme::rgba(10, 10, 16, 0.78));
        ctx.draw_text_center(kind_lbl, badge, TextStyle::Small9Reg, theme::TEXT_PRIMARY);

        if active {
            let act_b = Rect::new(art.x + art.w - 52.0, art.y + 6.0, 46.0, 18.0);
            ctx.fill_rrect(act_b, 6.0, theme::VIOLET);
            ctx.draw_text_center("Activo", act_b, TextStyle::Small9Reg, theme::TEXT_PRIMARY);
        }

        ctx.draw_text(&wp.title, Rect::new(cr.x + 10.0, cr.y + 104.0, cr.w - 20.0, 18.0), TextStyle::Body11Med, theme::TEXT_PRIMARY);
        ctx.draw_text(&wp.subtitle, Rect::new(cr.x + 10.0, cr.y + 123.0, cr.w - 20.0, 15.0), TextStyle::Small9Reg, theme::TEXT_TERTIARY);

        hits.push(HitRegion {
            id,
            rect: cr,
            action: Action::SelectWallpaper(wp.id.clone()),
            right_action: None,
            tooltip: None,
            slider: None,
        });
    }
    ctx.pop_clip();

    // Scroll indicator when Workshop has many wallpapers
    if total_h > grid_r.h {
        let th = (grid_r.h / total_h * grid_r.h).clamp(32.0, grid_r.h);
        let ty = grid_r.y + (scroll / (total_h - grid_r.h).max(1.0)) * (grid_r.h - th);
        ctx.fill_rrect(Rect::new(wp_card.x + right_w - 11.0, ty, 5.0, th), 2.5, theme::rgb(64, 64, 86));
    }
}

// ==================== 5. INSTANT SEARCH RESULTS (RAYCAST STYLE) ====================

fn render_search_results(ctx: &mut DrawCtx<'_>, state: &AppState, area: Rect, hits: &mut Vec<HitRegion>) {
    let pw = 640.0f32.min(area.w);
    let px = area.x + (area.w - pw) * 0.5;
    let card = Rect::new(px, area.y + 12.0, pw, 456.0);
    ctx.fill_card(card, 20.0, theme::SURFACE, theme::BORDER_STRONG);

    let raw_q = state.search_query.trim();
    let q = raw_q.to_lowercase();
    let mut matches: Vec<(String, String, String, Action)> = Vec::new();

    // 1. Instant Spotlight Math & Unit Calculator (`245 * 18`, `15% de 480`, `100 c to f`, `128 gb en mb`)
    if let Some(calc) = crate::math_eval::evaluate_query(raw_q) {
        matches.push((
            format!("= {}", calc.formatted),
            calc.subtitle,
            String::new(),
            Action::CopyTextAndClose(calc.formatted),
        ));
    }

    // 2. Quick Shell Command Runner (`> ipconfig`, `> notepad` — replaces `Win + R`!)
    if let Some(cmd_body) = raw_q.strip_prefix('>').map(|s| s.trim()).filter(|s| !s.is_empty()) {
        matches.push((
            format!("Ejecutar comando: {}", cmd_body),
            "Ejecutor rápido de AuraOS (reemplaza Win+R)".into(),
            String::new(),
            Action::RunShellCommand(cmd_body.to_string()),
        ));
    }

    // 3. `@audit` / `@agent` AI Desktop Self-Test Trigger
    if q.starts_with("@audit") || q.starts_with("@agent") || q.starts_with("@test") {
        matches.push((
            "Ejecutar Auditoría IA con Cursor Autónomo (AuraAgent)".into(),
            "Prueba en vivo todos los módulos sin mover tu ratón físico".into(),
            String::new(),
            Action::LaunchAuraAgentAudit,
        ));
    }

    for w in &state.open_windows {
        if w.title.to_lowercase().contains(&q) || w.process_name.to_lowercase().contains(&q) {
            matches.push((w.title.clone(), "Ventana abierta".into(), w.exe_path.clone(), Action::FocusWindow(w.hwnd)));
        }
    }
    for g in &state.games {
        if g.name.to_lowercase().contains(&q) {
            matches.push((g.name.clone(), format!("Juego · {}", g.platform), g.icon_path.clone(), Action::LaunchTarget(g.launch_target.clone())));
        }
    }
    for a in &state.dev_ai_apps {
        if a.name.to_lowercase().contains(&q) {
            matches.push((a.name.clone(), a.category.clone(), a.icon_path.clone(), Action::LaunchTarget(a.target.clone())));
        }
    }
    for a in &state.all_apps {
        if a.name.to_lowercase().contains(&q) && !matches.iter().any(|m| m.0.eq_ignore_ascii_case(&a.name)) {
            matches.push((a.name.clone(), a.category.clone(), a.icon_path.clone(), Action::LaunchTarget(a.target.clone())));
        }
    }

    if matches.is_empty() {
        ctx.draw_text_center("Sin resultados · Presiona Esc para limpiar", card, TextStyle::Body12Med, theme::TEXT_TERTIARY);
        return;
    }

    let mut y = card.y + 14.0;
    for (i, (title, sub, ic_path, act)) in matches.into_iter().take(8).enumerate() {
        let rr = Rect::new(card.x + 12.0, y, card.w - 24.0, 46.0);
        let sel = i == state.search_selected || state.hovered_id.as_deref() == Some(&format!("srch_{}", i));
        ctx.fill_rrect(rr, 12.0, if sel { theme::VIOLET } else { theme::rgba(0, 0, 0, 0.0) });

        let ir = Rect::new(rr.x + 12.0, rr.y + 11.0, 24.0, 24.0);
        if ic_path.is_empty() || !ctx.draw_app_icon(&ic_path, ir) {
            ctx.draw_vector_icon(VectorIcon::Sparkle, ir.x + 12.0, ir.y + 12.0, 14.0, theme::TEXT_PRIMARY);
        }
        ctx.draw_text(&title, Rect::new(rr.x + 48.0, rr.y + 5.0, rr.w - 150.0, 19.0), TextStyle::Body12Semi, theme::TEXT_PRIMARY);
        ctx.draw_text(&sub, Rect::new(rr.x + 48.0, rr.y + 24.0, rr.w - 150.0, 15.0), TextStyle::Small9Reg, if sel { theme::rgba(240, 235, 255, 0.85) } else { theme::TEXT_TERTIARY });
        if sel {
            ctx.draw_text_right("Enter", Rect::new(rr.x + rr.w - 64.0, rr.y, 50.0, 46.0), TextStyle::Small10Bold, theme::TEXT_PRIMARY);
        }
        hits.push(HitRegion { id: format!("srch_{}", i), rect: rr, action: act, right_action: None, tooltip: None, slider: None });
        y += 50.0;
    }
}

// ==================== 5. INLINE SETTINGS VIEW ====================

fn render_settings_view(ctx: &mut DrawCtx<'_>, state: &AppState, area: Rect, hits: &mut Vec<HitRegion>) {
    let pw = 590.0f32.min(area.w);
    let px = area.x + (area.w - pw) * 0.5;
    let card = Rect::new(px, area.y + 12.0, pw, 514.0);
    ctx.fill_card(card, 22.0, theme::SURFACE, theme::BORDER_STRONG);

    ctx.draw_text("Ajustes de AuraOS v3.1 (Rust)", Rect::new(card.x + 24.0, card.y + 18.0, 320.0, 24.0), TextStyle::Title16Semi, theme::TEXT_PRIMARY);
    ctx.draw_text("100% nativo en Direct2D/DirectComposition · Zero .NET", Rect::new(card.x + 24.0, card.y + 42.0, 380.0, 16.0), TextStyle::Small10Med, theme::TEXT_TERTIARY);

    let toggles = [
        ("hide_taskbar", "Ocultar barra clásica de Windows (Ctrl+Alt+W restaura)", state.settings.hide_windows_taskbar),
        ("style_windows_apps", "Estilo AuraOS en apps de Windows (Edge, bordes violeta, oscuro)", state.settings.style_windows_apps),
        ("dock_auto_hide", "Ocultar Dock automáticamente al abrir/maximizar apps", state.settings.dock_auto_hide),
        ("dock_left", "Dock a la izquierda estilo Ubuntu (OFF = Abajo macOS)", !state.settings.dock_position.eq_ignore_ascii_case("Bottom")),
        ("desktop_clock", "Mostrar widgets interactivos en el escritorio (Música, Reloj)", state.settings.show_desktop_widgets),
        ("wallpaper_auto_pause", "Auto-pausar fondo animado/video al maximizar o jugar (0% GPU)", state.settings.wallpaper_auto_pause),
        ("auto_boost", "Permitir Game Boost en modo Gamer", state.settings.auto_game_boost),
    ];

    let mut y = card.y + 76.0;
    for (key, lbl, on) in toggles {
        let rr = Rect::new(card.x + 20.0, y, card.w - 40.0, 44.0);
        ctx.fill_rrect(rr, 12.0, theme::ELEVATED);
        ctx.draw_text(lbl, Rect::new(rr.x + 14.0, rr.y, rr.w - 78.0, 44.0), TextStyle::Body11Med, theme::TEXT_PRIMARY);
        let sw = Rect::new(rr.x + rr.w - 54.0, rr.y + 11.0, 40.0, 22.0);
        ctx.fill_rrect(sw, 11.0, if on { theme::VIOLET } else { theme::CARD_BG });
        ctx.fill_circle(if on { sw.x + 29.0 } else { sw.x + 11.0 }, sw.y + 11.0, 8.0, theme::CRISP_WHITE);
        hits.push(HitRegion { id: format!("set_{}", key), rect: rr, action: Action::ToggleSetting(key), right_action: None, tooltip: None, slider: None });
        y += 51.0;
    }

    let btn_res = Rect::new(card.x + 20.0, y + 10.0, 196.0, 36.0);
    ctx.fill_rrect(btn_res, 12.0, theme::CORAL_BG);
    ctx.draw_text_center("Restaurar barra de Windows", btn_res, TextStyle::Body11Med, theme::CORAL);
    hits.push(HitRegion { id: "set_restore".into(), rect: btn_res, action: Action::RestoreWindowsAndExit, right_action: None, tooltip: None, slider: None });

    let btn_upd = Rect::new(card.x + card.w - 170.0, y + 10.0, 150.0, 36.0);
    ctx.fill_rrect(btn_upd, 12.0, theme::VIOLET);
    ctx.draw_text_center("Actualizar versión", btn_upd, TextStyle::Body11Med, theme::TEXT_PRIMARY);
    hits.push(HitRegion { id: "set_upd".into(), rect: btn_upd, action: Action::LaunchUpdater, right_action: None, tooltip: None, slider: None });
}

// ==================== 6. INLINE MODAL: + AGREGAR IA / APP WEB ====================

fn render_add_dev_modal(ctx: &mut DrawCtx<'_>, state: &AppState, w: f32, h: f32, hits: &mut Vec<HitRegion>) {
    ctx.fill_rrect(Rect::new(0.0, 0.0, w, h), 0.0, theme::rgba(0, 0, 0, 0.55));
    let mw = 440.0;
    let mh = 278.0;
    let mr = Rect::new((w - mw) * 0.5, (h - mh) * 0.5, mw, mh);
    ctx.fill_card(mr, 22.0, theme::SURFACE, theme::VIOLET_BORDER);

    // Unified macOS Traffic Lights in Modal top-left
    let ml = [
        ("mod_tl_close", mr.x + 22.0, theme::CORAL, "×", Action::CancelModal),
        ("mod_tl_min", mr.x + 38.0, theme::AMBER, "−", Action::CancelModal),
        ("mod_tl_ok", mr.x + 54.0, theme::EMERALD, "+", Action::SubmitAddDevWebModal),
    ];
    let ml_hov = matches!(
        state.hovered_id.as_deref(),
        Some("mod_tl_close") | Some("mod_tl_min") | Some("mod_tl_ok")
    );
    for (id, cx, col, glyph, act) in ml {
        ctx.fill_circle(cx, mr.y + 24.0, if ml_hov { 5.4 } else { 4.8 }, col);
        if ml_hov {
            ctx.draw_text_center(glyph, Rect::new(cx - 5.5, mr.y + 17.5, 11.0, 12.0), TextStyle::Small9Reg, theme::DARK_INK);
        }
        hits.push(HitRegion { id: id.into(), rect: Rect::new(cx - 7.5, mr.y + 16.0, 15.0, 16.0), action: act, right_action: None, tooltip: None, slider: None });
    }

    ctx.draw_text("Agregar IA o Herramienta Web", Rect::new(mr.x + 70.0, mr.y + 15.0, 320.0, 22.0), TextStyle::Title14Semi, theme::TEXT_PRIMARY);
    ctx.draw_text("Se guardará permanentemente en tu apartado Dev & AI", Rect::new(mr.x + 22.0, mr.y + 42.0, 380.0, 16.0), TextStyle::Small10Med, theme::TEXT_TERTIARY);

    // Field 0: Name
    ctx.draw_text("NOMBRE (ej. DeepSeek, v0, Cursor)", Rect::new(mr.x + 22.0, mr.y + 68.0, 320.0, 15.0), TextStyle::Small9Reg, theme::TEXT_TERTIARY);
    let f0 = Rect::new(mr.x + 22.0, mr.y + 86.0, mw - 44.0, 36.0);
    ctx.fill_card(f0, 11.0, theme::CARD_BG, if state.add_modal.active_field == 0 { theme::VIOLET } else { theme::BORDER });
    let n_txt = if state.add_modal.name.is_empty() { "Escribe el nombre..." } else { state.add_modal.name.as_str() };
    ctx.draw_text(n_txt, Rect::new(f0.x + 12.0, f0.y, f0.w - 24.0, 36.0), TextStyle::Body12Med, if state.add_modal.name.is_empty() { theme::TEXT_TERTIARY } else { theme::TEXT_PRIMARY });
    hits.push(HitRegion { id: "mod_f0".into(), rect: f0, action: Action::FocusModalField(0), right_action: None, tooltip: None, slider: None });

    // Field 1: URL
    ctx.draw_text("URL WEB (ej. https://chat.deepseek.com)", Rect::new(mr.x + 22.0, mr.y + 132.0, 320.0, 15.0), TextStyle::Small9Reg, theme::TEXT_TERTIARY);
    let f1 = Rect::new(mr.x + 22.0, mr.y + 150.0, mw - 44.0, 36.0);
    ctx.fill_card(f1, 11.0, theme::CARD_BG, if state.add_modal.active_field == 1 { theme::VIOLET } else { theme::BORDER });
    let u_txt = if state.add_modal.url.is_empty() { "https://..." } else { state.add_modal.url.as_str() };
    ctx.draw_text(u_txt, Rect::new(f1.x + 12.0, f1.y, f1.w - 24.0, 36.0), TextStyle::Body12Med, if state.add_modal.url.is_empty() { theme::TEXT_TERTIARY } else { theme::TEXT_PRIMARY });
    hits.push(HitRegion { id: "mod_f1".into(), rect: f1, action: Action::FocusModalField(1), right_action: None, tooltip: None, slider: None });

    if !state.add_modal.error.is_empty() {
        ctx.draw_text(&state.add_modal.error, Rect::new(mr.x + 22.0, mr.y + 194.0, mw - 44.0, 16.0), TextStyle::Small10Med, theme::CORAL);
    }

    let b_cancel = Rect::new(mr.x + 22.0, mr.y + mh - 52.0, 108.0, 34.0);
    ctx.fill_rrect(b_cancel, 11.0, theme::ELEVATED);
    ctx.draw_text_center("Cancelar", b_cancel, TextStyle::Body11Med, theme::TEXT_SECONDARY);
    hits.push(HitRegion { id: "mod_cancel".into(), rect: b_cancel, action: Action::CancelModal, right_action: None, tooltip: None, slider: None });

    let b_save = Rect::new(mr.x + mw - 142.0, mr.y + mh - 52.0, 120.0, 34.0);
    ctx.fill_rrect(b_save, 11.0, theme::VIOLET);
    ctx.draw_text_center("Guardar IA", b_save, TextStyle::Body11Med, theme::TEXT_PRIMARY);
    hits.push(HitRegion { id: "mod_save".into(), rect: b_save, action: Action::SubmitAddDevWebModal, right_action: None, tooltip: None, slider: None });
}
