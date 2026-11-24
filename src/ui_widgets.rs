//! AuraOS v5.1 Modular Draggable Desktop Widgets (Material You 3 + illogical-impulse Styles!)
//! - Clock Widget with 3 Expressive Styles:
//!   • `Vertical` (bold stacked `23` / `47` + side date & weather badge)
//!   • `Cookie` (Material 3 scalloped rosette clock badge)
//!   • `Horizontal` (classic wide translucent glass clock)
//! - Animated Music Widget with Material You aura, 26-bar harmonic wave + wavy progress bar!
//! - Live Weather & PC Monitor Widgets fully synchronized with dynamic Material You palette!

use crate::models::WidgetKind;
use crate::renderer::{DrawCtx, Rect, TextStyle};
use crate::theme::{self, VectorIcon};
use crate::ui_state::{Action, AppState, HitRegion};

pub fn widget_base_size(kind: WidgetKind) -> (f32, f32) {
    match kind {
        WidgetKind::Clock => (420.0, 196.0),
        WidgetKind::Music => (396.0, 174.0),
        WidgetKind::Weather => (308.0, 152.0),
        WidgetKind::Monitor => (308.0, 144.0),
    }
}

pub fn widget_scale_factor(tier: u8) -> f32 {
    match tier {
        0 => 0.85,
        2 => 1.18,
        _ => 1.0,
    }
}

pub fn widget_window_dims(kind: WidgetKind, tier: u8) -> (i32, i32) {
    let (bw, bh) = widget_base_size(kind);
    let s = widget_scale_factor(tier);
    ((bw * s).round() as i32, (bh * s).round() as i32)
}

pub fn render_desktop_widget(
    ctx: &mut DrawCtx<'_>,
    state: &AppState,
    kind: WidgetKind,
    tier: u8,
    hovered_window: bool,
    magnetic_locked: bool,
    hits: &mut Vec<HitRegion>,
) {
    hits.clear();
    let p = theme::pal();
    let (bw, bh) = widget_base_size(kind);
    let scale = widget_scale_factor(tier);
    let scaled = (scale - 1.0).abs() > 0.001;
    if scaled {
        ctx.set_transform_scale_origin(scale, 0.0, 0.0, 0.0, 0.0);
    }

    match kind {
        WidgetKind::Clock => render_clock_widget(ctx, state, bw, bh),
        WidgetKind::Music => render_music_widget(ctx, state, bw, bh, hits),
        WidgetKind::Weather => render_weather_widget(ctx, state, bw, bh),
        WidgetKind::Monitor => render_monitor_widget(ctx, state, bw, bh),
    }

    if magnetic_locked {
        let card = Rect::new(4.0, 4.0, bw - 8.0, bh - 8.0);
        ctx.stroke_rrect(card, 22.0, p.accent_light, 2.4);
        let badge = Rect::new(bw - 104.0, 10.0, 90.0, 18.0);
        ctx.fill_rrect(badge, 9.0, p.accent);
        ctx.draw_text_center("Acoplado", badge, TextStyle::Small9Reg, theme::CRISP_WHITE);
    }

    // Unified macOS Traffic Lights in top-left of EVERY desktop widget (× hide, − shrink, + grow)
    let lights = [
        ("wg_hide", 20.0f32, theme::CORAL, "×", Action::ToggleWidget(kind), "Ocultar widget"),
        ("wg_dec", 36.0f32, theme::AMBER, "−", Action::ResizeWidget(kind, -1), "Reducir tamaño"),
        ("wg_inc", 52.0f32, theme::EMERALD, "+", Action::ResizeWidget(kind, 1), "Aumentar tamaño"),
    ];
    let any_light_hov = hovered_window
        || matches!(state.hovered_id.as_deref(), Some("wg_hide") | Some("wg_dec") | Some("wg_inc"));
    let pill = Rect::new(11.0, 10.0, 50.0, 17.0);
    if any_light_hov {
        ctx.fill_card(pill, 8.5, theme::with_alpha(p.canvas, 0.86), p.border_strong);
    }
    for (id, cx, col, glyph, action, tip) in lights {
        let cy = 18.5;
        let draw_col = if any_light_hov { col } else { theme::with_alpha(col, 0.72) };
        ctx.fill_circle(cx, cy, if any_light_hov { 5.2 } else { 4.3 }, draw_col);
        if any_light_hov {
            ctx.draw_text_center(glyph, Rect::new(cx - 5.5, cy - 6.5, 11.0, 12.0), TextStyle::Small9Reg, theme::DARK_INK);
        }
        hits.push(HitRegion {
            id: id.into(),
            rect: Rect::new((cx - 7.5) * scale, (cy - 8.0) * scale, 15.0 * scale, 16.0 * scale),
            action,
            right_action: None,
            tooltip: Some(tip.into()),
            slider: None,
        });
    }

    if scaled {
        for h in hits.iter_mut() {
            if !h.id.starts_with("wg_") {
                h.rect = Rect::new(h.rect.x * scale, h.rect.y * scale, h.rect.w * scale, h.rect.h * scale);
            }
        }
        ctx.reset_transform();
    }
}

// ==================== 1. CLOCK WIDGET (VERTICAL / COOKIE / HORIZONTAL) ====================

fn render_clock_widget(ctx: &mut DrawCtx<'_>, state: &AppState, w: f32, h: f32) {
    use windows::Win32::System::SystemInformation::GetLocalTime;
    let p = theme::pal();
    let st = unsafe { GetLocalTime() };
    let hh = format!("{:02}", st.wHour);
    let mm = format!("{:02}", st.wMinute);
    let time_str = format!("{}:{}", hh, mm);
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
    let date_str = format!("{}, {} de {}", dow, st.wDay, mon);
    let w_txt = if state.weather.loaded && !state.weather.city.is_empty() {
        format!("{}°C · {} · {}", state.weather.temp_c, state.weather.condition, state.weather.city)
    } else {
        "Clima en vivo...".to_string()
    };

    match state.settings.clock_widget_style.to_lowercase().as_str() {
        "vertical" => {
            // Iconic illogical-impulse stacked bold `23` / `47` + Material You info column!
            let card = Rect::new(4.0, 4.0, w - 8.0, h - 8.0);
            ctx.fill_card(card, 26.0, theme::with_alpha(p.surface, 0.72), theme::with_alpha(p.accent_border, 0.45));

            // Left stacked giant digits pill
            let dig_box = Rect::new(16.0, 32.0, 142.0, h - 46.0);
            ctx.fill_rrect(dig_box, 20.0, theme::with_alpha(p.accent_tint, 0.85));
            ctx.draw_text_center(&hh, Rect::new(dig_box.x, dig_box.y - 4.0, dig_box.w, 82.0), TextStyle::Clock72Light, p.text_primary);
            ctx.draw_text_center(&mm, Rect::new(dig_box.x, dig_box.y + 66.0, dig_box.w, 82.0), TextStyle::Clock72Light, p.accent_light);

            // Right date + weather + status column
            let rx = 174.0;
            let rw = w - rx - 20.0;
            ctx.draw_text("AURAOS", Rect::new(rx, 40.0, rw, 16.0), TextStyle::Small10Bold, p.accent_light);
            ctx.draw_text(&date_str, Rect::new(rx, 60.0, rw, 26.0), TextStyle::Title16Semi, p.text_primary);

            let wp = Rect::new(rx, 96.0, rw, 30.0);
            ctx.fill_card(wp, 12.0, theme::with_alpha(p.card_bg, 0.88), p.border);
            ctx.draw_text(&w_txt, Rect::new(wp.x + 10.0, wp.y, wp.w - 16.0, 30.0), TextStyle::Body11Med, p.text_secondary);

            let up = Rect::new(rx, 136.0, rw, 28.0);
            ctx.fill_rrect(up, 11.0, theme::with_alpha(p.pill_subtle, 0.82));
            let stat_line = format!("CPU {}% · RAM {}%", state.cpu_pct.round() as i32, state.ram_pct);
            ctx.draw_text(&stat_line, Rect::new(up.x + 10.0, up.y, up.w - 16.0, 28.0), TextStyle::Mono11Med, p.text_tertiary);
        }
        "cookie" => {
            // Expressive Material 3 Scalloped Cookie / Rosette Clock Badge!
            let card = Rect::new(4.0, 4.0, w - 8.0, h - 8.0);
            ctx.fill_card(card, 28.0, theme::with_alpha(p.canvas, 0.52), theme::with_alpha(p.border, 0.55));

            let cx = 104.0;
            let cy = h * 0.5 + 4.0;
            // 10 scalloped cookie petals around center
            for i in 0..10 {
                let ang = (i as f32) * (std::f32::consts::TAU / 10.0);
                ctx.fill_circle(cx + ang.cos() * 48.0, cy + ang.sin() * 48.0, 26.0, p.accent_tint);
            }
            ctx.fill_circle(cx, cy, 58.0, p.accent_tint);
            ctx.stroke_circle(cx, cy, 62.0, theme::with_alpha(p.accent_light, 0.45), 1.6);
            ctx.draw_text_center(&hh, Rect::new(cx - 50.0, cy - 46.0, 100.0, 46.0), TextStyle::HeroTemp34Bold, p.text_primary);
            ctx.draw_text_center(&mm, Rect::new(cx - 50.0, cy - 2.0, 100.0, 46.0), TextStyle::HeroTemp34Bold, p.accent_light);

            let rx = 192.0;
            let rw = w - rx - 18.0;
            ctx.draw_text(&date_str, Rect::new(rx, 52.0, rw, 26.0), TextStyle::Title14Semi, p.text_primary);
            let pill = Rect::new(rx, 88.0, rw, 30.0);
            ctx.fill_card(pill, 14.0, theme::with_alpha(p.surface, 0.88), p.accent_border);
            ctx.draw_text(&w_txt, Rect::new(pill.x + 10.0, pill.y, pill.w - 16.0, 30.0), TextStyle::Body11Med, p.text_primary);
            ctx.draw_text(&state.uptime_label, Rect::new(rx, 128.0, rw, 20.0), TextStyle::Small10Med, p.text_tertiary);
        }
        _ => {
            // Classic Horizontal Translucent Clock
            ctx.fill_card(
                Rect::new(4.0, 4.0, w - 8.0, h - 8.0),
                24.0,
                theme::with_alpha(p.canvas, 0.60),
                theme::rgba(255, 255, 255, 0.10),
            );
            ctx.draw_text_center(&time_str, Rect::new(2.0, 16.0, w, 86.0), TextStyle::Clock72Light, theme::rgba(0, 0, 0, 0.40));
            ctx.draw_text_center(&time_str, Rect::new(0.0, 14.0, w, 86.0), TextStyle::Clock72Light, p.text_primary);
            ctx.draw_text_center(&date_str, Rect::new(0.0, 104.0, w, 24.0), TextStyle::Title14Semi, p.text_secondary);
            let pill = Rect::new((w - 280.0) * 0.5, 138.0, 280.0, 28.0);
            ctx.fill_card(pill, 14.0, theme::with_alpha(p.surface, 0.84), p.accent_border);
            ctx.draw_text_center(&w_txt, pill, TextStyle::Body11Med, p.text_primary);
        }
    }
}

// ==================== 2. ANIMATED MUSIC WIDGET (MATERIAL YOU + WAVY BAR!) ====================

fn render_music_widget(ctx: &mut DrawCtx<'_>, state: &AppState, w: f32, h: f32, hits: &mut Vec<HitRegion>) {
    let p = theme::pal();
    let playing = state.now_playing.is_playing;
    let phase = state.music_wave_phase;
    let energy = state.music_energy.clamp(0.0, 1.0);

    let card = Rect::new(4.0, 4.0, w - 8.0, h - 8.0);
    let border = if playing {
        theme::with_alpha(p.accent, 0.48 + 0.16 * (phase * 1.4).sin().abs())
    } else {
        p.border_strong
    };
    ctx.fill_card(card, 22.0, theme::with_alpha(p.surface, 0.90), border);

    let _clip = ctx.push_rrect_mask_layer(card, card, 22.0, 1.0);
    if energy > 0.02 {
        let gx = card.x + 68.0 + (phase * 0.7).sin() * 24.0;
        let gy = card.y + 52.0 + (phase * 0.5).cos() * 14.0;
        ctx.fill_radial_glow(gx, gy, 150.0, theme::with_alpha(p.accent, 0.24 * energy), theme::with_alpha(p.accent, 0.0));
        let gx2 = card.x + card.w - 64.0 + (phase * 0.9).cos() * 20.0;
        ctx.fill_radial_glow(gx2, card.y + card.h - 24.0, 120.0, theme::with_alpha(p.accent_light, 0.16 * energy), theme::with_alpha(p.accent_light, 0.0));
    }

    let pulse = 1.0 + 0.04 * energy * (phase * 3.2).sin();
    let art_sz = 50.0 * pulse;
    let art_r = Rect::new(card.x + 16.0 + (50.0 - art_sz) * 0.5, card.y + 30.0 + (50.0 - art_sz) * 0.5, art_sz, art_sz);
    ctx.fill_gradient_rrect(art_r, 15.0, p.accent_light, p.accent_deep);
    ctx.stroke_circle(art_r.x + art_r.w * 0.5, art_r.y + art_r.h * 0.5, 15.0, theme::rgba(255, 255, 255, 0.16), 1.2);
    ctx.draw_vector_icon(VectorIcon::MusicNote, art_r.x + art_r.w * 0.5, art_r.y + art_r.h * 0.5, 19.0, theme::CRISP_WHITE);

    let source_badge = if state.now_playing.source_app.is_empty() { "Reproductor" } else { state.now_playing.source_app.as_str() };
    ctx.fill_circle(card.x + 80.0, card.y + 36.0, 3.5, if playing { theme::EMERALD } else { p.text_tertiary });
    let hdr_txt = format!("{} · {}", source_badge, if playing { "Reproduciendo" } else { "En pausa" });
    ctx.draw_text(&hdr_txt, Rect::new(card.x + 89.0, card.y + 28.0, card.w - 190.0, 16.0), TextStyle::Small10Med, p.accent_light);
    ctx.draw_text(&state.now_playing.title, Rect::new(card.x + 76.0, card.y + 46.0, card.w - 192.0, 22.0), TextStyle::Title14Semi, p.text_primary);
    let artist_line = if state.now_playing.artist.is_empty() { "Abre Spotify, YouTube o tu reproductor".to_string() } else { state.now_playing.artist.clone() };
    ctx.draw_text(&artist_line, Rect::new(card.x + 76.0, card.y + 68.0, card.w - 192.0, 17.0), TextStyle::Body11Reg, p.text_secondary);

    let btns = [
        ("mw_prev", VectorIcon::Prev, false, Action::MediaPrev),
        ("mw_play", VectorIcon::PlayPause, true, Action::MediaPlayPause),
        ("mw_next", VectorIcon::Next, false, Action::MediaNext),
    ];
    let mut bx = card.x + card.w - 108.0;
    for (id, ic, hero, act) in btns {
        let sz = if hero { 32.0 } else { 25.0 };
        let by = card.y + 60.0 - sz * 0.5;
        let br = Rect::new(bx, by, sz, sz);
        let hov = state.hovered_id.as_deref() == Some(id);
        let bg = if hero {
            if hov { p.accent_light } else { theme::CRISP_WHITE }
        } else if hov {
            p.elevated_hover
        } else {
            p.elevated
        };
        ctx.fill_rrect(br, sz * 0.5, bg);
        ctx.draw_vector_icon(ic, br.x + sz * 0.5, br.y + sz * 0.5, if hero { 13.0 } else { 11.0 }, if hero { theme::DARK_INK } else { p.text_primary });
        hits.push(HitRegion { id: id.into(), rect: br, action: act, right_action: None, tooltip: None, slider: None });
        bx += sz + 6.0;
    }

    // Harmonic Visualizer + Material 3 Wavy Progress Line
    let viz_r = Rect::new(card.x + 16.0, card.y + 92.0, card.w - 32.0, 42.0);
    ctx.fill_rrect(viz_r, 12.0, theme::rgba(255, 255, 255, 0.035));
    let bars = 26usize;
    let bar_gap = 4.0f32;
    let bar_w = ((viz_r.w - 16.0 - (bars as f32 - 1.0) * bar_gap) / bars as f32).max(4.0);
    let mid_y = viz_r.y + viz_r.h * 0.5;
    for i in 0..bars {
        let u = i as f32 / (bars - 1) as f32;
        let env = (std::f32::consts::PI * u).sin().powf(0.75);
        let w1 = (phase * 4.2 + i as f32 * 0.55).sin();
        let w2 = (phase * 6.7 - i as f32 * 0.92).cos() * 0.55;
        let w3 = (phase * 2.4 + i as f32 * 0.28).sin() * 0.45;
        let norm = ((w1 + w2 + w3) * 0.35 + 0.55).clamp(0.08, 1.0);
        let bar_h = 4.0 + energy * env * norm * (viz_r.h - 10.0);
        let x = viz_r.x + 8.0 + i as f32 * (bar_w + bar_gap);
        let y = mid_y - bar_h * 0.5;
        let col = if energy > 0.05 {
            theme::with_alpha(p.accent_light, 0.45 + 0.52 * norm * energy)
        } else {
            theme::with_alpha(p.text_tertiary, 0.28)
        };
        ctx.fill_rrect(Rect::new(x, y, bar_w, bar_h), bar_w * 0.5, col);
    }

    let wave_prog = if playing { 0.32 + ((phase * 0.04).sin() * 0.5 + 0.5) * 0.48 } else { 0.40 };
    ctx.draw_wavy_progress_bar(
        Rect::new(card.x + 18.0, card.y + card.h - 22.0, card.w - 36.0, 12.0),
        wave_prog,
        phase,
        playing,
        p.accent_light,
        theme::with_alpha(p.border_strong, 0.7),
    );

    if _clip.is_some() {
        ctx.pop_layer();
    }
}

// ==================== 3. LIVE WEATHER CARD WIDGET ====================

fn render_weather_widget(ctx: &mut DrawCtx<'_>, state: &AppState, w: f32, h: f32) {
    let p = theme::pal();
    let card = Rect::new(4.0, 4.0, w - 8.0, h - 8.0);
    ctx.fill_gradient_rrect(card, 22.0, p.accent_light, p.accent_deep);
    ctx.stroke_rrect(card, 22.0, theme::rgba(255, 255, 255, 0.16), 1.0);

    let ic_r = Rect::new(card.x + 16.0, card.y + 32.0, 38.0, 38.0);
    ctx.fill_rrect(ic_r, 12.0, theme::rgba(255, 255, 255, 0.22));
    ctx.draw_vector_icon(
        if state.weather.weather_code <= 1 { VectorIcon::Sun } else { VectorIcon::Cloud },
        ic_r.x + 19.0,
        ic_r.y + 19.0,
        18.0,
        if state.weather.weather_code <= 1 { theme::AMBER } else { theme::CRISP_WHITE },
    );

    let city_lbl = if state.weather.loaded && !state.weather.city.is_empty() { state.weather.city.as_str() } else { "Ubicando..." };
    let temp_lbl = if state.weather.loaded { format!("{}°", state.weather.temp_c) } else { "--°".to_string() };
    ctx.draw_text("CLIMA EN VIVO", Rect::new(card.x + 64.0, card.y + 33.0, 120.0, 14.0), TextStyle::Small9Reg, theme::rgba(245, 245, 255, 0.85));
    ctx.draw_text(city_lbl, Rect::new(card.x + 64.0, card.y + 48.0, 140.0, 20.0), TextStyle::Body12Semi, theme::CRISP_WHITE);
    ctx.draw_text_right(&temp_lbl, Rect::new(card.x + card.w - 84.0, card.y + 28.0, 68.0, 44.0), TextStyle::HeroTemp34Bold, theme::CRISP_WHITE);

    let pw = (card.w - 44.0) / 3.0;
    let pills = [
        format!("Sens {}°", state.weather.feels_like_c),
        format!("Hum {}%", state.weather.humidity_pct),
        format!("{} km/h", state.weather.wind_kmh),
    ];
    for (i, txt) in pills.into_iter().enumerate() {
        let pr = Rect::new(card.x + 14.0 + i as f32 * (pw + 8.0), card.y + card.h - 40.0, pw, 26.0);
        ctx.fill_rrect(pr, 9.0, theme::rgba(0, 0, 0, 0.22));
        ctx.draw_text_center(&txt, pr, TextStyle::Small10Med, theme::CRISP_WHITE);
    }
}

// ==================== 4. SYSTEM MONITOR WIDGET ====================

fn render_monitor_widget(ctx: &mut DrawCtx<'_>, state: &AppState, w: f32, h: f32) {
    let p = theme::pal();
    let card = Rect::new(4.0, 4.0, w - 8.0, h - 8.0);
    ctx.fill_card(card, 22.0, theme::with_alpha(p.surface, 0.90), p.border_strong);

    ctx.draw_text("MONITOR", Rect::new(card.x + 66.0, card.y + 10.0, 76.0, 15.0), TextStyle::Small9Reg, p.text_tertiary);
    ctx.draw_text_right(
        &format!("CPU {}% · RAM {}%", state.cpu_pct.round() as i32, state.ram_pct),
        Rect::new(card.x + 132.0, card.y + 9.0, card.w - 148.0, 18.0),
        TextStyle::Mono11Med,
        p.accent_light,
    );

    ctx.draw_area_graph(Rect::new(card.x + 16.0, card.y + 38.0, card.w - 32.0, 58.0), &state.cpu_history, p.accent);

    let ram_bar = Rect::new(card.x + 16.0, card.y + card.h - 24.0, card.w - 32.0, 7.0);
    ctx.fill_rrect(ram_bar, 3.5, p.elevated);
    ctx.fill_rrect(
        Rect::new(ram_bar.x, ram_bar.y, (state.ram_pct as f32 / 100.0).clamp(0.05, 1.0) * ram_bar.w, 7.0),
        3.5,
        if state.ram_pct > 80 { theme::CORAL } else { theme::EMERALD },
    );
}
