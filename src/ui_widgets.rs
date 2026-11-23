//! Modular Draggable & Resizable Desktop Widgets Engine (v3.1)
//! - Clock & Date Widget (Segoe UI Variable Light + Weather pill)
//! - Animated Music Widget (Live Windows SMTC + Fluid 60 FPS Harmonic Visualizer Wave + Controls!)
//! - Live Weather Card Widget (Electric Violet Drool Card)
//! - PC System Monitor Widget (Live CPU Sparkline + RAM Pill)
//! - Drag anywhere on widget to reposition; hover top-right controls to resize (- / +) or hide (x)!

use crate::models::WidgetKind;
use crate::renderer::{DrawCtx, Rect, TextStyle};
use crate::theme::{self, VectorIcon};
use crate::ui_state::{Action, AppState, HitRegion};

pub fn widget_base_size(kind: WidgetKind) -> (f32, f32) {
    match kind {
        WidgetKind::Clock => (420.0, 184.0),
        WidgetKind::Music => (396.0, 162.0),
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
    hits: &mut Vec<HitRegion>,
) {
    hits.clear();
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

    // Unified macOS Traffic Lights in top-left of EVERY desktop widget (× hide, − shrink, + grow)!
    // Always softly integrated for 100% OS-wide visual coherence; illuminates with crisp × − + glyphs on hover!
    let lights = [
        ("wg_hide", 20.0f32, theme::CORAL, "×", Action::ToggleWidget(kind), "Ocultar widget"),
        ("wg_dec", 36.0f32, theme::AMBER, "−", Action::ResizeWidget(kind, -1), "Reducir tamaño"),
        ("wg_inc", 52.0f32, theme::EMERALD, "+", Action::ResizeWidget(kind, 1), "Aumentar tamaño"),
    ];
    let any_light_hov = hovered_window
        || matches!(state.hovered_id.as_deref(), Some("wg_hide") | Some("wg_dec") | Some("wg_inc"));
    let pill = Rect::new(11.0, 10.0, 50.0, 17.0);
    if any_light_hov {
        ctx.fill_card(pill, 8.5, theme::rgba(14, 14, 21, 0.84), theme::rgba(255, 255, 255, 0.14));
    }
    for (id, cx, col, glyph, action, tip) in lights {
        let cy = 18.5;
        let draw_col = if any_light_hov {
            col
        } else {
            windows::Win32::Graphics::Direct2D::Common::D2D1_COLOR_F { a: 0.72, ..col }
        };
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

    // Scale any widget inner hits to actual window coordinates
    if scaled {
        for h in hits.iter_mut() {
            if !h.id.starts_with("wg_") {
                h.rect = Rect::new(h.rect.x * scale, h.rect.y * scale, h.rect.w * scale, h.rect.h * scale);
            }
        }
        ctx.reset_transform();
    }
}

// ==================== 1. CLOCK & DATE WIDGET ====================

fn render_clock_widget(ctx: &mut DrawCtx<'_>, state: &AppState, w: f32, h: f32) {
    use windows::Win32::System::SystemInformation::GetLocalTime;
    let st = unsafe { GetLocalTime() };
    let time_str = format!("{:02}:{:02}", st.wHour, st.wMinute);
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

    // Subtle matte glass hover/readability backdrop
    ctx.fill_card(
        Rect::new(4.0, 4.0, w - 8.0, h - 8.0),
        24.0,
        theme::rgba(14, 14, 21, 0.56),
        theme::rgba(255, 255, 255, 0.08),
    );

    ctx.draw_text_center(
        &time_str,
        Rect::new(2.0, 12.0, w, 86.0),
        TextStyle::Clock72Light,
        theme::rgba(0, 0, 0, 0.40),
    );
    ctx.draw_text_center(
        &time_str,
        Rect::new(0.0, 10.0, w, 86.0),
        TextStyle::Clock72Light,
        theme::rgba(255, 255, 255, 0.96),
    );

    ctx.draw_text_center(
        &date_str,
        Rect::new(0.0, 98.0, w, 24.0),
        TextStyle::Title14Semi,
        theme::rgba(235, 236, 248, 0.88),
    );

    let pill = Rect::new((w - 224.0) * 0.5, 132.0, 224.0, 28.0);
    ctx.fill_card(pill, 14.0, theme::rgba(18, 18, 28, 0.80), theme::rgba(124, 92, 255, 0.30));
    let w_txt = format!("{}°C  ·  {}  ·  {}", state.weather.temp_c, state.weather.condition, state.weather.city);
    ctx.draw_text_center(&w_txt, pill, TextStyle::Body11Med, theme::TEXT_PRIMARY);
}

// ==================== 2. ANIMATED MUSIC WIDGET (FLUID 60 FPS VISUALIZER!) ====================

fn render_music_widget(
    ctx: &mut DrawCtx<'_>,
    state: &AppState,
    w: f32,
    h: f32,
    hits: &mut Vec<HitRegion>,
) {
    let playing = state.now_playing.is_playing;
    let phase = state.music_wave_phase;
    let energy = state.music_energy.clamp(0.0, 1.0); // smoothly eases 0.0 -> 1.0 when playing!

    let card = Rect::new(4.0, 4.0, w - 8.0, h - 8.0);
    let border = if playing {
        theme::rgba(124, 92, 255, 0.42 + 0.16 * (phase * 1.4).sin().abs())
    } else {
        theme::BORDER_STRONG
    };
    ctx.fill_card(card, 22.0, theme::rgba(16, 16, 24, 0.88), border);

    // Soft animated Electric Violet ambient radial aura behind album disc while playing!
    let _clip = ctx.push_rrect_mask_layer(card, card, 22.0, 1.0);
    if energy > 0.02 {
        let gx = card.x + 68.0 + (phase * 0.7).sin() * 24.0;
        let gy = card.y + 52.0 + (phase * 0.5).cos() * 14.0;
        ctx.fill_radial_glow(
            gx,
            gy,
            150.0,
            theme::rgba(124, 92, 255, 0.24 * energy),
            theme::rgba(124, 92, 255, 0.0),
        );
        let gx2 = card.x + card.w - 64.0 + (phase * 0.9).cos() * 20.0;
        ctx.fill_radial_glow(
            gx2,
            card.y + card.h - 24.0,
            120.0,
            theme::rgba(242, 109, 125, 0.14 * energy),
            theme::rgba(242, 109, 125, 0.0),
        );
    }

    // Album / Vinyl Art Badge (pulsing softly with beat! Positioned below unified top-left traffic lights)
    let pulse = 1.0 + 0.04 * energy * (phase * 3.2).sin();
    let art_sz = 50.0 * pulse;
    let art_r = Rect::new(card.x + 16.0 + (50.0 - art_sz) * 0.5, card.y + 30.0 + (50.0 - art_sz) * 0.5, art_sz, art_sz);
    ctx.fill_gradient_rrect(art_r, 15.0, theme::VIOLET_LIGHT, theme::VIOLET_DEEP);
    // Subtle inner vinyl ring
    ctx.stroke_circle(art_r.x + art_r.w * 0.5, art_r.y + art_r.h * 0.5, 15.0, theme::rgba(255, 255, 255, 0.16), 1.2);
    ctx.draw_vector_icon(
        VectorIcon::MusicNote,
        art_r.x + art_r.w * 0.5,
        art_r.y + art_r.h * 0.5,
        19.0,
        theme::TEXT_PRIMARY,
    );

    // Live SMTC Track Metadata
    let source_badge = if state.now_playing.source_app.is_empty() {
        "Reproductor"
    } else {
        state.now_playing.source_app.as_str()
    };
    let status_dot_col = if playing { theme::EMERALD } else { theme::TEXT_TERTIARY };
    ctx.fill_circle(card.x + 80.0, card.y + 36.0, 3.5, status_dot_col);
    let hdr_txt = format!(
        "{} · {}",
        source_badge,
        if playing { "Reproduciendo" } else { "En pausa" }
    );
    ctx.draw_text(&hdr_txt, Rect::new(card.x + 89.0, card.y + 28.0, card.w - 190.0, 16.0), TextStyle::Small10Med, theme::VIOLET_LIGHT);

    ctx.draw_text(
        &state.now_playing.title,
        Rect::new(card.x + 76.0, card.y + 46.0, card.w - 192.0, 22.0),
        TextStyle::Title14Semi,
        theme::TEXT_PRIMARY,
    );
    let artist_line = if state.now_playing.artist.is_empty() {
        "Abre Spotify, YouTube o tu reproductor".to_string()
    } else {
        state.now_playing.artist.clone()
    };
    ctx.draw_text(
        &artist_line,
        Rect::new(card.x + 76.0, card.y + 68.0, card.w - 192.0, 17.0),
        TextStyle::Body11Reg,
        theme::TEXT_SECONDARY,
    );

    // Interactive Media Controls on Right (Prev · Play/Pause · Next)
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
            if hov { theme::VIOLET_LIGHT } else { theme::CRISP_WHITE }
        } else if hov {
            theme::ELEVATED_HOVER
        } else {
            theme::ELEVATED
        };
        ctx.fill_rrect(br, sz * 0.5, bg);
        ctx.draw_vector_icon(
            ic,
            br.x + sz * 0.5,
            br.y + sz * 0.5,
            if hero { 13.0 } else { 11.0 },
            if hero { theme::DARK_INK } else { theme::TEXT_PRIMARY },
        );
        hits.push(HitRegion {
            id: id.into(),
            rect: br,
            action: act,
            right_action: None,
            tooltip: None,
            slider: None,
        });
        bx += sz + 6.0;
    }

    // ==================== FLUID HARMONIC WAVEFORM VISUALIZER ====================
    let viz_r = Rect::new(card.x + 16.0, card.y + 94.0, card.w - 32.0, 46.0);
    ctx.fill_rrect(viz_r, 12.0, theme::rgba(255, 255, 255, 0.035));

    let bars = 26usize;
    let bar_gap = 4.0f32;
    let bar_w = ((viz_r.w - 16.0 - (bars as f32 - 1.0) * bar_gap) / bars as f32).max(4.0);
    let mid_y = viz_r.y + viz_r.h * 0.5;

    for i in 0..bars {
        let u = i as f32 / (bars - 1) as f32; // 0..1 across width
        // Bell envelope so center waves rise taller and edges taper organically
        let env = (std::f32::consts::PI * u).sin().powf(0.75);
        // Layered harmonic frequencies for lively organic musical motion
        let w1 = (phase * 4.2 + i as f32 * 0.55).sin();
        let w2 = (phase * 6.7 - i as f32 * 0.92).cos() * 0.55;
        let w3 = (phase * 2.4 + i as f32 * 0.28).sin() * 0.45;
        let norm = ((w1 + w2 + w3) * 0.35 + 0.55).clamp(0.08, 1.0);

        let bar_h = 4.0 + energy * env * norm * (viz_r.h - 12.0);
        let x = viz_r.x + 8.0 + i as f32 * (bar_w + bar_gap);
        let y = mid_y - bar_h * 0.5;

        // Smooth violet -> lavender -> subtle coral crest gradient across bars
        let col = if energy > 0.05 {
            let r = (124.0 + u * 85.0).min(242.0) as u8;
            let g = (92.0 + (1.0 - (u - 0.5).abs() * 2.0) * 58.0) as u8;
            let b = (255.0 - u * 45.0) as u8;
            theme::rgba(r, g, b, 0.45 + 0.52 * norm * energy)
        } else {
            theme::rgba(138, 138, 162, 0.25)
        };
        ctx.fill_rrect(Rect::new(x, y, bar_w, bar_h), bar_w * 0.5, col);
    }

    if _clip.is_some() {
        ctx.pop_layer();
    }
}

// ==================== 3. LIVE WEATHER CARD WIDGET ====================

fn render_weather_widget(ctx: &mut DrawCtx<'_>, state: &AppState, w: f32, h: f32) {
    let card = Rect::new(4.0, 4.0, w - 8.0, h - 8.0);
    ctx.fill_gradient_rrect(card, 22.0, theme::VIOLET_LIGHT, theme::VIOLET_DEEP);
    ctx.stroke_rrect(card, 22.0, theme::rgba(255, 255, 255, 0.16), 1.0);

    let ic_r = Rect::new(card.x + 16.0, card.y + 32.0, 38.0, 38.0);
    ctx.fill_rrect(ic_r, 12.0, theme::rgba(255, 255, 255, 0.22));
    ctx.draw_vector_icon(
        if state.weather.weather_code <= 1 { VectorIcon::Sun } else { VectorIcon::Cloud },
        ic_r.x + 19.0,
        ic_r.y + 19.0,
        18.0,
        if state.weather.weather_code <= 1 { theme::AMBER } else { theme::TEXT_PRIMARY },
    );

    ctx.draw_text("CLIMA EN VIVO", Rect::new(card.x + 64.0, card.y + 33.0, 120.0, 14.0), TextStyle::Small9Reg, theme::rgba(240, 235, 255, 0.85));
    ctx.draw_text(&state.weather.city, Rect::new(card.x + 64.0, card.y + 48.0, 140.0, 20.0), TextStyle::Body12Semi, theme::TEXT_PRIMARY);
    ctx.draw_text_right(&format!("{}°", state.weather.temp_c), Rect::new(card.x + card.w - 84.0, card.y + 28.0, 68.0, 44.0), TextStyle::HeroTemp34Bold, theme::TEXT_PRIMARY);

    let pw = (card.w - 44.0) / 3.0;
    let pills = [
        format!("Sens {}°", state.weather.feels_like_c),
        format!("Hum {}%", state.weather.humidity_pct),
        format!("{} km/h", state.weather.wind_kmh),
    ];
    for (i, p) in pills.into_iter().enumerate() {
        let pr = Rect::new(card.x + 14.0 + i as f32 * (pw + 8.0), card.y + card.h - 40.0, pw, 26.0);
        ctx.fill_rrect(pr, 9.0, theme::rgba(0, 0, 0, 0.22));
        ctx.draw_text_center(&p, pr, TextStyle::Small10Med, theme::TEXT_PRIMARY);
    }
}

// ==================== 4. SYSTEM MONITOR WIDGET ====================

fn render_monitor_widget(ctx: &mut DrawCtx<'_>, state: &AppState, w: f32, h: f32) {
    let card = Rect::new(4.0, 4.0, w - 8.0, h - 8.0);
    ctx.fill_card(card, 22.0, theme::rgba(16, 16, 24, 0.88), theme::BORDER_STRONG);

    ctx.draw_text("MONITOR", Rect::new(card.x + 66.0, card.y + 10.0, 76.0, 15.0), TextStyle::Small9Reg, theme::TEXT_TERTIARY);
    ctx.draw_text_right(
        &format!("CPU {}% · RAM {}%", state.cpu_pct.round() as i32, state.ram_pct),
        Rect::new(card.x + 132.0, card.y + 9.0, card.w - 148.0, 18.0),
        TextStyle::Mono11Med,
        theme::VIOLET_LIGHT,
    );

    ctx.draw_area_graph(
        Rect::new(card.x + 16.0, card.y + 38.0, card.w - 32.0, 58.0),
        &state.cpu_history,
        theme::VIOLET,
    );

    let ram_bar = Rect::new(card.x + 16.0, card.y + card.h - 24.0, card.w - 32.0, 7.0);
    ctx.fill_rrect(ram_bar, 3.5, theme::ELEVATED);
    ctx.fill_rrect(
        Rect::new(ram_bar.x, ram_bar.y, (state.ram_pct as f32 / 100.0).clamp(0.05, 1.0) * ram_bar.w, 7.0),
        3.5,
        if state.ram_pct > 80 { theme::CORAL } else { theme::EMERALD },
    );
}
