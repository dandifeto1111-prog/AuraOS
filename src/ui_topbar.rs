//! 34px Drool Dark Mode Top AppBar v3.1
//! - Unified Active-Window AuraOS Traffic-Light Quick Controls (Close / Min / Max for Edge & ALL Windows apps!)
//! - 4-Mode Segmented Switcher: PC | Dev | Gamer | Fondos (with smooth sliding Electric Violet pill)
//! - Micro Media Controls, Live Audio/Lang/Wifi status, Instant Search, Settings, Power Menu

use crate::models::ShellMode;
use crate::renderer::{DrawCtx, Rect, TextStyle};
use crate::theme::{self, VectorIcon};
use crate::ui_state::{Action, AppState, HitRegion};

pub const TOPBAR_HEIGHT: f32 = 34.0;

pub fn format_top_clock() -> String {
    use windows::Win32::System::SystemInformation::GetLocalTime;
    let st = unsafe { GetLocalTime() };
    let dow = match st.wDayOfWeek {
        0 => "do.",
        1 => "lu.",
        2 => "ma.",
        3 => "mi.",
        4 => "ju.",
        5 => "vi.",
        _ => "sá.",
    };
    let mon = match st.wMonth {
        1 => "ene.",
        2 => "feb.",
        3 => "mar.",
        4 => "abr.",
        5 => "may.",
        6 => "jun.",
        7 => "jul.",
        8 => "ago.",
        9 => "sep.",
        10 => "oct.",
        11 => "nov.",
        _ => "dic.",
    };
    format!("{} {} {}  {:02}:{:02}", dow, st.wDay, mon, st.wHour, st.wMinute)
}

pub fn render_topbar(ctx: &mut DrawCtx<'_>, state: &AppState, width: f32, hits: &mut Vec<HitRegion>) {
    hits.clear();
    let h = TOPBAR_HEIGHT;

    ctx.fill_rrect(Rect::new(0.0, 0.0, width, h), 0.0, theme::TOPBAR_BG);
    ctx.draw_line(0.0, h - 0.5, width, h - 0.5, theme::BORDER, 1.0);

    // ==================== LEFT CLUSTER ====================
    let mut lx = 10.0;

    // Actividades pill
    let act_rect = Rect::new(lx, 5.0, 114.0, 24.0);
    let act_hov = state.hovered_id.as_deref() == Some("top_act");
    let act_bg = if state.overview_open {
        theme::VIOLET_TINT
    } else if act_hov {
        theme::ELEVATED
    } else {
        theme::PILL_SUBTLE
    };
    ctx.fill_card(
        act_rect,
        11.0,
        act_bg,
        if state.overview_open { theme::VIOLET_BORDER } else { theme::BORDER },
    );
    let d1 = if state.mode == ShellMode::PC { theme::VIOLET } else { theme::rgba(255, 255, 255, 0.35) };
    let d2 = if state.mode == ShellMode::Dev { theme::VIOLET } else { theme::rgba(255, 255, 255, 0.35) };
    let d3 = if state.mode == ShellMode::Gamer || state.mode == ShellMode::Studio { theme::VIOLET } else { theme::rgba(255, 255, 255, 0.35) };
    ctx.fill_rrect(Rect::new(lx + 9.0, 14.5, if state.mode == ShellMode::PC { 12.0 } else { 5.0 }, 5.0), 2.5, d1);
    let dx2 = lx + if state.mode == ShellMode::PC { 24.0 } else { 17.0 };
    ctx.fill_rrect(Rect::new(dx2, 14.5, if state.mode == ShellMode::Dev { 12.0 } else { 5.0 }, 5.0), 2.5, d2);
    let dx3 = dx2 + if state.mode == ShellMode::Dev { 15.0 } else { 8.0 };
    ctx.fill_rrect(Rect::new(dx3, 14.5, if state.mode == ShellMode::Gamer || state.mode == ShellMode::Studio { 12.0 } else { 5.0 }, 5.0), 2.5, d3);
    ctx.draw_text("Actividades", Rect::new(lx + 44.0, 5.0, 66.0, 24.0), TextStyle::Body11Med, theme::TEXT_PRIMARY);
    hits.push(HitRegion {
        id: "top_act".into(),
        rect: act_rect,
        action: Action::ToggleOverview,
        right_action: None,
        tooltip: Some("Actividades y Buscador instantáneo (Alt+Space)".into()),
        slider: None,
    });
    lx += 124.0;

    ctx.draw_line(lx, 10.0, lx, 24.0, theme::BORDER, 1.0);
    lx += 10.0;

    // Unified AuraOS Active Window Traffic-Light Controls (with macOS hover glyphs × − +!)
    if state.active_user_hwnd != 0 {
        let lights = [
            ("top_aw_close", theme::CORAL, "×", Action::ActiveWinClose, "Cerrar ventana activa"),
            ("top_aw_min", theme::AMBER, "−", Action::ActiveWinMin, "Minimizar ventana activa"),
            ("top_aw_max", theme::EMERALD, "+", Action::ActiveWinMaxToggle, "Maximizar / Restaurar ventana"),
        ];
        let any_light_hov = matches!(
            state.hovered_id.as_deref(),
            Some("top_aw_close") | Some("top_aw_min") | Some("top_aw_max")
        );
        for (id, col, glyph, act, tip) in lights {
            ctx.fill_circle(lx + 6.0, 17.0, if any_light_hov { 5.8 } else { 5.0 }, col);
            if any_light_hov {
                ctx.draw_text_center(glyph, Rect::new(lx, 10.5, 12.0, 12.5), TextStyle::Small9Reg, theme::DARK_INK);
            }
            hits.push(HitRegion {
                id: id.into(),
                rect: Rect::new(lx - 1.0, 8.0, 14.0, 18.0),
                action: act,
                right_action: None,
                tooltip: Some(tip.into()),
                slider: None,
            });
            lx += 16.0;
        }
        lx += 6.0;
    }

    // Active window label
    let app_w = 112.0;
    ctx.draw_text(&state.active_window_label, Rect::new(lx, 5.0, app_w, 24.0), TextStyle::Body11Med, theme::TEXT_SECONDARY);
    lx += app_w + 6.0;

    // AuraFiles quick chip
    let files_rect = Rect::new(lx, 5.0, 78.0, 24.0);
    let files_hov = state.hovered_id.as_deref() == Some("top_files");
    ctx.fill_rrect(files_rect, 10.0, if files_hov { theme::ELEVATED } else { theme::rgba(0, 0, 0, 0.0) });
    ctx.draw_vector_icon(VectorIcon::Folder, lx + 14.0, 17.0, 12.0, theme::VIOLET_LIGHT);
    ctx.draw_text("Archivos", Rect::new(lx + 24.0, 5.0, 52.0, 24.0), TextStyle::Body11Med, theme::TEXT_SECONDARY);
    hits.push(HitRegion {
        id: "top_files".into(),
        rect: files_rect,
        action: Action::LaunchAuraFiles(None),
        right_action: None,
        tooltip: Some("Abrir AuraFiles (Alt+E)".into()),
        slider: None,
    });

    // Optional Automatic Update Pill (appears whenever a new local or GitHub Release is available!)
    if let Some(upd) = &state.pending_update {
        lx += 82.0;
        let lbl = format!("↑ v{}", upd.version);
        let uw = (lbl.len() as f32 * 6.8 + 18.0).max(66.0);
        let ur = Rect::new(lx, 6.0, uw, 22.0);
        let uhov = state.hovered_id.as_deref() == Some("top_upd");
        ctx.fill_card(
            ur,
            11.0,
            if uhov { theme::VIOLET_LIGHT } else { theme::VIOLET_TINT },
            theme::VIOLET_BORDER,
        );
        ctx.draw_text_center(&lbl, ur, TextStyle::Small10Bold, theme::TEXT_PRIMARY);
        hits.push(HitRegion {
            id: "top_upd".into(),
            rect: ur,
            action: Action::ApplyPendingUpdate,
            right_action: None,
            tooltip: Some(format!("Actualizar a AuraOS v{} ahora", upd.version)),
            slider: None,
        });
    }

    // ==================== CENTER CLUSTER (Clock + Weather) ====================
    let clock_str = format_top_clock();
    let center_str = if state.weather.loaded && !state.weather.city.is_empty() {
        format!("{}  ·  {}°C {}", clock_str, state.weather.temp_c, state.weather.city)
    } else {
        clock_str
    };
    let cw = 304.0;
    let cx = (width - cw) * 0.5;
    let center_rect = Rect::new(cx, 4.0, cw, 26.0);
    if state.hovered_id.as_deref() == Some("top_clock") {
        ctx.fill_rrect(center_rect, 11.0, theme::ELEVATED);
    }
    ctx.draw_text_center(&center_str, center_rect, TextStyle::Body12Semi, theme::TEXT_PRIMARY);
    hits.push(HitRegion {
        id: "top_clock".into(),
        rect: center_rect,
        action: Action::ToggleOverview,
        right_action: None,
        tooltip: Some("Abrir Vista de Actividades".into()),
        slider: None,
    });

    // ==================== RIGHT CLUSTER ====================
    let mut rx = width - 10.0;

    // 1. Power Button
    rx -= 26.0;
    let pwr_rect = Rect::new(rx, 5.0, 24.0, 24.0);
    let pwr_hov = state.hovered_id.as_deref() == Some("top_power");
    if state.power_open || pwr_hov {
        ctx.fill_rrect(pwr_rect, 12.0, if state.power_open { theme::CORAL_BG } else { theme::ELEVATED });
    }
    ctx.draw_vector_icon(
        VectorIcon::Power,
        pwr_rect.x + 12.0,
        pwr_rect.y + 12.0,
        12.5,
        if state.power_open || pwr_hov { theme::CORAL } else { theme::TEXT_SECONDARY },
    );
    hits.push(HitRegion {
        id: "top_power".into(),
        rect: pwr_rect,
        action: Action::TogglePowerMenu,
        right_action: None,
        tooltip: Some("Energía: Suspender, Reiniciar, Apagar".into()),
        slider: None,
    });

    // 2. Settings Button
    rx -= 26.0;
    let cfg_rect = Rect::new(rx, 5.0, 24.0, 24.0);
    if state.hovered_id.as_deref() == Some("top_cfg") {
        ctx.fill_rrect(cfg_rect, 12.0, theme::ELEVATED);
    }
    ctx.draw_vector_icon(VectorIcon::Settings, cfg_rect.x + 12.0, cfg_rect.y + 12.0, 12.5, theme::TEXT_SECONDARY);
    hits.push(HitRegion {
        id: "top_cfg".into(),
        rect: cfg_rect,
        action: Action::OpenSettings,
        right_action: None,
        tooltip: Some("Ajustes de AuraOS".into()),
        slider: None,
    });

    // 3. Search Button
    rx -= 26.0;
    let srch_rect = Rect::new(rx, 5.0, 24.0, 24.0);
    if state.hovered_id.as_deref() == Some("top_search") {
        ctx.fill_rrect(srch_rect, 12.0, theme::ELEVATED);
    }
    ctx.draw_vector_icon(VectorIcon::Search, srch_rect.x + 12.0, srch_rect.y + 12.0, 12.0, theme::TEXT_SECONDARY);
    hits.push(HitRegion {
        id: "top_search".into(),
        rect: srch_rect,
        action: Action::ToggleOverview,
        right_action: None,
        tooltip: Some("Buscador instantáneo (Alt+Space)".into()),
        slider: None,
    });

    // 4. Integrated System Tray Cluster (Language + Volume% + Wifi + Chevron)
    rx -= 8.0;
    let sys_w = 132.0;
    rx -= sys_w;
    let sys_rect = Rect::new(rx, 5.0, sys_w, 24.0);
    ctx.fill_card(
        sys_rect,
        11.0,
        if state.cc_open { theme::VIOLET_TINT } else { theme::PILL_SUBTLE },
        if state.cc_open { theme::VIOLET_BORDER } else { theme::BORDER },
    );

    let lang_rect = Rect::new(rx + 3.0, 6.0, 32.0, 22.0);
    if state.hovered_id.as_deref() == Some("top_lang") {
        ctx.fill_rrect(lang_rect, 8.0, theme::ELEVATED);
    }
    ctx.draw_text_center(&state.keyboard_lang, lang_rect, TextStyle::Small10Bold, theme::TEXT_SECONDARY);
    hits.push(HitRegion {
        id: "top_lang".into(),
        rect: lang_rect,
        action: Action::CycleKeyboardLayout,
        right_action: None,
        tooltip: Some("Cambiar idioma de teclado".into()),
        slider: None,
    });

    let cc_rect = Rect::new(rx + 36.0, 6.0, sys_w - 39.0, 22.0);
    if state.hovered_id.as_deref() == Some("top_cc") {
        ctx.fill_rrect(cc_rect, 8.0, theme::ELEVATED);
    }
    ctx.draw_vector_icon(
        if state.master_muted { VectorIcon::VolumeMute } else { VectorIcon::Volume },
        cc_rect.x + 11.0,
        17.0,
        11.5,
        if state.master_muted { theme::CORAL } else { theme::TEXT_PRIMARY },
    );
    let vol_lbl = if state.master_muted { "MUTE".into() } else { format!("{}%", state.master_vol) };
    ctx.draw_text(&vol_lbl, Rect::new(cc_rect.x + 21.0, 6.0, 38.0, 22.0), TextStyle::Mono11Med, theme::TEXT_SECONDARY);
    let net_kind = state.radios.net_kind;
    match net_kind {
        crate::radios::NetKind::Ethernet => {
            ctx.draw_vector_icon(VectorIcon::Ethernet, cc_rect.x + 67.0, 17.0, 12.0, theme::EMERALD);
        }
        crate::radios::NetKind::Wifi => {
            let bars = state.radios.wifi_bars.max(1);
            let active_col = if bars >= 2 { theme::TEXT_PRIMARY } else { theme::AMBER };
            ctx.draw_wifi_signal(
                cc_rect.x + 67.0,
                16.5,
                12.0,
                bars,
                active_col,
                theme::rgba(245, 245, 250, 0.20),
            );
        }
        crate::radios::NetKind::Offline => {
            ctx.draw_vector_icon(VectorIcon::NoInternet, cc_rect.x + 67.0, 17.0, 12.0, theme::CORAL);
        }
    }
    ctx.draw_vector_icon(VectorIcon::ChevronDown, cc_rect.x + 83.0, 17.0, 9.0, theme::TEXT_TERTIARY);
    let net_tip = match net_kind {
        crate::radios::NetKind::Ethernet => "Ethernet conectado · Centro de Control",
        crate::radios::NetKind::Wifi => "Wi-Fi conectado · Centro de Control",
        crate::radios::NetKind::Offline => "Sin conexión a Internet · Centro de Control",
    };
    hits.push(HitRegion {
        id: "top_cc".into(),
        rect: cc_rect,
        action: Action::ToggleControlCenter,
        right_action: Some(Action::ToggleMasterMute),
        tooltip: Some(net_tip.into()),
        slider: None,
    });

    // 5. Micro Media Controls Pill
    rx -= 8.0;
    let med_w = 78.0;
    rx -= med_w;
    let med_rect = Rect::new(rx, 5.0, med_w, 24.0);
    ctx.fill_card(med_rect, 11.0, theme::PILL_SUBTLE, theme::BORDER);
    let btns = [
        ("top_m_prev", VectorIcon::Prev, Action::MediaPrev, "Anterior"),
        ("top_m_play", VectorIcon::PlayPause, Action::MediaPlayPause, "Reproducir / Pausar"),
        ("top_m_next", VectorIcon::Next, Action::MediaNext, "Siguiente"),
    ];
    for (idx, (id, ic, act, tip)) in btns.into_iter().enumerate() {
        let br = Rect::new(rx + 4.0 + idx as f32 * 24.0, 6.0, 22.0, 22.0);
        if state.hovered_id.as_deref() == Some(id) {
            ctx.fill_rrect(br, 9.0, theme::ELEVATED);
        }
        let col = if idx == 1 && state.now_playing.is_playing { theme::VIOLET_LIGHT } else { theme::TEXT_SECONDARY };
        ctx.draw_vector_icon(ic, br.x + 11.0, br.y + 11.0, 10.5, col);
        hits.push(HitRegion {
            id: id.into(),
            rect: br,
            action: act,
            right_action: None,
            tooltip: Some(tip.into()),
            slider: None,
        });
    }

    // 6. Compact Interactive Live CPU & RAM Telemetry -> Opens AuraTasks.exe!
    if width >= 1400.0 {
        rx -= 8.0;
        let stat_w = 104.0;
        rx -= stat_w;
        let stat_rect = Rect::new(rx, 5.0, stat_w, 24.0);
        if state.hovered_id.as_deref() == Some("top_tasks") {
            ctx.fill_card(stat_rect, 10.0, theme::ELEVATED, theme::VIOLET_BORDER);
        }
        let stat_txt = format!("CPU {:>2}% · {:.1}G", state.cpu_pct.round() as i32, state.ram_used_gb);
        ctx.draw_text_center(&stat_txt, stat_rect, TextStyle::Mono11Med, if state.hovered_id.as_deref() == Some("top_tasks") { theme::VIOLET_LIGHT } else { theme::TEXT_TERTIARY });
        hits.push(HitRegion {
            id: "top_tasks".into(),
            rect: stat_rect,
            action: Action::LaunchAuraTasks,
            right_action: None,
            tooltip: Some("Abrir AuraTasks · Administrador de Tareas (Ctrl+Shift+Esc)".into()),
            slider: None,
        });
    }

    // 7. 4-Mode Segmented Switcher (PC | Dev | Gamer | Fondos) — TRUE TOGGLE!
    rx -= 10.0;
    let mode_w = 198.0;
    rx -= mode_w;
    let mode_box = Rect::new(rx, 5.0, mode_w, 24.0);
    ctx.fill_card(mode_box, 11.0, theme::PILL_SUBTLE, theme::BORDER);

    let seg_w = (mode_w - 6.0) / 4.0;
    let pill_x = rx + 3.0 + state.overview_tab_pill_x.value.clamp(0.0, 3.0) * seg_w;
    ctx.fill_rrect(
        Rect::new(pill_x, 7.0, seg_w, 20.0),
        9.0,
        if state.overview_open { theme::VIOLET } else { theme::ELEVATED_HOVER },
    );

    let modes = [
        ("top_mode_pc", "PC", ShellMode::PC),
        ("top_mode_dev", "Dev", ShellMode::Dev),
        ("top_mode_gamer", "Gamer", ShellMode::Gamer),
        ("top_mode_studio", "Fondos", ShellMode::Studio),
    ];
    for (i, (id, lbl, m)) in modes.into_iter().enumerate() {
        let mr = Rect::new(rx + 3.0 + i as f32 * seg_w, 6.0, seg_w, 22.0);
        let active = state.mode == m;
        ctx.draw_text_center(
            lbl,
            mr,
            if active { TextStyle::Small10Bold } else { TextStyle::Small10Med },
            if active { theme::TEXT_PRIMARY } else { theme::TEXT_SECONDARY },
        );
        hits.push(HitRegion {
            id: id.into(),
            rect: mr,
            action: Action::ClickModeTab(m),
            right_action: None,
            tooltip: Some(format!("Cambiar a vista {}", lbl)),
            slider: None,
        });
    }
}
