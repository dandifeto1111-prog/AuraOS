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

    let p = theme::pal();
    let alpha = state.settings.glass_opacity.clamp(0.50, 0.96) * open_t;
    ctx.fill_rrect(full_r, 0.0, theme::with_alpha(p.canvas, alpha));

    // Zero-layer GPU origin-aware scale + slide (skip matrix override at rest so parent transforms compose cleanly!)
    if open_t < 0.995 {
        let scale = 0.96 + 0.04 * open_t;
        let rise_y = (1.0 - open_t) * -8.0;
        ctx.set_transform_scale_origin(scale, ox, oy, 0.0, rise_y);
    }

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

    // 5-Mode Segmented Pill (Overview / Dev & AI / Gamer / Studio / Fondos)
    let seg_r = Rect::new(margin_x, top_y, 436.0, 38.0);
    ctx.fill_card(seg_r, 19.0, p.surface, p.border);
    let tab_w = (seg_r.w - 8.0) / 5.0;
    let pill_x = seg_r.x + 4.0 + state.overview_tab_pill_x.value.clamp(0.0, 4.0) * tab_w;
    if !state.overview_settings_view {
        ctx.fill_rrect(Rect::new(pill_x, seg_r.y + 4.0, tab_w, 30.0), 15.0, p.accent);
    }
    let tabs = [
        ("ov_tab_pc", "Overview", ShellMode::PC),
        ("ov_tab_dev", "Dev & AI", ShellMode::Dev),
        ("ov_tab_gamer", "Gamer", ShellMode::Gamer),
        ("ov_tab_creative", "Studio", ShellMode::Creative),
        ("ov_tab_studio", "Fondos", ShellMode::Studio),
    ];
    for (i, (id, lbl, m)) in tabs.into_iter().enumerate() {
        let tr = Rect::new(seg_r.x + 4.0 + i as f32 * tab_w, seg_r.y + 4.0, tab_w, 30.0);
        let active = !state.overview_settings_view && state.mode == m;
        ctx.draw_text_center(
            lbl,
            tr,
            if active { TextStyle::Body11Med } else { TextStyle::Body11Reg },
            if active { theme::CRISP_WHITE } else { p.text_secondary },
        );
        hits.push(HitRegion { id: id.into(), rect: tr, action: Action::ClickModeTab(m), right_action: None, tooltip: None, slider: None });
    }

    // Centered Instant Search Pill (Raycast-style — clickable focus + live caret + clear button!)
    let sw = 320.0f32.min(content_w - 664.0).max(196.0);
    let sx = margin_x + 450.0 + ((content_w - 450.0 - 214.0 - sw) * 0.5).max(0.0);
    let sr = Rect::new(sx, top_y, sw, 38.0);
    let sr_hov = state.hovered_id.as_deref() == Some("ov_search");
    ctx.fill_card(
        sr,
        19.0,
        if sr_hov || !state.search_query.is_empty() { p.elevated } else { p.surface },
        if !state.search_query.is_empty() || sr_hov { p.accent_light } else { p.accent_border },
    );
    ctx.draw_vector_icon(
        VectorIcon::Search,
        sr.x + 20.0,
        sr.y + 19.0,
        13.5,
        if !state.search_query.is_empty() || sr_hov { p.accent_light } else { p.text_tertiary },
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
        if state.overview_settings_view { theme::accent() } else { theme::SURFACE },
        theme::BORDER,
    );
    ctx.draw_vector_icon(VectorIcon::Settings, set_r.x + 17.0, set_r.y + 17.0, 14.0, theme::TEXT_PRIMARY);
    hits.push(HitRegion { id: "ov_set".into(), rect: set_r, action: Action::OpenSettings, right_action: None, tooltip: None, slider: None });

    let af_r = Rect::new(rx - 200.0, top_y + 2.0, 102.0, 34.0);
    ctx.fill_rrect(af_r, 12.0, theme::accent());
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
            ShellMode::Creative => render_creative_studio_bento(ctx, state, body_r_anim, hits),
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
    ctx.draw_text("Juegos", Rect::new(sys_card.x + 88.0, sys_card.y + 92.0, 60.0, 16.0), TextStyle::Small10Med, theme::accent_light());

    // Card 1B: AuraFiles shortcut
    let af_card = Rect::new(x0 + half_w + 12.0, y0, half_w, 128.0);
    let af_hov = state.hovered_id.as_deref() == Some("pc_af_card");
    ctx.fill_card(af_card, 20.0, if af_hov { theme::ELEVATED } else { theme::CARD_BG }, theme::BORDER);
    ctx.draw_text("AuraFiles", Rect::new(af_card.x + 16.0, af_card.y + 14.0, 110.0, 20.0), TextStyle::Body12Semi, theme::TEXT_PRIMARY);
    ctx.draw_vector_icon(VectorIcon::External, af_card.x + af_card.w - 22.0, af_card.y + 24.0, 12.0, theme::TEXT_SECONDARY);
    let drv_txt = format!("{} discos listos", state.drives.len());
    ctx.draw_text(&drv_txt, Rect::new(af_card.x + 16.0, af_card.y + 62.0, af_card.w - 28.0, 24.0), TextStyle::Title14Semi, theme::accent_light());
    ctx.draw_text("Instantáneo · Pestañas · IA", Rect::new(af_card.x + 16.0, af_card.y + 92.0, af_card.w - 28.0, 16.0), TextStyle::Small10Med, theme::TEXT_TERTIARY);
    hits.push(HitRegion { id: "pc_af_card".into(), rect: af_card, action: Action::LaunchAuraFiles(None), right_action: None, tooltip: None, slider: None });

    // Card 2: Drool Hatched Opacity Slider
    let op_card = Rect::new(x0, y0 + 142.0, col_w, 86.0);
    ctx.fill_card(op_card, 20.0, theme::CARD_BG, theme::BORDER);
    let tr = Rect::new(op_card.x + 18.0, op_card.y + 18.0, op_card.w - 36.0, 24.0);
    ctx.fill_hatched_track(tr, 12.0);
    let op_frac = ((state.settings.glass_opacity - 0.50) / 0.46).clamp(0.0, 1.0);
    ctx.fill_rrect(Rect::new(tr.x, tr.y, (op_frac * tr.w).max(24.0), 24.0), 12.0, theme::accent());
    let kx = (tr.x + op_frac * (tr.w - 24.0) + 12.0).clamp(tr.x + 12.0, tr.x + tr.w - 12.0);
    ctx.fill_circle(kx, tr.y + 12.0, 11.0, theme::CRISP_WHITE);
    ctx.stroke_circle(kx, tr.y + 12.0, 11.0, theme::accent(), 2.0);
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
    ctx.fill_rrect(ic_s, 9.0, theme::accent());
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
            ctx.fill_rrect(pct_r, 6.0, theme::accent());
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
        let bg = if idx == 0 { theme::accent() } else if hov { theme::ELEVATED_HOVER } else { theme::ELEVATED };
        ctx.fill_rrect(rr, 15.0, bg);
        ctx.fill_circle(rr.x + 16.0, rr.y + 22.0, 4.0, if idx == 0 { theme::CRISP_WHITE } else { theme::accent_light() });
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
        ("pc_sc_dl", "AuraFiles · Descargas", Action::LaunchAuraFiles(Some(format!("{}\\Downloads", user_prof))), theme::accent()),
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
    ctx.fill_gradient_rrect(w_card, 22.0, theme::accent_light(), theme::accent_deep());
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
        theme::accent(),
    );
    ctx.draw_text("Intel Core i3-10100F", Rect::new(c_card.x + 18.0, c_card.y + 120.0, 160.0, 16.0), TextStyle::Small10Med, theme::DARK_MUTED);
    ctx.draw_text_right("3.60 GHz · 4 núcleos", Rect::new(c_card.x + c_card.w - 150.0, c_card.y + 120.0, 132.0, 16.0), TextStyle::Small10Med, theme::accent());

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
    ctx.fill_rrect(st_pill, 10.0, if state.ram_pct > 80 { theme::CORAL } else { theme::accent() });
    ctx.draw_text_center(if state.ram_pct > 80 { "Alta" } else { "Ideal" }, st_pill, TextStyle::Small9Reg, theme::TEXT_PRIMARY);
}

// ==================== 2. DEV & AI HUB (AI QUOTAS + GITHUB/GIT COMMITS + TOOLS!) ====================

fn render_dev_bento(ctx: &mut DrawCtx<'_>, state: &AppState, area: Rect, hits: &mut Vec<HitRegion>) {
    let p = theme::pal();
    let gap = 14.0;
    let left_w = (area.w * 0.58 - gap * 0.5).max(460.0);
    let right_w = area.w - left_w - gap;
    let y0 = area.y + 10.0;
    let total_h = (area.h - 20.0).max(480.0);

    // ---------- LEFT TOP: AI & DEV WORKSPACE TOOLS ----------
    let tools_h = (total_h * 0.44).clamp(210.0, 270.0);
    let ai_card = Rect::new(area.x, y0, left_w, tools_h);
    ctx.fill_card(ai_card, 22.0, p.card_bg, p.border);
    ctx.draw_text("AI & DEV WORKSPACE", Rect::new(ai_card.x + 18.0, ai_card.y + 14.0, 200.0, 18.0), TextStyle::Small10Bold, p.accent_light);
    ctx.draw_text("Tus herramientas IA y de código", Rect::new(ai_card.x + 18.0, ai_card.y + 30.0, 260.0, 15.0), TextStyle::Small9Reg, p.text_tertiary);

    let btn_web = Rect::new(ai_card.x + ai_card.w - 246.0, ai_card.y + 14.0, 120.0, 28.0);
    ctx.fill_rrect(btn_web, 10.0, p.accent);
    ctx.draw_vector_icon(VectorIcon::Plus, btn_web.x + 14.0, btn_web.y + 14.0, 10.5, theme::CRISP_WHITE);
    ctx.draw_text("IA Web", Rect::new(btn_web.x + 26.0, btn_web.y, 88.0, 28.0), TextStyle::Small10Bold, theme::CRISP_WHITE);
    hits.push(HitRegion { id: "dev_add_web".into(), rect: btn_web, action: Action::OpenAddDevWebModal, right_action: None, tooltip: None, slider: None });

    let btn_loc = Rect::new(ai_card.x + ai_card.w - 118.0, ai_card.y + 14.0, 102.0, 28.0);
    ctx.fill_card(btn_loc, 10.0, p.elevated, p.border_strong);
    ctx.draw_vector_icon(VectorIcon::Plus, btn_loc.x + 14.0, btn_loc.y + 14.0, 10.5, p.accent_light);
    ctx.draw_text("App local", Rect::new(btn_loc.x + 25.0, btn_loc.y, 72.0, 28.0), TextStyle::Small10Bold, p.text_primary);
    hits.push(HitRegion { id: "dev_add_local".into(), rect: btn_loc, action: Action::AddDevLocalFile, right_action: None, tooltip: None, slider: None });

    let cols = 3usize;
    let cw = (ai_card.w - 36.0 - 16.0) / cols as f32;
    let ch = 56.0;
    let gx = ai_card.x + 18.0;
    let gy = ai_card.y + 54.0;
    for (idx, item) in state.dev_ai_apps.iter().take(9).enumerate() {
        let c = (idx % cols) as f32;
        let r = (idx / cols) as f32;
        let tr = Rect::new(gx + c * (cw + 8.0), gy + r * (ch + 8.0), cw, ch);
        if tr.y + tr.h > ai_card.y + ai_card.h - 10.0 {
            break;
        }
        let id = format!("dev_item_{}", idx);
        let hov = state.hovered_id.as_deref() == Some(&id);
        ctx.fill_card(tr, 13.0, if hov { p.elevated_hover } else { p.elevated }, if item.is_custom { p.accent_border } else { p.border });
        let ib = Rect::new(tr.x + 10.0, tr.y + 11.0, 34.0, 34.0);
        ctx.fill_rrect(ib, 9.0, if item.is_web { p.accent_tint } else { p.card_bg });
        if item.icon_path.is_empty() || !ctx.draw_app_icon(&item.icon_path, Rect::new(ib.x + 5.0, ib.y + 5.0, 24.0, 24.0)) {
            ctx.draw_vector_icon(if item.is_web { VectorIcon::Sparkle } else { VectorIcon::Code }, ib.x + 17.0, ib.y + 17.0, 14.0, p.accent_light);
        }
        let tw = if item.is_custom { tr.w - 76.0 } else { tr.w - 54.0 };
        ctx.draw_text(&item.name, Rect::new(tr.x + 50.0, tr.y + 10.0, tw, 17.0), TextStyle::Body11Med, p.text_primary);
        ctx.draw_text(&item.category, Rect::new(tr.x + 50.0, tr.y + 28.0, tw, 14.0), TextStyle::Small9Reg, p.text_tertiary);
        hits.push(HitRegion {
            id,
            rect: tr,
            action: Action::LaunchTarget(item.target.clone()),
            right_action: item.custom_id.as_ref().map(|cid| Action::RemoveCustomDevItem(cid.clone())),
            tooltip: None,
            slider: None,
        });
        if let Some(cid) = &item.custom_id {
            let del_r = Rect::new(tr.x + tr.w - 22.0, tr.y + 6.0, 16.0, 16.0);
            ctx.fill_rrect(del_r, 8.0, theme::CORAL_BG);
            ctx.draw_vector_icon(VectorIcon::Close, del_r.x + 8.0, del_r.y + 8.0, 7.5, theme::CORAL);
            hits.push(HitRegion { id: format!("dev_del_{}", idx), rect: del_r, action: Action::RemoveCustomDevItem(cid.clone()), right_action: None, tooltip: Some("Quitar".into()), slider: None });
        }
    }

    // ---------- LEFT BOTTOM: MULTI-USER AI QUOTAS & USAGE MONITOR ----------
    let q_y = y0 + tools_h + gap;
    let q_h = (total_h - tools_h - gap).max(220.0);
    let q_card = Rect::new(area.x, q_y, left_w, q_h);
    ctx.fill_card(q_card, 22.0, p.card_bg, p.border);
    ctx.draw_text("CUOTAS Y ESTADO DE TUS IAs (POR CUENTA DE USUARIO)", Rect::new(q_card.x + 18.0, q_card.y + 14.0, q_card.w - 130.0, 18.0), TextStyle::Small10Bold, p.accent_light);
    ctx.draw_text("Antigravity · Gemini · Claude · Ollama Local / OpenRouter", Rect::new(q_card.x + 18.0, q_card.y + 30.0, q_card.w - 130.0, 15.0), TextStyle::Small9Reg, p.text_tertiary);

    let ref_q = Rect::new(q_card.x + q_card.w - 104.0, q_card.y + 14.0, 88.0, 26.0);
    ctx.fill_card(ref_q, 9.0, p.elevated, p.border_strong);
    ctx.draw_vector_icon(VectorIcon::Refresh, ref_q.x + 14.0, ref_q.y + 13.0, 10.5, p.accent_light);
    ctx.draw_text("Sincronizar", Rect::new(ref_q.x + 24.0, ref_q.y, 60.0, 26.0), TextStyle::Small9Reg, p.text_primary);
    hits.push(HitRegion { id: "dev_refresh".into(), rect: ref_q, action: Action::RefreshDevHub, right_action: None, tooltip: Some("Refrescar Ollama, APIs y GitHub".into()), slider: None });

    let providers = if state.dev_hub.providers.is_empty() {
        vec![
            crate::dev_hub::AiProviderStatus { id: "antigravity".into(), name: "Antigravity".into(), subtitle: "Tu sesión · Cuota local".into(), pct_remaining: state.dev_hub_cfg.antigravity_pct, badge: "CUENTA".into(), live_connected: true, dashboard_url: "https://aistudio.google.com/".into(), adjustable: true },
            crate::dev_hub::AiProviderStatus { id: "gemini".into(), name: "Gemini".into(), subtitle: "Google AI Studio · Tu cuenta".into(), pct_remaining: state.dev_hub_cfg.gemini_pct, badge: "WEB".into(), live_connected: false, dashboard_url: "https://aistudio.google.com/apikey".into(), adjustable: true },
            crate::dev_hub::AiProviderStatus { id: "claude".into(), name: "Claude".into(), subtitle: "Anthropic · Tu cuenta".into(), pct_remaining: state.dev_hub_cfg.claude_pct, badge: "WEB".into(), live_connected: false, dashboard_url: "https://claude.ai/settings/usage".into(), adjustable: true },
            crate::dev_hub::AiProviderStatus { id: "ollama".into(), name: "Ollama Local".into(), subtitle: "Detectando 127.0.0.1:11434...".into(), pct_remaining: 0, badge: "LOCAL".into(), live_connected: false, dashboard_url: "http://127.0.0.1:11434".into(), adjustable: false },
        ]
    } else {
        state.dev_hub.providers.clone()
    };

    let q_cols = 2usize;
    let qcw = (q_card.w - 36.0 - 10.0) * 0.5;
    let qch = ((q_h - 68.0) * 0.5).clamp(72.0, 96.0);
    for (idx, prov) in providers.iter().take(4).enumerate() {
        let c = (idx % q_cols) as f32;
        let r = (idx / q_cols) as f32;
        let qr = Rect::new(q_card.x + 18.0 + c * (qcw + 10.0), q_card.y + 54.0 + r * (qch + 8.0), qcw, qch);
        let hov = state.hovered_id.as_deref() == Some(&format!("dev_aiq_{}", idx));
        ctx.fill_card(qr, 14.0, if hov { p.elevated_hover } else { p.elevated }, if prov.live_connected { p.accent_border } else { p.border });

        ctx.draw_text(&prov.name, Rect::new(qr.x + 12.0, qr.y + 8.0, 118.0, 18.0), TextStyle::Body12Semi, p.text_primary);
        let b_r = Rect::new(qr.x + qr.w - 64.0, qr.y + 8.0, 52.0, 17.0);
        ctx.fill_rrect(b_r, 6.0, if prov.live_connected { p.accent_tint } else { p.card_bg });
        ctx.draw_text_center(&prov.badge, b_r, TextStyle::Small9Reg, if prov.live_connected { p.accent_light } else { p.text_tertiary });
        ctx.draw_text(&prov.subtitle, Rect::new(qr.x + 12.0, qr.y + 26.0, qr.w - 24.0, 14.0), TextStyle::Small9Reg, p.text_tertiary);

        // Material 3 Quota Progress Bar
        let bar_r = Rect::new(qr.x + 12.0, qr.y + 44.0, qr.w - 64.0, 8.0);
        ctx.fill_rrect(bar_r, 4.0, p.card_bg);
        let fill_col = if prov.pct_remaining <= 20 { theme::CORAL } else if prov.pct_remaining <= 45 { theme::AMBER } else { p.accent };
        let fw = ((prov.pct_remaining as f32 / 100.0).clamp(0.0, 1.0) * bar_r.w).max(if prov.pct_remaining > 0 { 6.0 } else { 0.0 });
        if fw > 0.0 {
            ctx.fill_rrect(Rect::new(bar_r.x, bar_r.y, fw, 8.0), 4.0, fill_col);
        }
        ctx.draw_text_right(&format!("{}%", prov.pct_remaining), Rect::new(qr.x + qr.w - 48.0, qr.y + 39.0, 36.0, 17.0), TextStyle::Mono11Med, p.text_primary);

        // Bottom quick controls: [-5%] [+5%] [Reset] + [Panel]
        let by = qr.y + qr.h - 24.0;
        if prov.adjustable {
            let b_dec = Rect::new(qr.x + 12.0, by, 26.0, 18.0);
            let b_inc = Rect::new(qr.x + 42.0, by, 26.0, 18.0);
            let b_rst = Rect::new(qr.x + 72.0, by, 42.0, 18.0);
            ctx.fill_rrect(b_dec, 6.0, p.card_bg);
            ctx.fill_rrect(b_inc, 6.0, p.card_bg);
            ctx.fill_rrect(b_rst, 6.0, p.card_bg);
            ctx.draw_text_center("−5", b_dec, TextStyle::Small9Reg, p.text_secondary);
            ctx.draw_text_center("+5", b_inc, TextStyle::Small9Reg, p.text_secondary);
            ctx.draw_text_center("100%", b_rst, TextStyle::Small9Reg, p.accent_light);
            hits.push(HitRegion { id: format!("q_dec_{}", idx), rect: b_dec, action: Action::AdjustAiQuota(prov.id.clone(), -5), right_action: None, tooltip: Some("Restar 5%".into()), slider: None });
            hits.push(HitRegion { id: format!("q_inc_{}", idx), rect: b_inc, action: Action::AdjustAiQuota(prov.id.clone(), 5), right_action: None, tooltip: Some("Sumar 5%".into()), slider: None });
            hits.push(HitRegion { id: format!("q_rst_{}", idx), rect: b_rst, action: Action::ResetAiQuota(prov.id.clone()), right_action: None, tooltip: Some("Reiniciar ciclo a 100%".into()), slider: None });
        }
        let open_r = Rect::new(qr.x + qr.w - 72.0, by, 60.0, 18.0);
        ctx.fill_rrect(open_r, 6.0, p.accent_tint);
        ctx.draw_text_center("Panel ↗", open_r, TextStyle::Small9Reg, p.accent_light);
        hits.push(HitRegion { id: format!("dev_aiq_{}", idx), rect: open_r, action: Action::LaunchTarget(prov.dashboard_url.clone()), right_action: None, tooltip: Some(format!("Abrir panel oficial de {}", prov.name)), slider: None });
    }

    // ---------- RIGHT TOP: MULTI-USER GITHUB & LOCAL GIT COMMITS MONITOR! ----------
    let right_x = area.x + left_w + gap;
    let git_h = (total_h * 0.66).clamp(310.0, 420.0);
    let g_card = Rect::new(right_x, y0, right_w, git_h);
    ctx.fill_card(g_card, 22.0, p.card_bg, p.border);
    ctx.draw_vector_icon(VectorIcon::GitBranch, g_card.x + 24.0, g_card.y + 22.0, 14.0, p.accent_light);
    ctx.draw_text("GITHUB Y COMMITS GIT", Rect::new(g_card.x + 38.0, g_card.y + 13.0, 180.0, 18.0), TextStyle::Small10Bold, p.accent_light);

    // Editable / Clickable @username pill (auto-detected per user!)
    let gh_label = if state.gh_input_active {
        format!("@{}|", state.gh_input_text)
    } else if !state.dev_hub.github.username.is_empty() {
        format!("@{}", state.dev_hub.github.username)
    } else {
        "Configurar @GitHub".to_string()
    };
    let gh_pill = Rect::new(g_card.x + g_card.w - 156.0, g_card.y + 12.0, 140.0, 24.0);
    ctx.fill_card(gh_pill, 10.0, if state.gh_input_active { p.elevated_hover } else { p.elevated }, if state.gh_input_active { p.accent } else { p.border_strong });
    ctx.draw_text_center(&gh_label, gh_pill, TextStyle::Small10Bold, p.text_primary);
    hits.push(HitRegion {
        id: "dev_gh_user".into(),
        rect: gh_pill,
        action: if !state.gh_input_active && !state.dev_hub.github.profile_url.is_empty() {
            Action::LaunchTarget(state.dev_hub.github.profile_url.clone())
        } else {
            Action::StartGithubUserEdit
        },
        right_action: Some(Action::StartGithubUserEdit),
        tooltip: Some("Clic abrir perfil GitHub · Clic derecho cambiar @usuario".into()),
        slider: None,
    });

    // 7-Day GitHub Commits Mini Activity Bar Chart
    let chart_r = Rect::new(g_card.x + 16.0, g_card.y + 44.0, g_card.w - 32.0, 74.0);
    ctx.fill_card(chart_r, 14.0, p.elevated, p.border);
    let total_c = state.dev_hub.github.total_recent_commits;
    let gh_hdr = if state.dev_hub.github.fetched_ok {
        format!("Actividad 7 días · {} commits recientes", total_c)
    } else {
        "Actividad GitHub (clic der. en @usuario para vincular)".to_string()
    };
    ctx.draw_text(&gh_hdr, Rect::new(chart_r.x + 12.0, chart_r.y + 6.0, chart_r.w - 24.0, 16.0), TextStyle::Small9Reg, p.text_secondary);

    let max_day = state.dev_hub.github.daily_commits_7d.iter().copied().max().unwrap_or(1).max(1) as f32;
    let bw = ((chart_r.w - 24.0 - 6.0 * 8.0) / 7.0).max(14.0);
    let day_lbls = ["D-6", "D-5", "D-4", "D-3", "D-2", "Ayer", "Hoy"];
    for (i, &cnt) in state.dev_hub.github.daily_commits_7d.iter().enumerate() {
        let bx = chart_r.x + 12.0 + i as f32 * (bw + 8.0);
        let frac = (cnt as f32 / max_day).clamp(0.0, 1.0);
        let bh = if cnt > 0 { 8.0 + frac * 24.0 } else { 4.0 };
        let by = chart_r.y + 54.0 - bh;
        ctx.fill_rrect(Rect::new(bx, by, bw, bh), 4.0, if cnt > 0 { p.accent } else { p.card_bg });
        ctx.draw_text_center(day_lbls[i], Rect::new(bx, chart_r.y + 56.0, bw, 14.0), TextStyle::Small9Reg, p.text_tertiary);
    }

    // Local Git Repositories Live Status
    let mut ry = g_card.y + 128.0;
    ctx.draw_text("REPOSITORIOS LOCALES (GIT STATUS)", Rect::new(g_card.x + 18.0, ry, g_card.w - 36.0, 15.0), TextStyle::Small9Reg, p.text_tertiary);
    ry += 20.0;

    if state.dev_hub.repos.is_empty() {
        ctx.draw_text("No se detectaron repositorios .git locales aún.", Rect::new(g_card.x + 18.0, ry, g_card.w - 36.0, 28.0), TextStyle::Body11Reg, p.text_tertiary);
    } else {
        for (i, repo) in state.dev_hub.repos.iter().take(4).enumerate() {
            if ry + 48.0 > g_card.y + g_card.h - 10.0 {
                break;
            }
            let rr = Rect::new(g_card.x + 14.0, ry, g_card.w - 28.0, 48.0);
            let hov = state.hovered_id.as_deref() == Some(&format!("dev_repo_{}", i));
            ctx.fill_card(rr, 12.0, if hov { p.elevated_hover } else { p.elevated }, p.border);
            ctx.draw_text(&repo.name, Rect::new(rr.x + 10.0, rr.y + 5.0, rr.w - 140.0, 18.0), TextStyle::Body11Med, p.text_primary);

            // Branch + Dirty Files Badge
            let br_lbl = format!("{}", repo.branch);
            let br_r = Rect::new(rr.x + rr.w - 130.0, rr.y + 6.0, 66.0, 16.0);
            ctx.fill_rrect(br_r, 6.0, p.accent_tint);
            ctx.draw_text_center(&br_lbl, br_r, TextStyle::Small9Reg, p.accent_light);

            let st_r = Rect::new(rr.x + rr.w - 60.0, rr.y + 6.0, 52.0, 16.0);
            let (st_bg, st_fg, st_txt) = if repo.dirty_files > 0 {
                (theme::rgba(245, 183, 49, 0.22), theme::AMBER, format!("{} mod", repo.dirty_files))
            } else {
                (theme::rgba(34, 197, 94, 0.20), theme::EMERALD, "Limpio".to_string())
            };
            ctx.fill_rrect(st_r, 6.0, st_bg);
            ctx.draw_text_center(&st_txt, st_r, TextStyle::Small9Reg, st_fg);

            let commit_line = format!("{} · {} ({})", repo.last_hash, repo.last_msg, repo.last_when);
            ctx.draw_text(&commit_line, Rect::new(rr.x + 10.0, rr.y + 25.0, rr.w - 20.0, 16.0), TextStyle::Small9Reg, p.text_secondary);
            hits.push(HitRegion {
                id: format!("dev_repo_{}", i),
                rect: rr,
                action: Action::LaunchAuraFiles(Some(repo.path.clone())),
                right_action: None,
                tooltip: Some(format!("Abrir carpeta {}", repo.path)),
                slider: None,
            });
            ry += 54.0;
        }
    }

    // ---------- RIGHT BOTTOM: ACTIVE LOCAL SERVERS + TERMINAL SHORTCUTS ----------
    let p_y = y0 + git_h + gap;
    let p_h = (total_h - git_h - gap).max(140.0);
    let p_card = Rect::new(right_x, p_y, right_w, p_h);
    ctx.fill_card(p_card, 22.0, p.card_bg, p.border);
    ctx.draw_text("SERVIDORES LOCALES Y TERMINAL", Rect::new(p_card.x + 16.0, p_card.y + 12.0, right_w - 32.0, 16.0), TextStyle::Small10Bold, p.text_tertiary);

    let mut py = p_card.y + 34.0;
    if state.dev_ports.is_empty() {
        ctx.draw_text("Puertos 3000 / 5173 / 8000 / 11434 inactivos.", Rect::new(p_card.x + 16.0, py, right_w - 32.0, 22.0), TextStyle::Small9Reg, p.text_tertiary);
        py += 28.0;
    } else {
        for (i, port) in state.dev_ports.iter().take(2).enumerate() {
            let pr = Rect::new(p_card.x + 14.0, py, right_w - 28.0, 30.0);
            ctx.fill_card(pr, 9.0, p.elevated, p.border);
            ctx.fill_circle(pr.x + 12.0, pr.y + 15.0, 4.0, theme::EMERALD);
            ctx.draw_text(&format!("localhost:{} · {}", port.port, port.label), Rect::new(pr.x + 22.0, pr.y, pr.w - 32.0, 30.0), TextStyle::Mono11Med, p.text_primary);
            hits.push(HitRegion { id: format!("dev_port_{}", i), rect: pr, action: Action::LaunchTarget(port.url.clone()), right_action: None, tooltip: None, slider: None });
            py += 34.0;
        }
    }

    let term_y = (p_card.y + p_card.h - 42.0).max(py + 4.0);
    let bw2 = (right_w - 36.0) * 0.5;
    let b_wt = Rect::new(p_card.x + 14.0, term_y, bw2, 30.0);
    let b_ps = Rect::new(b_wt.x + bw2 + 8.0, term_y, bw2, 30.0);
    ctx.fill_rrect(b_wt, 10.0, p.elevated);
    ctx.fill_rrect(b_ps, 10.0, p.elevated);
    ctx.draw_text_center("Windows Terminal", b_wt, TextStyle::Small10Bold, p.text_primary);
    ctx.draw_text_center("PowerShell", b_ps, TextStyle::Small10Bold, p.text_primary);
    hits.push(HitRegion { id: "dev_a_wt".into(), rect: b_wt, action: Action::LaunchTarget("wt.exe".into()), right_action: None, tooltip: None, slider: None });
    hits.push(HitRegion { id: "dev_a_code".into(), rect: b_ps, action: Action::LaunchTarget("powershell.exe".into()), right_action: None, tooltip: None, slider: None });
}

// ==================== 3. GAMER CONSOLE (+ MANUAL ADD GAME + GAME BOOST!) ====================

fn render_gamer_bento(ctx: &mut DrawCtx<'_>, state: &AppState, area: Rect, hits: &mut Vec<HitRegion>) {
    let y0 = area.y + 14.0;

    // Top Hero Strip: Game Boost + "+ Agregar juego" Button
    let strip = Rect::new(area.x, y0, area.w, 68.0);
    ctx.fill_card(
        strip,
        20.0,
        if state.game_boost_active { theme::accent_tint() } else { theme::CARD_BG },
        if state.game_boost_active { theme::accent() } else { theme::BORDER },
    );

    let g_ic = Rect::new(strip.x + 16.0, strip.y + 15.0, 38.0, 38.0);
    ctx.fill_rrect(g_ic, 12.0, theme::accent());
    ctx.draw_vector_icon(VectorIcon::Gamepad, g_ic.x + 19.0, g_ic.y + 19.0, 18.0, theme::TEXT_PRIMARY);
    ctx.draw_text(&format!("Biblioteca Gamer ({} juegos)", state.games.len()), Rect::new(strip.x + 66.0, strip.y + 14.0, 360.0, 20.0), TextStyle::Title14Semi, theme::TEXT_PRIMARY);
    ctx.draw_text(&state.game_boost_status, Rect::new(strip.x + 66.0, strip.y + 36.0, 460.0, 16.0), TextStyle::Body11Reg, theme::TEXT_SECONDARY);

    // "+ Agregar juego" button
    let add_g = Rect::new(strip.x + strip.w - 306.0, strip.y + 17.0, 138.0, 34.0);
    ctx.fill_card(add_g, 12.0, theme::ELEVATED, theme::accent_border());
    ctx.draw_vector_icon(VectorIcon::Plus, add_g.x + 16.0, add_g.y + 17.0, 11.5, theme::accent_light());
    ctx.draw_text("Agregar juego", Rect::new(add_g.x + 29.0, add_g.y, 102.0, 34.0), TextStyle::Body11Med, theme::TEXT_PRIMARY);
    hits.push(HitRegion { id: "gamer_add".into(), rect: add_g, action: Action::AddCustomGameFile, right_action: None, tooltip: Some("Elegir ejecutable .exe o acceso directo .lnk".into()), slider: None });

    // Game Boost Toggle Button
    let boost_r = Rect::new(strip.x + strip.w - 156.0, strip.y + 17.0, 140.0, 34.0);
    ctx.fill_rrect(boost_r, 12.0, if state.game_boost_active { theme::EMERALD } else { theme::accent() });
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
        ctx.fill_card(cr, 18.0, if hov { theme::ELEVATED_HOVER } else { theme::CARD_BG }, if hov { theme::accent() } else { theme::BORDER });

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
                ctx.draw_vector_icon(VectorIcon::Gamepad, art_r.x + art_r.w * 0.5, art_r.y + 57.0, 28.0, theme::accent_light());
            }
        }

        // Platform badge
        let plat_r = Rect::new(art_r.x + 8.0, art_r.y + 8.0, 56.0, 19.0);
        ctx.fill_rrect(plat_r, 7.0, theme::rgba(13, 13, 18, 0.78));
        ctx.draw_text_center(&g.platform, plat_r, TextStyle::Small9Reg, theme::TEXT_PRIMARY);

        // Title + Play hint
        ctx.draw_text(&g.name, Rect::new(cr.x + 12.0, cr.y + 130.0, cr.w - 24.0, 20.0), TextStyle::Body12Semi, theme::TEXT_PRIMARY);
        ctx.draw_text("Clic para jugar", Rect::new(cr.x + 12.0, cr.y + 154.0, cr.w - 24.0, 16.0), TextStyle::Small9Reg, if hov { theme::accent_light() } else { theme::TEXT_TERTIARY });

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

// ==================== 3B. AURA STUDIO CREATIVE SUITE (AURABEATS DAW + PHOTOS + VIDEO) ====================

fn render_creative_studio_bento(ctx: &mut DrawCtx<'_>, state: &AppState, area: Rect, hits: &mut Vec<HitRegion>) {
    let p = theme::pal();
    let gap = 16.0;
    let y0 = area.y + 10.0;

    // --- TOP HEADER BANNER ---
    let hdr = Rect::new(area.x, y0, area.w, 72.0);
    ctx.fill_card(hdr, 22.0, p.card_bg, p.border);
    let badge_ic = Rect::new(hdr.x + 18.0, hdr.y + 16.0, 40.0, 40.0);
    ctx.fill_rrect(badge_ic, 13.0, p.accent_tint);
    ctx.draw_vector_icon(VectorIcon::Sparkle, badge_ic.x + 20.0, badge_ic.y + 20.0, 18.0, p.accent_light);
    ctx.draw_text(
        "AURA STUDIO · SUITE CREATIVA NATIVA",
        Rect::new(hdr.x + 70.0, hdr.y + 16.0, hdr.w - 350.0, 20.0),
        TextStyle::Title14Semi,
        p.text_primary,
    );
    ctx.draw_text(
        "Producción musical con AuraBeats DAW, fotografía GPU y herramientas creativas integradas a AuraOS",
        Rect::new(hdr.x + 70.0, hdr.y + 38.0, hdr.w - 350.0, 18.0),
        TextStyle::Body11Reg,
        p.text_tertiary,
    );

    let status_pill = Rect::new(hdr.x + hdr.w - 256.0, hdr.y + 20.0, 238.0, 32.0);
    let (st_bg, st_bd, st_col, st_txt) = if state.aurabeats_installed {
        (
            theme::rgba(34, 197, 94, 0.14),
            theme::rgba(34, 197, 94, 0.42),
            theme::EMERALD,
            format!("AuraBeats listo · {} plugins DSP", state.aurabeats_plugin_count.max(28))
        )
    } else {
        (
            p.accent_tint,
            p.accent_border,
            p.accent_light,
            "Proceso aislado · 0 MB en reposo".to_string(),
        )
    };
    ctx.fill_card(status_pill, 12.0, st_bg, st_bd);
    ctx.fill_circle(status_pill.x + 14.0, status_pill.y + 16.0, 4.0, st_col);
    ctx.draw_text(
        &st_txt,
        Rect::new(status_pill.x + 24.0, status_pill.y, status_pill.w - 30.0, 32.0),
        TextStyle::Small10Bold,
        st_col,
    );

    // --- MAIN 3-COLUMN CREATIVE BENTO (AURABEATS 44% + AURAPHOTOS 28% + VIDEO SLOT 28%) ---
    let main_y = hdr.y + hdr.h + gap;
    let main_h = (area.h - 72.0 - 102.0 - gap * 2.0 - 16.0).clamp(312.0, 386.0);
    let daw_w = ((area.w - gap * 2.0) * 0.44).max(380.0);
    let side_w = ((area.w - daw_w - gap * 2.0) * 0.5).max(240.0);

    // 1. HERO CARD: AURABEATS DAW (GPL-3.0 Fork de Lemon-Studio integrado a AuraOS)
    let daw_r = Rect::new(area.x, main_y, daw_w, main_h);
    let daw_hov = state.hovered_id.as_deref() == Some("studio_aurabeats");
    ctx.fill_card(
        daw_r,
        24.0,
        if daw_hov { p.elevated } else { p.card_bg },
        if daw_hov { p.accent_light } else { p.accent_border },
    );

    // Subtle Obsidian-Violet header highlight strip inside AuraBeats card
    let daw_top = Rect::new(daw_r.x + 14.0, daw_r.y + 14.0, daw_r.w - 28.0, 108.0);
    ctx.fill_card(daw_top, 18.0, theme::with_alpha(p.accent_tint, 0.72), theme::with_alpha(p.accent_border, 0.55));

    let ico_r = Rect::new(daw_top.x + 16.0, daw_top.y + 18.0, 72.0, 72.0);
    ctx.fill_rrect(ico_r, 18.0, p.surface);
    let mut drew_daw_icon = false;
    if let Some(ic_path) = &state.aurabeats_icon_path {
        ctx.push_clip(ico_r);
        drew_daw_icon = ctx.draw_image_file(ic_path, ico_r, 1.0);
        ctx.pop_clip();
    }
    if !drew_daw_icon {
        ctx.draw_vector_icon(VectorIcon::MusicNote, ico_r.x + 36.0, ico_r.y + 36.0, 30.0, p.accent_light);
    }

    let tag_r = Rect::new(ico_r.x + 88.0, daw_top.y + 18.0, 136.0, 21.0);
    ctx.fill_rrect(tag_r, 8.0, p.accent);
    ctx.draw_text_center("AUDIO · VST3 / CLAP", tag_r, TextStyle::Small9Reg, theme::CRISP_WHITE);

    ctx.draw_text(
        "AuraBeats DAW",
        Rect::new(ico_r.x + 88.0, daw_top.y + 44.0, daw_top.w - 110.0, 28.0),
        TextStyle::Title22Bold,
        p.text_primary,
    );
    ctx.draw_text(
        "Estación de audio digital nativa en Rust · Tema Aura Obsidian",
        Rect::new(ico_r.x + 88.0, daw_top.y + 74.0, daw_top.w - 104.0, 18.0),
        TextStyle::Body11Reg,
        p.text_secondary,
    );

    // Feature description (2 crisp lines — zero ellipsis truncation!) & 2x2 capability chips
    ctx.draw_text(
        "Compón beats, graba MIDI y mezcla con Piano Roll, Channel Rack y Mixer.",
        Rect::new(daw_r.x + 20.0, daw_top.y + daw_top.h + 14.0, daw_r.w - 40.0, 18.0),
        TextStyle::Body11Reg,
        p.text_secondary,
    );
    ctx.draw_text(
        "Incluye 28 plugins DSP nativos, VST3/VST2/CLAP e importación .flp / .lsp.",
        Rect::new(daw_r.x + 20.0, daw_top.y + daw_top.h + 33.0, daw_r.w - 40.0, 18.0),
        TextStyle::Body11Reg,
        p.text_tertiary,
    );

    let chips = [
        ("Piano Roll & Mixer", theme::accent_light()),
        ("28 Plugins DSP nativos", theme::EMERALD),
        ("Puente VST3 / VST2 / CLAP", theme::BLUE),
        ("Proyectos .flp y .lsp", theme::AMBER),
    ];
    let chip_w = (daw_r.w - 50.0) * 0.5;
    let chip_y0 = daw_top.y + daw_top.h + 64.0;
    for (ci, (clbl, ccol)) in chips.into_iter().enumerate() {
        let col = (ci % 2) as f32;
        let row = (ci / 2) as f32;
        let cr = Rect::new(daw_r.x + 20.0 + col * (chip_w + 10.0), chip_y0 + row * 34.0, chip_w, 26.0);
        ctx.fill_card(cr, 9.0, p.surface, p.border);
        ctx.fill_circle(cr.x + 12.0, cr.y + 13.0, 3.5, ccol);
        ctx.draw_text(clbl, Rect::new(cr.x + 22.0, cr.y, cr.w - 28.0, 26.0), TextStyle::Small10Med, p.text_primary);
    }

    // Primary CTA Button for AuraBeats
    let daw_btn = Rect::new(daw_r.x + 20.0, daw_r.y + daw_r.h - 56.0, daw_r.w - 40.0, 38.0);
    ctx.fill_rrect(daw_btn, 13.0, if daw_hov { p.accent_light } else { p.accent });
    ctx.draw_vector_icon(VectorIcon::Play, daw_btn.x + daw_btn.w * 0.5 - 86.0, daw_btn.y + 19.0, 12.0, theme::CRISP_WHITE);
    ctx.draw_text_center(
        if state.aurabeats_installed { "Abrir AuraBeats DAW" } else { "Compilar / Abrir AuraBeats" },
        daw_btn,
        TextStyle::Body12Semi,
        theme::CRISP_WHITE,
    );
    hits.push(HitRegion {
        id: "studio_aurabeats".into(),
        rect: daw_r,
        action: Action::LaunchAuraBeats,
        right_action: None,
        tooltip: Some("Lanzar AuraBeats DAW (motor aislado de audio GPL-3.0)".into()),
        slider: None,
    });

    // 2. CARD: AURAPHOTOS STUDIO (GPU Photo Editor)
    let ph_x = daw_r.x + daw_r.w + gap;
    let ph_r = Rect::new(ph_x, main_y, side_w, main_h);
    let ph_hov = state.hovered_id.as_deref() == Some("studio_auraphotos");
    ctx.fill_card(
        ph_r,
        24.0,
        if ph_hov { p.elevated } else { p.card_bg },
        if ph_hov { theme::EMERALD } else { p.border },
    );

    let ph_ic = Rect::new(ph_r.x + 20.0, ph_r.y + 20.0, 52.0, 52.0);
    ctx.fill_card(ph_ic, 16.0, theme::rgba(34, 197, 94, 0.14), theme::rgba(34, 197, 94, 0.40));
    ctx.draw_vector_icon(VectorIcon::Image, ph_ic.x + 26.0, ph_ic.y + 26.0, 24.0, theme::EMERALD);

    let ph_tag = Rect::new(ph_r.x + ph_r.w - 94.0, ph_r.y + 24.0, 74.0, 21.0);
    ctx.fill_rrect(ph_tag, 8.0, theme::rgba(34, 197, 94, 0.16));
    ctx.draw_text_center("FOTO · GPU", ph_tag, TextStyle::Small9Reg, theme::EMERALD);

    ctx.draw_text("AuraPhotos Studio", Rect::new(ph_r.x + 20.0, ph_r.y + 88.0, ph_r.w - 40.0, 24.0), TextStyle::Title16Semi, p.text_primary);
    ctx.draw_text(
        "Visor y editor fotográfico GPU Direct2D",
        Rect::new(ph_r.x + 20.0, ph_r.y + 118.0, ph_r.w - 40.0, 18.0),
        TextStyle::Body11Reg,
        p.text_secondary,
    );
    ctx.draw_text(
        "con zoom fluido, rotación y exportación.",
        Rect::new(ph_r.x + 20.0, ph_r.y + 137.0, ph_r.w - 40.0, 18.0),
        TextStyle::Body11Reg,
        p.text_tertiary,
    );

    for (pi, plbl) in ["Direct2D 60 FPS sin lag", "PNG · JPG · WEBP · BMP", "Integrado con AuraFiles"].into_iter().enumerate() {
        let pr = Rect::new(ph_r.x + 20.0, ph_r.y + 182.0 + pi as f32 * 30.0, ph_r.w - 40.0, 24.0);
        ctx.fill_card(pr, 8.0, p.surface, p.border);
        ctx.fill_circle(pr.x + 11.0, pr.y + 12.0, 3.0, theme::EMERALD);
        ctx.draw_text(plbl, Rect::new(pr.x + 20.0, pr.y, pr.w - 26.0, 24.0), TextStyle::Small10Med, p.text_secondary);
    }

    let ph_btn = Rect::new(ph_r.x + 20.0, ph_r.y + ph_r.h - 56.0, ph_r.w - 40.0, 38.0);
    ctx.fill_card(ph_btn, 13.0, if ph_hov { theme::EMERALD } else { p.elevated }, theme::rgba(34, 197, 94, 0.45));
    ctx.draw_text_center("Abrir AuraPhotos", ph_btn, TextStyle::Body12Semi, if ph_hov { theme::DARK_INK } else { p.text_primary });
    hits.push(HitRegion {
        id: "studio_auraphotos".into(),
        rect: ph_r,
        action: Action::LaunchAuraApp("AuraPhotos.exe"),
        right_action: None,
        tooltip: Some("Abrir AuraPhotos Studio".into()),
        slider: None,
    });

    // 3. CARD: VIDEO EDITOR SLOT (Próximo módulo open-source en Aura Studio)
    let vid_x = ph_r.x + ph_r.w + gap;
    let vid_r = Rect::new(vid_x, main_y, side_w, main_h);
    let vid_hov = state.hovered_id.as_deref() == Some("studio_video_player");
    ctx.fill_card(
        vid_r,
        24.0,
        if vid_hov { p.elevated } else { p.card_bg },
        if vid_hov { theme::AMBER } else { p.border },
    );

    let vid_ic = Rect::new(vid_r.x + 20.0, vid_r.y + 20.0, 52.0, 52.0);
    ctx.fill_card(vid_ic, 16.0, theme::rgba(245, 183, 49, 0.14), theme::rgba(245, 183, 49, 0.40));
    ctx.draw_vector_icon(VectorIcon::Film, vid_ic.x + 26.0, vid_ic.y + 26.0, 24.0, theme::AMBER);

    let vid_tag = Rect::new(vid_r.x + vid_r.w - 112.0, vid_r.y + 24.0, 92.0, 21.0);
    ctx.fill_rrect(vid_tag, 8.0, theme::rgba(245, 183, 49, 0.16));
    ctx.draw_text_center("PRÓXIMO MÓDULO", vid_tag, TextStyle::Small9Reg, theme::AMBER);

    ctx.draw_text("Editor de Video", Rect::new(vid_r.x + 20.0, vid_r.y + 88.0, vid_r.w - 40.0, 24.0), TextStyle::Title16Semi, p.text_primary);
    ctx.draw_text(
        "Espacio reservado en Aura Studio para",
        Rect::new(vid_r.x + 20.0, vid_r.y + 118.0, vid_r.w - 40.0, 18.0),
        TextStyle::Body11Reg,
        p.text_secondary,
    );
    ctx.draw_text(
        "tu próximo editor open-source nativo.",
        Rect::new(vid_r.x + 20.0, vid_r.y + 137.0, vid_r.w - 40.0, 18.0),
        TextStyle::Body11Reg,
        p.text_tertiary,
    );

    for (vi, vlbl) in ["Timeline GPU multipista", "Render nativo hardware", "Integración con AuraPlayer"].into_iter().enumerate() {
        let vr = Rect::new(vid_r.x + 20.0, vid_r.y + 182.0 + vi as f32 * 30.0, vid_r.w - 40.0, 24.0);
        ctx.fill_card(vr, 8.0, p.surface, p.border);
        ctx.fill_circle(vr.x + 11.0, vr.y + 12.0, 3.0, theme::AMBER);
        ctx.draw_text(vlbl, Rect::new(vr.x + 20.0, vr.y, vr.w - 26.0, 24.0), TextStyle::Small10Med, p.text_tertiary);
    }

    let vid_btn = Rect::new(vid_r.x + 20.0, vid_r.y + vid_r.h - 56.0, vid_r.w - 40.0, 38.0);
    ctx.fill_card(vid_btn, 13.0, if vid_hov { theme::rgba(245, 183, 49, 0.24) } else { p.elevated }, theme::rgba(245, 183, 49, 0.42));
    ctx.draw_text_center("Abrir AuraPlayer", vid_btn, TextStyle::Body12Semi, theme::AMBER);
    hits.push(HitRegion {
        id: "studio_video_player".into(),
        rect: vid_r,
        action: Action::LaunchAuraApp("AuraPlayer.exe"),
        right_action: None,
        tooltip: Some("Abrir reproductor multimedia AuraPlayer".into()),
        slider: None,
    });

    // --- BOTTOM QUICK CREATIVE TOOLS STRIP ---
    let strip_y = main_y + main_h + gap;
    let tools = [
        ("studio_tool_pdf", "AuraPDF Studio", "Lector & editor PDF nativo", VectorIcon::FileText, theme::CORAL, Action::LaunchAuraPdf),
        ("studio_tool_shot", "AuraShot OCR", "Capturas y texto instantáneo", VectorIcon::Sparkle, p.accent_light, Action::LaunchAuraShot),
        ("studio_tool_player", "AuraPlayer", "Video y audio sin bloatware", VectorIcon::Play, theme::BLUE, Action::LaunchAuraApp("AuraPlayer.exe")),
        ("studio_tool_notes", "AuraNotes", "Notas, guiones y código", VectorIcon::Code, theme::EMERALD, Action::LaunchAuraApp("AuraNotes.exe")),
    ];
    let tw = (area.w - gap * 3.0) / 4.0;
    for (ti, (id, title, sub, icon, accent, act)) in tools.into_iter().enumerate() {
        let tr = Rect::new(area.x + ti as f32 * (tw + gap), strip_y, tw, 76.0);
        let thov = state.hovered_id.as_deref() == Some(id);
        ctx.fill_card(tr, 18.0, if thov { p.elevated } else { p.card_bg }, if thov { p.accent_border } else { p.border });
        let tib = Rect::new(tr.x + 14.0, tr.y + 18.0, 40.0, 40.0);
        ctx.fill_rrect(tib, 12.0, theme::with_alpha(accent, 0.16));
        ctx.draw_vector_icon(icon, tib.x + 20.0, tib.y + 20.0, 17.0, accent);
        ctx.draw_text(title, Rect::new(tr.x + 64.0, tr.y + 17.0, tr.w - 86.0, 20.0), TextStyle::Body12Semi, p.text_primary);
        ctx.draw_text(sub, Rect::new(tr.x + 64.0, tr.y + 39.0, tr.w - 86.0, 16.0), TextStyle::Small9Reg, p.text_tertiary);
        ctx.draw_vector_icon(VectorIcon::External, tr.x + tr.w - 22.0, tr.y + 38.0, 11.0, if thov { p.text_primary } else { p.text_tertiary });
        hits.push(HitRegion {
            id: id.into(),
            rect: tr,
            action: act,
            right_action: None,
            tooltip: Some(format!("Abrir {}", title)),
            slider: None,
        });
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
    ctx.draw_text("WIDGETS DEL ESCRITORIO", Rect::new(wg_card.x + 18.0, wg_card.y + 16.0, left_w - 36.0, 16.0), TextStyle::Small10Bold, theme::accent_light());
    ctx.draw_text("Arrastra cualquiera en tu escritorio · Ajusta tamaño", Rect::new(wg_card.x + 18.0, wg_card.y + 34.0, left_w - 36.0, 14.0), TextStyle::Small9Reg, theme::TEXT_TERTIARY);

    let specs = [
        (WidgetKind::Music, "Reproductor Musical", "Onda animada 60 FPS + SMTC", VectorIcon::MusicNote, theme::accent()),
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
            if enabled { theme::accent_border() } else { theme::BORDER },
        );

        let ib = Rect::new(rr.x + 12.0, rr.y + 14.0, 32.0, 32.0);
        ctx.fill_rrect(ib, 10.0, if enabled { accent } else { theme::CARD_BG });
        ctx.draw_vector_icon(icon, ib.x + 16.0, ib.y + 16.0, 14.0, if enabled { theme::TEXT_PRIMARY } else { theme::TEXT_SECONDARY });

        ctx.draw_text(title, Rect::new(rr.x + 52.0, rr.y + 12.0, rr.w - 110.0, 18.0), TextStyle::Body12Semi, theme::TEXT_PRIMARY);
        ctx.draw_text(sub, Rect::new(rr.x + 52.0, rr.y + 31.0, rr.w - 110.0, 15.0), TextStyle::Small9Reg, theme::TEXT_TERTIARY);

        // Toggle switch pill
        let sw = Rect::new(rr.x + rr.w - 50.0, rr.y + 19.0, 38.0, 22.0);
        ctx.fill_rrect(sw, 11.0, if enabled { theme::accent() } else { theme::CARD_BG });
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
                ctx.fill_rrect(tr, 8.0, if sel { theme::accent() } else { theme::CARD_BG });
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

    let pal = theme::pal();

    // Smart Grid Arrange + Reset positions buttons at bottom of Widgets column
    let grid_r = Rect::new(wg_card.x + 14.0, wg_card.y + wg_card.h - 84.0, left_w - 28.0, 32.0);
    let grid_hov = state.hovered_id.as_deref() == Some("st_wg_grid");
    ctx.fill_card(grid_r, 11.0, if grid_hov { pal.accent_light } else { pal.accent }, pal.accent_border);
    ctx.draw_text_center("Ordenar widgets en cuadrícula", grid_r, TextStyle::Body11Med, pal.text_primary);
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
    ctx.fill_rrect(rst_r, 11.0, if rst_hov { pal.elevated_hover } else { pal.elevated });
    ctx.draw_text_center("Restablecer posiciones por defecto", rst_r, TextStyle::Body11Med, pal.text_secondary);
    hits.push(HitRegion {
        id: "st_wg_reset".into(),
        rect: rst_r,
        action: Action::ResetWidgetsLayout,
        right_action: None,
        tooltip: None,
        slider: None,
    });

    // --- RIGHT PANEL: INTEGRATED WALLPAPER ENGINE (GRADIENT WAVES + PRESETS + WORKSHOP) ---
    let wp_card = Rect::new(area.x + left_w + gap, y0, right_w, panel_h);
    ctx.fill_card(wp_card, 22.0, pal.card_bg, pal.border);

    let vid_cnt = state.wallpapers.iter().filter(|w| w.kind == WallpaperKind::Video).count();
    let web_cnt = state.wallpapers.iter().filter(|w| w.kind == WallpaperKind::Web).count();
    let preset_cnt = state.wallpapers.iter().filter(|w| w.kind == WallpaperKind::Preset).count();
    let hdr_sub = format!("{} predefinidos AuraOS (incl. Gradient Waves) · {} video + {} web Workshop · Auto-pausa 0% GPU", preset_cnt, vid_cnt, web_cnt);
    ctx.draw_text("MOTOR DE FONDOS AURAOS (GRADIENT WAVES + WALLPAPER ENGINE)", Rect::new(wp_card.x + 18.0, wp_card.y + 15.0, right_w - 220.0, 16.0), TextStyle::Small10Bold, pal.accent_light);
    ctx.draw_text(&hdr_sub, Rect::new(wp_card.x + 18.0, wp_card.y + 33.0, right_w - 220.0, 14.0), TextStyle::Small9Reg, pal.text_tertiary);

    // Top-right action pills: [+ Video / Imagen] + optional [Cerrar Wallpaper Engine]
    let add_wp_r = Rect::new(wp_card.x + right_w - 156.0, wp_card.y + 16.0, 140.0, 30.0);
    ctx.fill_rrect(add_wp_r, 11.0, pal.accent);
    ctx.draw_text_center("+ Video / Imagen", add_wp_r, TextStyle::Body11Med, pal.text_primary);
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

    // 1-click Adaptive Gradient Waves Color Bar (changes BOTH the live waves & entire AuraOS palette!)
    let cbar_r = Rect::new(wp_card.x + 14.0, wp_card.y + 54.0, right_w - 28.0, 38.0);
    ctx.fill_card(cbar_r, 12.0, pal.surface, pal.border);
    ctx.draw_text(
        "COLOR GRADIENT WAVES (ADAPTA TODO AURAOS)",
        Rect::new(cbar_r.x + 12.0, cbar_r.y + 11.0, 275.0, 16.0),
        TextStyle::Small9Reg,
        pal.text_secondary,
    );
    let sw_start_x = cbar_r.x + 286.0;
    let sw_avail = (cbar_r.w - 296.0).max(160.0);
    let sw_n = crate::gradient_waves::COLOR_SWATCHES.len() as f32;
    let sw_gap = 6.0;
    let sw_w = ((sw_avail - sw_gap * (sw_n - 1.0)) / sw_n).clamp(24.0, 76.0);
    let active_custom = state.settings.waves_custom_color.trim().to_ascii_uppercase();
    for (si, (name, hex, (sr, sg, sb))) in crate::gradient_waves::COLOR_SWATCHES.iter().enumerate() {
        let sr_rect = Rect::new(sw_start_x + si as f32 * (sw_w + sw_gap), cbar_r.y + 6.0, sw_w, 26.0);
        let sid = format!("st_gw_col_{}", si);
        let hov_s = state.hovered_id.as_deref() == Some(&sid);
        let is_sel = active_custom == *hex
            || (active_custom.is_empty()
                && state
                    .settings
                    .wallpaper_id
                    .strip_prefix("preset:")
                    .and_then(crate::gradient_waves::preset_by_key)
                    .map(|p| p.wave_color == (*sr, *sg, *sb))
                    .unwrap_or(false));
        ctx.fill_card(
            sr_rect,
            8.0,
            if is_sel { pal.accent_tint } else if hov_s { pal.elevated_hover } else { pal.elevated },
            if is_sel { pal.accent_light } else { pal.border },
        );
        ctx.fill_circle(sr_rect.x + 11.0, sr_rect.y + 13.0, 5.0, theme::rgb(*sr, *sg, *sb));
        if sw_w >= 54.0 {
            ctx.draw_text(name, Rect::new(sr_rect.x + 20.0, sr_rect.y + 6.0, sw_w - 23.0, 14.0), TextStyle::Small9Reg, pal.text_primary);
        }
        hits.push(HitRegion {
            id: sid,
            rect: sr_rect,
            action: Action::SetWavesCustomColor((*hex).to_string()),
            right_action: None,
            tooltip: Some(format!("Aplicar color {} ({}) a Gradient Waves y adaptar toda la paleta de AuraOS", name, hex)),
            slider: None,
        });
    }

    // Scrollable 3-column Wallpaper Visual Gallery
    let grid_r = Rect::new(wp_card.x + 14.0, wp_card.y + 100.0, right_w - 28.0, panel_h - 114.0);
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
            if active { pal.accent_tint } else if hov { pal.elevated_hover } else { pal.elevated },
            if active { pal.accent } else if hov { pal.accent_border } else { pal.border },
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
                ctx.fill_gradient_rrect(art, 10.0, theme::rgb(ar / 2, ag / 2, ab / 2), pal.card_bg);
                ctx.draw_vector_icon(
                    if wp.kind == WallpaperKind::External { VectorIcon::Refresh } else { VectorIcon::Sparkle },
                    art.x + art.w * 0.5,
                    art.y + art.h * 0.5,
                    22.0,
                    pal.text_secondary,
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
        ctx.draw_text_center(kind_lbl, badge, TextStyle::Small9Reg, pal.text_primary);

        if active {
            let act_b = Rect::new(art.x + art.w - 52.0, art.y + 6.0, 46.0, 18.0);
            ctx.fill_rrect(act_b, 6.0, pal.accent);
            ctx.draw_text_center("Activo", act_b, TextStyle::Small9Reg, pal.text_primary);
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
        ctx.fill_rrect(rr, 12.0, if sel { theme::accent() } else { theme::rgba(0, 0, 0, 0.0) });

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
        ctx.fill_rrect(sw, 11.0, if on { theme::accent() } else { theme::CARD_BG });
        ctx.fill_circle(if on { sw.x + 29.0 } else { sw.x + 11.0 }, sw.y + 11.0, 8.0, theme::CRISP_WHITE);
        hits.push(HitRegion { id: format!("set_{}", key), rect: rr, action: Action::ToggleSetting(key), right_action: None, tooltip: None, slider: None });
        y += 51.0;
    }

    let btn_res = Rect::new(card.x + 20.0, y + 10.0, 196.0, 36.0);
    ctx.fill_rrect(btn_res, 12.0, theme::CORAL_BG);
    ctx.draw_text_center("Restaurar barra de Windows", btn_res, TextStyle::Body11Med, theme::CORAL);
    hits.push(HitRegion { id: "set_restore".into(), rect: btn_res, action: Action::RestoreWindowsAndExit, right_action: None, tooltip: None, slider: None });

    let btn_upd = Rect::new(card.x + card.w - 170.0, y + 10.0, 150.0, 36.0);
    ctx.fill_rrect(btn_upd, 12.0, theme::accent());
    ctx.draw_text_center("Actualizar versión", btn_upd, TextStyle::Body11Med, theme::TEXT_PRIMARY);
    hits.push(HitRegion { id: "set_upd".into(), rect: btn_upd, action: Action::LaunchUpdater, right_action: None, tooltip: None, slider: None });
}

// ==================== 6. INLINE MODAL: + AGREGAR IA / APP WEB ====================

fn render_add_dev_modal(ctx: &mut DrawCtx<'_>, state: &AppState, w: f32, h: f32, hits: &mut Vec<HitRegion>) {
    ctx.fill_rrect(Rect::new(0.0, 0.0, w, h), 0.0, theme::rgba(0, 0, 0, 0.55));
    let mw = 440.0;
    let mh = 278.0;
    let mr = Rect::new((w - mw) * 0.5, (h - mh) * 0.5, mw, mh);
    ctx.fill_card(mr, 22.0, theme::SURFACE, theme::accent_border());

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
    ctx.fill_card(f0, 11.0, theme::CARD_BG, if state.add_modal.active_field == 0 { theme::accent() } else { theme::BORDER });
    let n_txt = if state.add_modal.name.is_empty() { "Escribe el nombre..." } else { state.add_modal.name.as_str() };
    ctx.draw_text(n_txt, Rect::new(f0.x + 12.0, f0.y, f0.w - 24.0, 36.0), TextStyle::Body12Med, if state.add_modal.name.is_empty() { theme::TEXT_TERTIARY } else { theme::TEXT_PRIMARY });
    hits.push(HitRegion { id: "mod_f0".into(), rect: f0, action: Action::FocusModalField(0), right_action: None, tooltip: None, slider: None });

    // Field 1: URL
    ctx.draw_text("URL WEB (ej. https://chat.deepseek.com)", Rect::new(mr.x + 22.0, mr.y + 132.0, 320.0, 15.0), TextStyle::Small9Reg, theme::TEXT_TERTIARY);
    let f1 = Rect::new(mr.x + 22.0, mr.y + 150.0, mw - 44.0, 36.0);
    ctx.fill_card(f1, 11.0, theme::CARD_BG, if state.add_modal.active_field == 1 { theme::accent() } else { theme::BORDER });
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
    ctx.fill_rrect(b_save, 11.0, theme::accent());
    ctx.draw_text_center("Guardar IA", b_save, TextStyle::Body11Med, theme::TEXT_PRIMARY);
    hits.push(HitRegion { id: "mod_save".into(), rect: b_save, action: Action::SubmitAddDevWebModal, right_action: None, tooltip: None, slider: None });
}
