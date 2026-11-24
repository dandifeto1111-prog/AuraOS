//! AuraOS v5.1 Top AppBar — Material You 3 + Float/Hug/Rect Styles + Laptop Battery!
//! - Supports `Hug` (Material You tinted clusters), `Float` (floating rounded pill bar), and `Rect`!
//! - Automatically displays a real Laptop Battery `%` pill via `GetSystemPowerStatus` when a battery exists.
//! - Unified Active-Window macOS Traffic Lights (Close / Genie Min / Max), 4-Mode Switcher, Media, Clock+Pomodoro!

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
    let p = theme::pal();
    let is_float = state.settings.bar_style.eq_ignore_ascii_case("Float");
    let is_rect = state.settings.bar_style.eq_ignore_ascii_case("Rect");

    if is_float {
        let bar_r = Rect::new(8.0, 2.0, width - 16.0, h - 4.0);
        ctx.fill_card(bar_r, 14.0, theme::with_alpha(p.surface, 0.94), p.border_strong);
    } else if is_rect {
        ctx.fill_rrect(Rect::new(0.0, 0.0, width, h), 0.0, p.canvas);
        ctx.draw_line(0.0, h - 0.5, width, h - 0.5, p.border, 1.0);
    } else {
        // Default "Hug" illogical-impulse Material You tinted bar
        ctx.fill_rrect(Rect::new(0.0, 0.0, width, h), 0.0, p.topbar_bg);
        ctx.draw_line(0.0, h - 0.5, width, h - 0.5, p.border, 1.0);
    }

    // ==================== LEFT CLUSTER ====================
    let mut lx = if is_float { 16.0 } else { 10.0 };

    // Actividades pill
    let act_rect = Rect::new(lx, 5.0, 114.0, 24.0);
    let act_hov = state.hovered_id.as_deref() == Some("top_act");
    let act_bg = if state.overview_open {
        p.accent_tint
    } else if act_hov {
        p.elevated
    } else {
        p.pill_subtle
    };
    ctx.fill_card(
        act_rect,
        11.0,
        act_bg,
        if state.overview_open { p.accent_border } else { p.border },
    );
    let d1 = if state.mode == ShellMode::PC { p.accent } else { theme::rgba(255, 255, 255, 0.35) };
    let d2 = if state.mode == ShellMode::Dev { p.accent } else { theme::rgba(255, 255, 255, 0.35) };
    let d3 = if matches!(state.mode, ShellMode::Gamer | ShellMode::Creative | ShellMode::Studio) { p.accent } else { theme::rgba(255, 255, 255, 0.35) };
    ctx.fill_rrect(Rect::new(lx + 9.0, 14.5, if state.mode == ShellMode::PC { 12.0 } else { 5.0 }, 5.0), 2.5, d1);
    let dx2 = lx + if state.mode == ShellMode::PC { 24.0 } else { 17.0 };
    ctx.fill_rrect(Rect::new(dx2, 14.5, if state.mode == ShellMode::Dev { 12.0 } else { 5.0 }, 5.0), 2.5, d2);
    let dx3 = dx2 + if state.mode == ShellMode::Dev { 15.0 } else { 8.0 };
    ctx.fill_rrect(Rect::new(dx3, 14.5, if matches!(state.mode, ShellMode::Gamer | ShellMode::Creative | ShellMode::Studio) { 12.0 } else { 5.0 }, 5.0), 2.5, d3);
    ctx.draw_text("Actividades", Rect::new(lx + 44.0, 5.0, 66.0, 24.0), TextStyle::Body11Med, p.text_primary);
    hits.push(HitRegion {
        id: "top_act".into(),
        rect: act_rect,
        action: Action::ToggleOverview,
        right_action: None,
        tooltip: Some("Actividades y Buscador instantáneo (Alt+Space)".into()),
        slider: None,
    });
    lx += 124.0;

    ctx.draw_line(lx, 10.0, lx, 24.0, p.border, 1.0);
    lx += 10.0;

    // Unified AuraOS Active Window Traffic-Light Controls (× − + on hover!)
    if state.active_user_hwnd != 0 {
        let lights = [
            ("top_aw_close", theme::CORAL, "×", Action::ActiveWinClose, "Cerrar ventana activa"),
            ("top_aw_min", theme::AMBER, "−", Action::ActiveWinMin, "Minimizar ventana activa (Genie)"),
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
    let app_w = 110.0;
    ctx.draw_text(&state.active_window_label, Rect::new(lx, 5.0, app_w, 24.0), TextStyle::Body11Med, p.text_secondary);
    lx += app_w + 6.0;

    // AuraFiles quick chip
    let files_rect = Rect::new(lx, 5.0, 78.0, 24.0);
    let files_hov = state.hovered_id.as_deref() == Some("top_files");
    ctx.fill_rrect(files_rect, 10.0, if files_hov { p.elevated } else { theme::rgba(0, 0, 0, 0.0) });
    ctx.draw_vector_icon(VectorIcon::Folder, lx + 14.0, 17.0, 12.0, p.accent_light);
    ctx.draw_text("Archivos", Rect::new(lx + 24.0, 5.0, 52.0, 24.0), TextStyle::Body11Med, p.text_secondary);
    hits.push(HitRegion {
        id: "top_files".into(),
        rect: files_rect,
        action: Action::LaunchAuraFiles(None),
        right_action: None,
        tooltip: Some("Abrir AuraFiles (Alt+E)".into()),
        slider: None,
    });

    // Optional Automatic Update Pill
    if let Some(upd) = &state.pending_update {
        lx += 82.0;
        let lbl = format!("↑ v{}", upd.version);
        let uw = (lbl.len() as f32 * 6.8 + 18.0).max(66.0);
        let ur = Rect::new(lx, 6.0, uw, 22.0);
        let uhov = state.hovered_id.as_deref() == Some("top_upd");
        ctx.fill_card(
            ur,
            11.0,
            if uhov { p.accent_light } else { p.accent_tint },
            p.accent_border,
        );
        ctx.draw_text_center(&lbl, ur, TextStyle::Small10Bold, p.text_primary);
        hits.push(HitRegion {
            id: "top_upd".into(),
            rect: ur,
            action: Action::ApplyPendingUpdate,
            right_action: None,
            tooltip: Some(format!("Actualizar a AuraOS v{} ahora", upd.version)),
            slider: None,
        });
    }

    // ==================== RIGHT CLUSTER (Compute width first so Center Clock NEVER overlaps!) ====================
    let has_bat = state.battery_status.is_some();
    let has_tasks = width >= 1400.0;
    let right_margin = if is_float { 16.0 } else { 10.0 };
    let right_cluster_total = right_margin
        + 26.0 * 3.0 // power, cfg, search
        + if has_bat { 64.0 } else { 0.0 }
        + (8.0 + 132.0) // sys tray
        + (8.0 + 78.0)  // media
        + if has_tasks { 8.0 + 104.0 } else { 0.0 }
        + (10.0 + 246.0); // 5-mode switcher (PC | Dev | Gamer | Studio | Fondos)
    let right_start_x = width - right_cluster_total;

    // ==================== CENTER CLUSTER (Clock + Weather + Pomodoro -> AuraCalendar!) ====================
    let clock_str = format_top_clock();
    let pomo_suffix = if state.pomodoro_active {
        format!(" · {:02}:{:02}", state.pomodoro_secs_left / 60, state.pomodoro_secs_left % 60)
    } else {
        String::new()
    };
    let left_end_x = lx + if state.pending_update.is_some() { 94.0 } else { 86.0 };
    let avail_mid = (right_start_x - left_end_x - 16.0).max(160.0);
    let center_str = if avail_mid >= 325.0 && state.weather.loaded && !state.weather.city.is_empty() {
        format!("{} · {}°C {}{}", clock_str, state.weather.temp_c, state.weather.city, pomo_suffix)
    } else if avail_mid >= 230.0 && state.weather.loaded {
        format!("{} · {}°C{}", clock_str, state.weather.temp_c, pomo_suffix)
    } else {
        format!("{}{}", clock_str, pomo_suffix)
    };
    let desired_cw = (center_str.chars().count() as f32 * 7.2 + 26.0).clamp(160.0, 340.0);
    let cw = desired_cw.min(avail_mid);
    let ideal_cx = (width - cw) * 0.5;
    let cx = ideal_cx.clamp(left_end_x + 8.0, (right_start_x - cw - 8.0).max(left_end_x + 8.0));
    let center_rect = Rect::new(cx, 4.0, cw, 26.0);
    if state.calendar_open {
        ctx.fill_card(center_rect, 11.0, p.accent_tint, p.accent_border);
    } else if state.hovered_id.as_deref() == Some("top_clock") {
        ctx.fill_rrect(center_rect, 11.0, p.elevated);
    } else if !is_rect {
        ctx.fill_card(center_rect, 11.0, theme::with_alpha(p.pill_subtle, 0.72), p.border);
    }
    ctx.draw_text_center(&center_str, center_rect, TextStyle::Body12Semi, p.text_primary);
    hits.push(HitRegion {
        id: "top_clock".into(),
        rect: center_rect,
        action: Action::ToggleCalendar,
        right_action: Some(Action::ToggleOverview),
        tooltip: Some("Calendario Mensual y Temporizador Pomodoro".into()),
        slider: None,
    });

    // ==================== RIGHT CLUSTER ====================
    let mut rx = width - right_margin;

    // 1. Power Button
    rx -= 26.0;
    let pwr_rect = Rect::new(rx, 5.0, 24.0, 24.0);
    let pwr_hov = state.hovered_id.as_deref() == Some("top_power");
    if state.power_open || pwr_hov {
        ctx.fill_rrect(pwr_rect, 12.0, if state.power_open { theme::CORAL_BG } else { p.elevated });
    }
    ctx.draw_vector_icon(
        VectorIcon::Power,
        pwr_rect.x + 12.0,
        pwr_rect.y + 12.0,
        12.5,
        if state.power_open || pwr_hov { theme::CORAL } else { p.text_secondary },
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
        ctx.fill_rrect(cfg_rect, 12.0, p.elevated);
    }
    ctx.draw_vector_icon(VectorIcon::Settings, cfg_rect.x + 12.0, cfg_rect.y + 12.0, 12.5, p.text_secondary);
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
        ctx.fill_rrect(srch_rect, 12.0, p.elevated);
    }
    ctx.draw_vector_icon(VectorIcon::Search, srch_rect.x + 12.0, srch_rect.y + 12.0, 12.0, p.text_secondary);
    hits.push(HitRegion {
        id: "top_search".into(),
        rect: srch_rect,
        action: Action::ToggleOverview,
        right_action: None,
        tooltip: Some("Buscador instantáneo (Alt+Space)".into()),
        slider: None,
    });

    // 4. Real Laptop Battery Indicator Pill (auto-shown whenever hardware battery is present!)
    if let Some((pct, charging)) = state.battery_status {
        rx -= 6.0;
        let bat_w = 58.0;
        rx -= bat_w;
        let bat_r = Rect::new(rx, 5.0, bat_w, 24.0);
        let bat_col = if charging {
            theme::EMERALD
        } else if pct <= 20 {
            theme::CORAL
        } else if pct <= 45 {
            theme::AMBER
        } else {
            p.text_primary
        };
        ctx.fill_card(bat_r, 11.0, p.pill_subtle, p.border);
        ctx.draw_vector_icon(VectorIcon::Battery, bat_r.x + 14.0, 17.0, 12.0, bat_col);
        let b_txt = if charging { format!("{}%+", pct) } else { format!("{}%", pct) };
        ctx.draw_text(&b_txt, Rect::new(bat_r.x + 24.0, 5.0, 32.0, 24.0), TextStyle::Small10Bold, bat_col);
        hits.push(HitRegion {
            id: "top_bat".into(),
            rect: bat_r,
            action: Action::ToggleControlCenter,
            right_action: None,
            tooltip: Some(if charging {
                format!("Batería {}% (Cargando)", pct)
            } else {
                format!("Batería {}%", pct)
            }),
            slider: None,
        });
    }

    // 5. Integrated System Tray Cluster (Language + Volume% + Wifi + Chevron)
    rx -= 8.0;
    let sys_w = 132.0;
    rx -= sys_w;
    let sys_rect = Rect::new(rx, 5.0, sys_w, 24.0);
    ctx.fill_card(
        sys_rect,
        11.0,
        if state.cc_open { p.accent_tint } else { p.pill_subtle },
        if state.cc_open { p.accent_border } else { p.border },
    );

    let lang_rect = Rect::new(rx + 3.0, 6.0, 32.0, 22.0);
    if state.hovered_id.as_deref() == Some("top_lang") {
        ctx.fill_rrect(lang_rect, 8.0, p.elevated);
    }
    ctx.draw_text_center(&state.keyboard_lang, lang_rect, TextStyle::Small10Bold, p.text_secondary);
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
        ctx.fill_rrect(cc_rect, 8.0, p.elevated);
    }
    ctx.draw_vector_icon(
        if state.master_muted { VectorIcon::VolumeMute } else { VectorIcon::Volume },
        cc_rect.x + 11.0,
        17.0,
        11.5,
        if state.master_muted { theme::CORAL } else { p.text_primary },
    );
    let vol_lbl = if state.master_muted { "MUTE".into() } else { format!("{}%", state.master_vol) };
    ctx.draw_text(&vol_lbl, Rect::new(cc_rect.x + 21.0, 6.0, 38.0, 22.0), TextStyle::Mono11Med, p.text_secondary);
    let net_kind = state.radios.net_kind;
    match net_kind {
        crate::radios::NetKind::Ethernet => {
            ctx.draw_vector_icon(VectorIcon::Ethernet, cc_rect.x + 67.0, 17.0, 12.0, theme::EMERALD);
        }
        crate::radios::NetKind::Wifi => {
            let bars = state.radios.wifi_bars.max(1);
            let active_col = if bars >= 2 { p.text_primary } else { theme::AMBER };
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
    ctx.draw_vector_icon(VectorIcon::ChevronDown, cc_rect.x + 83.0, 17.0, 9.0, p.text_tertiary);
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

    // 6. Micro Media Controls Pill
    rx -= 8.0;
    let med_w = 78.0;
    rx -= med_w;
    let med_rect = Rect::new(rx, 5.0, med_w, 24.0);
    ctx.fill_card(med_rect, 11.0, p.pill_subtle, p.border);
    let btns = [
        ("top_m_prev", VectorIcon::Prev, Action::MediaPrev, "Anterior"),
        ("top_m_play", VectorIcon::PlayPause, Action::MediaPlayPause, "Reproducir / Pausar"),
        ("top_m_next", VectorIcon::Next, Action::MediaNext, "Siguiente"),
    ];
    for (idx, (id, ic, act, tip)) in btns.into_iter().enumerate() {
        let br = Rect::new(rx + 4.0 + idx as f32 * 24.0, 6.0, 22.0, 22.0);
        if state.hovered_id.as_deref() == Some(id) {
            ctx.fill_rrect(br, 9.0, p.elevated);
        }
        let col = if idx == 1 && state.now_playing.is_playing { p.accent_light } else { p.text_secondary };
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

    // 7. Compact Interactive Live CPU & RAM Telemetry -> Opens AuraTasks.exe!
    if width >= 1400.0 {
        rx -= 8.0;
        let stat_w = 104.0;
        rx -= stat_w;
        let stat_rect = Rect::new(rx, 5.0, stat_w, 24.0);
        if state.hovered_id.as_deref() == Some("top_tasks") {
            ctx.fill_card(stat_rect, 10.0, p.elevated, p.accent_border);
        }
        let stat_txt = format!("CPU {:>2}% · {:.1}G", state.cpu_pct.round() as i32, state.ram_used_gb);
        ctx.draw_text_center(
            &stat_txt,
            stat_rect,
            TextStyle::Mono11Med,
            if state.hovered_id.as_deref() == Some("top_tasks") { p.accent_light } else { p.text_tertiary },
        );
        hits.push(HitRegion {
            id: "top_tasks".into(),
            rect: stat_rect,
            action: Action::LaunchAuraTasks,
            right_action: None,
            tooltip: Some("Abrir AuraTasks · Administrador de Tareas (Ctrl+Shift+Esc)".into()),
            slider: None,
        });
    }

    // 8. 5-Mode Segmented Switcher (PC | Dev | Gamer | Studio | Fondos)
    rx -= 10.0;
    let mode_w = 246.0;
    rx -= mode_w;
    let mode_box = Rect::new(rx, 5.0, mode_w, 24.0);
    ctx.fill_card(mode_box, 11.0, p.pill_subtle, p.border);

    let seg_w = (mode_w - 6.0) / 5.0;
    let pill_x = rx + 3.0 + state.overview_tab_pill_x.value.clamp(0.0, 4.0) * seg_w;
    ctx.fill_rrect(
        Rect::new(pill_x, 7.0, seg_w, 20.0),
        9.0,
        if state.overview_open { p.accent } else { p.elevated_hover },
    );

    let modes = [
        ("top_mode_pc", "PC", ShellMode::PC),
        ("top_mode_dev", "Dev", ShellMode::Dev),
        ("top_mode_gamer", "Gamer", ShellMode::Gamer),
        ("top_mode_creative", "Studio", ShellMode::Creative),
        ("top_mode_studio", "Fondos", ShellMode::Studio),
    ];
    for (i, (id, lbl, m)) in modes.into_iter().enumerate() {
        let mr = Rect::new(rx + 3.0 + i as f32 * seg_w, 6.0, seg_w, 22.0);
        let active = state.mode == m;
        ctx.draw_text_center(
            lbl,
            mr,
            if active { TextStyle::Small10Bold } else { TextStyle::Small10Med },
            if active { p.text_primary } else { p.text_secondary },
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
