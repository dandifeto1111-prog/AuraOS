//! AuraOS v5.1 Unified *illogical-impulse* Material You Right Sidebar & Control Center
//! - Material You adaptive palette + live system uptime pill
//! - Hero SMTC Media Player with Material 3 Animated Wavy/Squiggly Progress Bar!
//! - Master Volume + Display Brightness Slider (`SetDeviceGammaRamp`) + Inline Audio Switcher
//! - 2x3 Material 3 Quick Toggles: Red, Bluetooth, Luz Nocturna (3800K), Café (Keep Awake), Teclado, AuraShot/Clip
//! - Bottom Tabbed Productivity Dock: [ To-Do ] [ Pomodoro ] [ Apps 2º Plano ]!

use crate::renderer::{DrawCtx, Rect, TextStyle};
use crate::theme::{self, VectorIcon};
use crate::ui_state::{Action, AppState, HitRegion, SliderDragKind};

pub const CC_WIDTH: f32 = 424.0;
pub const CC_MAX_H: f32 = 860.0;

pub fn render_control_center(ctx: &mut DrawCtx<'_>, state: &AppState, win_h: f32, hits: &mut Vec<HitRegion>) -> f32 {
    hits.clear();
    let t = state.cc_anim.value.clamp(0.0, 1.0);
    if t <= 0.01 {
        return 0.0;
    }

    let p = theme::pal();
    let ox = CC_WIDTH - 66.0;
    let oy = 14.0;
    let max_r = (CC_WIDTH * CC_WIDTH + win_h * win_h).sqrt() + 24.0;
    let cur_r = 22.0 + (max_r - 22.0) * t;
    let layer_alpha = (t * 2.2).clamp(0.0, 1.0);

    let full_r = Rect::new(0.0, 0.0, CC_WIDTH, win_h);
    let _layer = if t < 0.96 {
        ctx.push_circle_mask_layer(full_r, ox, oy, cur_r, layer_alpha)
    } else {
        None
    };

    let scale = 0.95 + 0.05 * t;
    let dy = (1.0 - t) * -6.0;
    ctx.set_transform_scale_origin(scale, ox, oy, 0.0, dy);

    let outer = Rect::new(6.0, 6.0, CC_WIDTH - 12.0, win_h - 12.0);
    ctx.fill_card(outer, 22.0, p.surface, p.border_strong);

    let content_rect = Rect::new(20.0, 18.0, CC_WIDTH - 46.0, win_h - 34.0);
    ctx.push_clip(content_rect);

    let lx = 22.0;
    let cw = CC_WIDTH - 50.0;
    let scroll = state.cc_scroll.max(0.0);
    let mut y = 20.0 - scroll;

    // 1. HEADER WITH MACOS TRAFFIC LIGHTS + UPTIME PILL + SETTINGS
    let cc_lights = [
        ("cc_tl_close", lx + 6.0, theme::CORAL, "×", Action::CloseAllOverlays),
        ("cc_tl_min", lx + 23.0, theme::AMBER, "−", Action::CloseAllOverlays),
        ("cc_tl_cfg", lx + 40.0, theme::EMERALD, "+", Action::LaunchAuraSettings(None)),
    ];
    let cc_tl_hov = matches!(
        state.hovered_id.as_deref(),
        Some("cc_tl_close") | Some("cc_tl_min") | Some("cc_tl_cfg")
    );
    for (id, cx, col, glyph, act) in cc_lights {
        ctx.fill_circle(cx, y + 11.0, if cc_tl_hov { 5.8 } else { 5.2 }, col);
        if cc_tl_hov {
            ctx.draw_text_center(glyph, Rect::new(cx - 5.5, y + 4.5, 11.0, 12.0), TextStyle::Small9Reg, theme::DARK_INK);
        }
        hits.push(HitRegion { id: id.into(), rect: Rect::new(cx - 8.0, y + 2.0, 16.0, 18.0), action: act, right_action: None, tooltip: None, slider: None });
    }
    ctx.draw_text("Centro AuraOS", Rect::new(lx + 56.0, y, 120.0, 22.0), TextStyle::Title14Semi, p.text_primary);

    // Uptime badge (illogical-impulse touch!)
    let up_r = Rect::new(lx + 178.0, y + 2.0, 96.0, 18.0);
    ctx.fill_rrect(up_r, 9.0, p.pill_subtle);
    ctx.draw_text_center(&state.uptime_label, up_r, TextStyle::Small9Reg, p.text_tertiary);

    let aura_cfg_r = Rect::new(lx + cw - 82.0, y, 82.0, 22.0);
    ctx.draw_text_right("Ajustes", aura_cfg_r, TextStyle::Body11Med, p.accent_light);
    hits.push(HitRegion {
        id: "cc_wincfg".into(),
        rect: aura_cfg_r,
        action: Action::LaunchAuraSettings(None),
        right_action: None,
        tooltip: Some("Abrir Configuración nativa de AuraOS".into()),
        slider: None,
    });
    y += 32.0;

    // 2. MATERIAL YOU HERO MEDIA PLAYER + ANIMATED WAVY PROGRESS BAR!
    let m_card = Rect::new(lx, y, cw, 78.0);
    ctx.fill_gradient_rrect(m_card, 18.0, p.accent_light, p.accent_deep);
    let m_ic = Rect::new(lx + 12.0, y + 12.0, 38.0, 38.0);
    ctx.fill_rrect(m_ic, 12.0, theme::rgba(255, 255, 255, 0.22));
    ctx.draw_vector_icon(VectorIcon::MusicNote, m_ic.x + 19.0, m_ic.y + 19.0, 17.0, theme::CRISP_WHITE);

    let sub_line = if !state.now_playing.artist.is_empty() {
        format!("{} · {}", state.now_playing.source_app, state.now_playing.artist)
    } else {
        state.now_playing.source_app.clone()
    };
    ctx.draw_text(&sub_line, Rect::new(lx + 60.0, y + 11.0, cw - 166.0, 15.0), TextStyle::Small10Med, theme::rgba(248, 248, 255, 0.88));
    ctx.draw_text(&state.now_playing.title, Rect::new(lx + 60.0, y + 27.0, cw - 166.0, 20.0), TextStyle::Body12Semi, theme::CRISP_WHITE);

    let m_btns = [
        ("cc_m_prev", VectorIcon::Prev, false, Action::MediaPrev),
        ("cc_m_play", VectorIcon::PlayPause, true, Action::MediaPlayPause),
        ("cc_m_next", VectorIcon::Next, false, Action::MediaNext),
    ];
    let mut mbx = lx + cw - 102.0;
    for (id, ic, hero, act) in m_btns {
        let sz = if hero { 32.0 } else { 26.0 };
        let by = y + (58.0 - sz) * 0.5;
        let br = Rect::new(mbx, by, sz, sz);
        let bg = if hero { theme::CRISP_WHITE } else { theme::rgba(0, 0, 0, 0.24) };
        ctx.fill_rrect(br, sz * 0.5, bg);
        ctx.draw_vector_icon(ic, br.x + sz * 0.5, br.y + sz * 0.5, if hero { 13.0 } else { 11.0 }, if hero { theme::DARK_INK } else { theme::CRISP_WHITE });
        hits.push(HitRegion { id: id.into(), rect: br, action: act, right_action: None, tooltip: None, slider: None });
        mbx += sz + 5.0;
    }

    // Real SMTC-Synced Material 3 Expressive Wavy Timeline Bar + Live MM:SS + Click/Drag-to-Seek!
    let has_tl = state.now_playing.has_timeline;
    let wave_prog = state.now_playing.live_progress();
    let (bar_x, bar_w) = if has_tl {
        let pos_txt = crate::models::NowPlaying::format_time(state.now_playing.live_position_sec());
        let dur_txt = crate::models::NowPlaying::format_time(state.now_playing.duration_sec);
        ctx.draw_text(&pos_txt, Rect::new(lx + 12.0, y + 57.0, 42.0, 14.0), TextStyle::Small9Reg, theme::rgba(255, 255, 255, 0.90));
        ctx.draw_text_right(&dur_txt, Rect::new(lx + cw - 54.0, y + 57.0, 42.0, 14.0), TextStyle::Small9Reg, theme::rgba(255, 255, 255, 0.76));
        (lx + 56.0, (cw - 112.0).max(60.0))
    } else {
        (lx + 14.0, cw - 28.0)
    };
    let bar_r = Rect::new(bar_x, y + 57.0, bar_w, 14.0);
    ctx.draw_wavy_progress_bar(
        bar_r,
        wave_prog,
        state.music_wave_phase,
        state.now_playing.is_playing && has_tl,
        theme::CRISP_WHITE,
        theme::rgba(255, 255, 255, 0.28),
    );
    if has_tl {
        hits.push(HitRegion {
            id: "cc_m_seek".into(),
            rect: Rect::new(bar_r.x - 4.0, bar_r.y - 4.0, bar_r.w + 8.0, bar_r.h + 8.0),
            action: Action::MediaSeek(wave_prog),
            right_action: None,
            tooltip: Some("Saltar en la línea de tiempo".into()),
            slider: Some(SliderDragKind::MediaSeek { x: bar_r.x, w: bar_r.w }),
        });
    }
    y += 88.0;

    // 3. VOLUME + REAL DISPLAY BRIGHTNESS + INLINE AUDIO SWITCHER CARD
    let devices = if state.cc_showing_mics { &state.mic_devices } else { &state.output_devices };
    let dev_rows = devices.len().max(1).min(3) as f32;
    let vol_card_h = 162.0 + dev_rows * 42.0;
    let vol_card = Rect::new(lx, y, cw, vol_card_h);
    ctx.fill_card(vol_card, 16.0, p.elevated, p.border);

    let mute_r = Rect::new(lx + 12.0, y + 11.0, 28.0, 28.0);
    ctx.fill_rrect(mute_r, 14.0, if state.master_muted { theme::CORAL } else { p.accent });
    ctx.draw_vector_icon(
        if state.master_muted { VectorIcon::VolumeMute } else { VectorIcon::Volume },
        mute_r.x + 14.0,
        mute_r.y + 14.0,
        13.0,
        theme::CRISP_WHITE,
    );
    hits.push(HitRegion {
        id: "cc_mute".into(),
        rect: mute_r,
        action: Action::ToggleMasterMute,
        right_action: None,
        tooltip: Some("Silenciar / Activar volumen maestro".into()),
        slider: None,
    });

    let active_out_name = state
        .output_devices
        .iter()
        .find(|d| d.is_default)
        .map(|d| d.short_name.as_str())
        .unwrap_or("Salida predeterminada");
    ctx.draw_text("Volumen", Rect::new(lx + 48.0, y + 9.0, 80.0, 16.0), TextStyle::Body12Semi, p.text_primary);
    ctx.draw_text(active_out_name, Rect::new(lx + 48.0, y + 25.0, cw - 120.0, 13.0), TextStyle::Small9Reg, p.text_tertiary);
    let vol_txt = if state.master_muted { "MUTE".into() } else { format!("{}%", state.master_vol) };
    ctx.draw_text_right(&vol_txt, Rect::new(lx + cw - 66.0, y + 12.0, 54.0, 20.0), TextStyle::Mono12Bold, p.text_primary);

    // Master Volume Slider
    let tr = Rect::new(lx + 12.0, y + 44.0, cw - 24.0, 20.0);
    ctx.fill_hatched_track(tr, 10.0);
    let frac = if state.master_muted { 0.0 } else { (state.master_vol as f32 / 100.0).clamp(0.0, 1.0) };
    let fill_w = (frac * tr.w).max(20.0);
    ctx.fill_rrect(Rect::new(tr.x, tr.y, fill_w, 20.0), 10.0, if state.master_muted { theme::CORAL } else { p.accent });
    let knob_x = (tr.x + frac * (tr.w - 20.0) + 10.0).clamp(tr.x + 10.0, tr.x + tr.w - 10.0);
    ctx.fill_circle(knob_x, tr.y + 10.0, 9.0, theme::CRISP_WHITE);
    ctx.stroke_circle(knob_x, tr.y + 10.0, 9.0, p.accent, 2.0);
    hits.push(HitRegion {
        id: "cc_vol_slider".into(),
        rect: tr,
        action: Action::SetMasterVolume(state.master_vol),
        right_action: None,
        tooltip: None,
        slider: Some(SliderDragKind::MasterVolume { x: tr.x, w: tr.w }),
    });

    // Real Display Brightness Slider (35% .. 100% via SetDeviceGammaRamp!)
    let br_y = y + 74.0;
    ctx.draw_vector_icon(VectorIcon::Sun, lx + 20.0, br_y + 10.0, 13.0, theme::AMBER);
    ctx.draw_text("Brillo", Rect::new(lx + 34.0, br_y + 1.0, 56.0, 18.0), TextStyle::Body11Med, p.text_secondary);
    let br_tr = Rect::new(lx + 84.0, br_y + 2.0, cw - 142.0, 16.0);
    ctx.fill_rrect(br_tr, 8.0, p.card_bg);
    let b_frac = ((state.settings.brightness_pct.clamp(35, 100) - 35) as f32 / 65.0).clamp(0.0, 1.0);
    let bf_w = (b_frac * br_tr.w).max(16.0);
    ctx.fill_rrect(Rect::new(br_tr.x, br_tr.y, bf_w, 16.0), 8.0, theme::AMBER);
    let b_knob = (br_tr.x + b_frac * (br_tr.w - 16.0) + 8.0).clamp(br_tr.x + 8.0, br_tr.x + br_tr.w - 8.0);
    ctx.fill_circle(b_knob, br_tr.y + 8.0, 7.5, theme::CRISP_WHITE);
    ctx.draw_text_right(
        &format!("{}%", state.settings.brightness_pct),
        Rect::new(lx + cw - 52.0, br_y + 1.0, 40.0, 18.0),
        TextStyle::Mono11Med,
        p.text_primary,
    );
    hits.push(HitRegion {
        id: "cc_bright_slider".into(),
        rect: Rect::new(br_tr.x, br_tr.y - 3.0, br_tr.w, 22.0),
        action: Action::SetBrightness(state.settings.brightness_pct),
        right_action: None,
        tooltip: Some("Ajustar brillo de pantalla en vivo".into()),
        slider: Some(SliderDragKind::Brightness { x: br_tr.x, w: br_tr.w }),
    });

    ctx.draw_line(lx + 12.0, y + 104.0, lx + cw - 12.0, y + 104.0, p.border_strong, 1.0);

    // Segmented [ Salida de audio ] [ Micrófono ] switcher
    let seg_bg = Rect::new(lx + 12.0, y + 112.0, 198.0, 26.0);
    ctx.fill_rrect(seg_bg, 9.0, p.card_bg);
    let out_tab = Rect::new(seg_bg.x + 3.0, seg_bg.y + 3.0, 108.0, 20.0);
    let mic_tab = Rect::new(seg_bg.x + 113.0, seg_bg.y + 3.0, 82.0, 20.0);
    if !state.cc_showing_mics {
        ctx.fill_rrect(out_tab, 7.0, p.accent);
    } else {
        ctx.fill_rrect(mic_tab, 7.0, p.accent);
    }
    ctx.draw_text_center("Salida de audio", out_tab, TextStyle::Small10Bold, if !state.cc_showing_mics { theme::CRISP_WHITE } else { p.text_secondary });
    ctx.draw_text_center("Micrófono", mic_tab, TextStyle::Small10Bold, if state.cc_showing_mics { theme::CRISP_WHITE } else { p.text_secondary });
    hits.push(HitRegion { id: "cc_tab_out".into(), rect: out_tab, action: Action::SwitchAudioDeviceTab(false), right_action: None, tooltip: None, slider: None });
    hits.push(HitRegion { id: "cc_tab_mic".into(), rect: mic_tab, action: Action::SwitchAudioDeviceTab(true), right_action: None, tooltip: None, slider: None });

    let panel_lnk = Rect::new(lx + cw - 58.0, y + 112.0, 46.0, 26.0);
    ctx.draw_text_right("Panel", panel_lnk, TextStyle::Small10Med, p.text_tertiary);
    hits.push(HitRegion { id: "cc_snd_panel".into(), rect: panel_lnk, action: Action::LaunchAuraSettings(Some("sound".into())), right_action: None, tooltip: None, slider: None });

    let mut dy_dev = y + 144.0;
    if devices.is_empty() {
        ctx.draw_text("Sin dispositivos detectados.", Rect::new(lx + 14.0, dy_dev, cw - 28.0, 28.0), TextStyle::Body11Reg, p.text_tertiary);
    } else {
        for (i, d) in devices.iter().take(3).enumerate() {
            let dr = Rect::new(lx + 12.0, dy_dev, cw - 24.0, 36.0);
            let id = format!("cc_dev_{}", i);
            let hov = state.hovered_id.as_deref() == Some(&id);
            let bg = if d.is_default {
                p.accent_tint
            } else if hov {
                p.elevated_hover
            } else {
                p.card_bg
            };
            ctx.fill_card(dr, 11.0, bg, if d.is_default { p.accent_border } else { p.border });

            let ic_box = Rect::new(dr.x + 8.0, dr.y + 7.0, 22.0, 22.0);
            ctx.fill_rrect(ic_box, 7.0, if d.is_default { p.accent } else { p.elevated });
            ctx.draw_vector_icon(
                if state.cc_showing_mics { VectorIcon::Mic } else { VectorIcon::Volume },
                ic_box.x + 11.0,
                ic_box.y + 11.0,
                11.0,
                if d.is_default { theme::CRISP_WHITE } else { p.text_secondary },
            );

            let tw = if d.is_default { dr.w - 94.0 } else { dr.w - 46.0 };
            ctx.draw_text(
                &d.short_name,
                Rect::new(dr.x + 38.0, dr.y + 3.0, tw, 16.0),
                if d.is_default { TextStyle::Body11Med } else { TextStyle::Body11Reg },
                if d.is_default { p.text_primary } else { p.text_secondary },
            );
            if !d.sub_label.is_empty() {
                ctx.draw_text(&d.sub_label, Rect::new(dr.x + 38.0, dr.y + 19.0, tw, 13.0), TextStyle::Small9Reg, p.text_tertiary);
            }
            if d.is_default {
                let badge = Rect::new(dr.x + dr.w - 50.0, dr.y + 9.0, 42.0, 18.0);
                ctx.fill_rrect(badge, 7.0, p.accent);
                ctx.draw_text_center("Activo", badge, TextStyle::Small9Reg, theme::CRISP_WHITE);
            }
            hits.push(HitRegion { id, rect: dr, action: Action::SelectAudioDevice(d.id.clone()), right_action: None, tooltip: Some(d.full_name.clone()), slider: None });
            dy_dev += 41.0;
        }
    }
    y += vol_card_h + 12.0;

    // 4. 2x4 MATERIAL 3 QUICK TOGGLES (Red/Wi-Fi, Compartir Internet, Bluetooth, Modo Avión, Luz Nocturna, Café, Teclado, AuraShot!)
    ctx.draw_text("CONTROLES RÁPIDOS", Rect::new(lx + 2.0, y, cw, 15.0), TextStyle::Small10Bold, p.text_tertiary);
    y += 19.0;

    let wifi_active = state.radios.has_internet || state.radios.wifi_on || state.radios.ethernet_up;
    let (net_title, net_icon, net_accent) = match state.radios.net_kind {
        crate::radios::NetKind::Ethernet => ("Red · Ethernet", VectorIcon::Ethernet, theme::EMERALD),
        crate::radios::NetKind::Wifi => ("Red · Wi-Fi", VectorIcon::Wifi, p.accent),
        crate::radios::NetKind::Offline => ("Sin internet", VectorIcon::NoInternet, theme::CORAL),
    };
    let wifi_sub = if !state.radios.net_label.is_empty() {
        format!("{} ›", state.radios.net_label)
    } else {
        format!("{} ›", state.net_status)
    };
    let hs = &state.radios.hotspot;
    let hs_sub = if !hs.supported {
        "No compatible".to_string()
    } else if hs.is_on {
        format!("ON · {}/{} disp.", hs.client_count, hs.max_clients)
    } else if !hs.ssid.is_empty() {
        format!("OFF · {}", hs.ssid)
    } else {
        "Apagado".to_string()
    };
    let bt_active = state.radios.bt_on;
    let bt_sub = if !state.radios.bt_available {
        "No disponible".to_string()
    } else if bt_active {
        "Activado".to_string()
    } else {
        "Desactivado".to_string()
    };
    let air_on = state.radios.airplane_mode;

    let tile_w = (cw - 8.0) * 0.5;
    let tiles = [
        (
            "cc_q_net",
            net_title,
            wifi_sub,
            net_icon,
            wifi_active || state.cc_network_drawer_open,
            net_accent,
            Action::ToggleNetworkDrawer,
            Some(Action::ToggleWifiRadio),
        ),
        (
            "cc_q_hotspot",
            "Compartir Red",
            hs_sub,
            VectorIcon::Sparkle,
            hs.is_on,
            theme::EMERALD,
            Action::ToggleHotspot,
            Some(Action::LaunchAuraSettings(Some("network".into()))),
        ),
        (
            "cc_q_bt",
            "Bluetooth",
            bt_sub,
            VectorIcon::Bluetooth,
            bt_active,
            theme::BLUE,
            Action::ToggleBluetoothRadio,
            Some(Action::LaunchAuraSettings(Some("network".into()))),
        ),
        (
            "cc_q_air",
            "Modo Avión",
            if air_on { "Radios apagadas".into() } else { "Desactivado".into() },
            VectorIcon::Power,
            air_on,
            theme::AMBER,
            Action::ToggleAirplaneMode,
            None,
        ),
        (
            "cc_q_night",
            "Luz Nocturna",
            if state.settings.night_light { "Filtro cálido ON".into() } else { "Filtro cálido OFF".into() },
            VectorIcon::Moon,
            state.settings.night_light,
            theme::AMBER,
            Action::ToggleNightLight,
            None,
        ),
        (
            "cc_q_awake",
            "Café / Despierto",
            if state.settings.keep_awake { "Sin suspensión".into() } else { "Normal".into() },
            VectorIcon::Coffee,
            state.settings.keep_awake,
            theme::EMERALD,
            Action::ToggleKeepAwake,
            None,
        ),
        (
            "cc_q_lang",
            "Teclado",
            format!("{} · {} idiomas", state.keyboard_lang, state.keyboard_layouts.len().max(1)),
            VectorIcon::Keyboard,
            state.cc_keyboard_menu_open,
            p.accent,
            Action::ToggleKeyboardMenu,
            Some(Action::CycleKeyboardLayout),
        ),
        (
            "cc_q_shot",
            "AuraShot · Clip",
            format!("Impr Pant · {} clips", state.clipboard_items.len()),
            VectorIcon::Maximize,
            state.clipboard_open,
            theme::CORAL,
            Action::LaunchAuraShot,
            Some(Action::ToggleClipboard),
        ),
    ];

    for (idx, (id, title, sub, icon, is_on, accent, act, right_act)) in tiles.into_iter().enumerate() {
        let col = (idx % 2) as f32;
        let row = (idx / 2) as f32;
        let tr = Rect::new(lx + col * (tile_w + 8.0), y + row * 52.0, tile_w, 45.0);
        let hov = state.hovered_id.as_deref() == Some(id);
        let border = if is_on { theme::with_alpha(accent, 0.58) } else { p.border };
        let tile_bg = if is_on {
            windows::Win32::Graphics::Direct2D::Common::D2D1_COLOR_F {
                r: accent.r * 0.24 + p.card_bg.r * 0.76,
                g: accent.g * 0.24 + p.card_bg.g * 0.76,
                b: accent.b * 0.24 + p.card_bg.b * 0.76,
                a: 0.98,
            }
        } else if hov {
            p.elevated_hover
        } else {
            p.elevated
        };
        ctx.fill_card(tr, 14.0, tile_bg, border);

        let ib = Rect::new(tr.x + 10.0, tr.y + 8.5, 28.0, 28.0);
        let offline_tile = id == "cc_q_net" && state.radios.net_kind == crate::radios::NetKind::Offline;
        ctx.fill_rrect(ib, 14.0, if offline_tile { theme::CORAL_BG } else if is_on { accent } else { p.card_bg });
        if id == "cc_q_net" && state.radios.net_kind == crate::radios::NetKind::Wifi {
            ctx.draw_wifi_signal(ib.x + 14.0, ib.y + 13.5, 13.5, state.radios.wifi_bars.max(1), theme::CRISP_WHITE, theme::rgba(255, 255, 255, 0.25));
        } else {
            ctx.draw_vector_icon(icon, ib.x + 14.0, ib.y + 14.0, 13.0, if offline_tile { theme::CORAL } else if is_on { theme::CRISP_WHITE } else { p.text_secondary });
        }
        ctx.draw_text(title, Rect::new(tr.x + 44.0, tr.y + 6.0, tr.w - 50.0, 16.0), TextStyle::Body11Med, p.text_primary);
        ctx.draw_text(&sub, Rect::new(tr.x + 44.0, tr.y + 23.0, tr.w - 50.0, 14.0), TextStyle::Small9Reg, if is_on { theme::rgba(240, 240, 252, 0.88) } else { p.text_tertiary });
        hits.push(HitRegion { id: id.into(), rect: tr, action: act, right_action: right_act, tooltip: None, slider: None });
    }
    y += 214.0;

    // Inline status toast inside Control Center (Hotspot / Wi-Fi / VPN feedback)
    if let Some((ref msg, ok, at)) = state.cc_status_toast {
        if at.elapsed().as_secs_f32() < 4.0 {
            let tr = Rect::new(lx, y, cw, 28.0);
            ctx.fill_card(tr, 10.0, p.card_bg, if ok { theme::EMERALD } else { theme::CORAL });
            ctx.draw_text(msg, Rect::new(tr.x + 10.0, tr.y, tr.w - 20.0, 28.0), TextStyle::Small10Med, if ok { theme::EMERALD } else { theme::CORAL });
            y += 34.0;
        }
    }

    // Inline Windows-11-Style Quick Network & Hotspot Drawer!
    if state.cc_network_drawer_open {
        let wifi_cnt = state.cc_wifi_networks.len().min(5);
        let vpn_cnt = state.cc_vpn_profiles.len().min(3);
        let drawer_h = 96.0
            + if wifi_cnt == 0 { 30.0 } else { wifi_cnt as f32 * 34.0 }
            + if vpn_cnt > 0 { 22.0 + vpn_cnt as f32 * 30.0 } else { 0.0 };
        ctx.fill_card(Rect::new(lx, y, cw, drawer_h), 15.0, p.card_bg, p.accent_border);

        let ip_info = if !state.cc_net_details.ipv4.is_empty() {
            format!("{} · IP {} · {} Mbps", state.cc_net_details.adapter_name, state.cc_net_details.ipv4, state.cc_net_details.link_speed_mbps)
        } else {
            state.radios.net_label.clone()
        };
        ctx.draw_text("Panel de Red y Zona Wi-Fi", Rect::new(lx + 12.0, y + 8.0, cw - 120.0, 16.0), TextStyle::Body11Med, p.text_primary);
        ctx.draw_text(&ip_info, Rect::new(lx + 12.0, y + 24.0, cw - 110.0, 14.0), TextStyle::Small9Reg, p.text_tertiary);

        let cfg_r = Rect::new(lx + cw - 92.0, y + 8.0, 82.0, 24.0);
        ctx.fill_rrect(cfg_r, 8.0, p.elevated);
        ctx.draw_text_center("Ajustes Red", cfg_r, TextStyle::Small9Reg, p.accent_light);
        hits.push(HitRegion { id: "cc_net_cfg".into(), rect: cfg_r, action: Action::LaunchAuraSettings(Some("network".into())), right_action: None, tooltip: None, slider: None });

        // Quick action pills: Wi-Fi toggle, Hotspot toggle, Scan networks
        let py = y + 44.0;
        let bw3 = (cw - 32.0) / 3.0;
        let w_btn = Rect::new(lx + 10.0, py, bw3, 26.0);
        ctx.fill_rrect(w_btn, 8.0, if state.radios.wifi_on { p.accent } else { p.elevated });
        ctx.draw_text_center(if state.radios.wifi_on { "Wi-Fi: ON" } else { "Wi-Fi: OFF" }, w_btn, TextStyle::Small10Bold, theme::CRISP_WHITE);
        hits.push(HitRegion { id: "cc_nd_wifi".into(), rect: w_btn, action: Action::ToggleWifiRadio, right_action: None, tooltip: None, slider: None });

        let h_btn = Rect::new(lx + 16.0 + bw3, py, bw3, 26.0);
        ctx.fill_rrect(h_btn, 8.0, if hs.is_on { theme::EMERALD } else { p.elevated });
        ctx.draw_text_center(if hs.is_on { "Hotspot: ON" } else { "Hotspot: OFF" }, h_btn, TextStyle::Small10Bold, theme::CRISP_WHITE);
        hits.push(HitRegion { id: "cc_nd_hs".into(), rect: h_btn, action: Action::ToggleHotspot, right_action: None, tooltip: None, slider: None });

        let s_btn = Rect::new(lx + 22.0 + bw3 * 2.0, py, bw3, 26.0);
        ctx.fill_rrect(s_btn, 8.0, p.elevated);
        ctx.draw_text_center(if state.cc_wifi_scanning { "Buscando..." } else { "Escanear" }, s_btn, TextStyle::Small10Bold, p.text_primary);
        hits.push(HitRegion { id: "cc_nd_scan".into(), rect: s_btn, action: Action::ScanWifiNetworks, right_action: None, tooltip: None, slider: None });

        let mut ny = py + 34.0;
        if state.cc_wifi_networks.is_empty() {
            let empty_msg = if !state.radios.wifi_available {
                "Sin adaptador Wi-Fi · Usando Ethernet"
            } else if !state.radios.wifi_on {
                "Enciende Wi-Fi para ver redes cercanas"
            } else if state.cc_wifi_scanning {
                "Escaneando redes Wi-Fi cercanas..."
            } else {
                "Pulsa 'Escanear' para listar redes Wi-Fi"
            };
            ctx.draw_text(empty_msg, Rect::new(lx + 12.0, ny, cw - 24.0, 22.0), TextStyle::Small10Med, p.text_tertiary);
            ny += 26.0;
        } else {
            for (i, net) in state.cc_wifi_networks.iter().take(5).enumerate() {
                let nr = Rect::new(lx + 10.0, ny, cw - 20.0, 28.0);
                let id = format!("cc_wifi_{}", i);
                let hov = state.hovered_id.as_deref() == Some(&id);
                ctx.fill_rrect(nr, 8.0, if net.connected { p.accent_tint } else if hov { p.elevated_hover } else { p.elevated });
                ctx.draw_wifi_signal(nr.x + 14.0, nr.y + 14.0, 12.0, net.bars.max(1), if net.connected { theme::EMERALD } else { p.accent_light }, theme::rgba(255, 255, 255, 0.18));
                ctx.draw_text(&net.ssid, Rect::new(nr.x + 30.0, nr.y, nr.w - 114.0, 28.0), TextStyle::Body11Med, p.text_primary);
                let badge = if net.connected { "Desconectar" } else if net.saved { "Conectar" } else { "Abrir Ajustes" };
                ctx.draw_text_right(badge, Rect::new(nr.x + nr.w - 82.0, nr.y, 74.0, 28.0), TextStyle::Small9Reg, if net.connected { theme::EMERALD } else { p.accent_light });
                let act = if net.connected {
                    Action::DisconnectWifi
                } else if net.saved || !net.secured {
                    Action::ConnectWifi(net.ssid.clone())
                } else {
                    Action::LaunchAuraSettings(Some("network".into()))
                };
                hits.push(HitRegion { id, rect: nr, action: act, right_action: None, tooltip: None, slider: None });
                ny += 34.0;
            }
        }

        if !state.cc_vpn_profiles.is_empty() {
            ctx.draw_text("PERFILES VPN", Rect::new(lx + 12.0, ny + 2.0, cw - 24.0, 14.0), TextStyle::Small9Reg, p.text_tertiary);
            ny += 18.0;
            for (i, vpn) in state.cc_vpn_profiles.iter().take(3).enumerate() {
                let vr = Rect::new(lx + 10.0, ny, cw - 20.0, 26.0);
                let id = format!("cc_vpn_{}", i);
                ctx.fill_rrect(vr, 8.0, if vpn.connected { p.accent_tint } else { p.elevated });
                ctx.draw_text(&vpn.name, Rect::new(vr.x + 10.0, vr.y, vr.w - 92.0, 26.0), TextStyle::Small10Bold, p.text_primary);
                ctx.draw_text_right(if vpn.connected { "Conectada" } else { "Conectar" }, Rect::new(vr.x + vr.w - 78.0, vr.y, 70.0, 26.0), TextStyle::Small9Reg, if vpn.connected { theme::EMERALD } else { p.accent_light });
                hits.push(HitRegion { id, rect: vr, action: Action::ToggleVpnProfile(vpn.name.clone(), vpn.connected), right_action: None, tooltip: None, slider: None });
                ny += 30.0;
            }
        }

        y += drawer_h + 10.0;
    }

    // Inline Keyboard Layout Drawer
    if state.cc_keyboard_menu_open {
        let kh = 16.0 + state.keyboard_layouts.len().max(1) as f32 * 34.0;
        ctx.fill_card(Rect::new(lx, y, cw, kh), 14.0, p.card_bg, p.accent_border);
        let mut ky = y + 8.0;
        for (i, lay) in state.keyboard_layouts.iter().enumerate() {
            let kr = Rect::new(lx + 8.0, ky, cw - 16.0, 28.0);
            let id = format!("cc_kbd_sel_{}", i);
            let active = if state.active_keyboard_hkl != 0 { lay.hkl == state.active_keyboard_hkl } else { i == 0 };
            let hov = state.hovered_id.as_deref() == Some(&id);
            ctx.fill_rrect(kr, 9.0, if active { p.accent } else if hov { p.elevated } else { theme::rgba(0, 0, 0, 0.0) });
            ctx.draw_text(&lay.short_code, Rect::new(kr.x + 10.0, kr.y, 48.0, 28.0), TextStyle::Mono11Med, p.text_primary);
            ctx.draw_text(&lay.label, Rect::new(kr.x + 60.0, kr.y, kr.w - 70.0, 28.0), TextStyle::Body11Med, if active { theme::CRISP_WHITE } else { p.text_secondary });
            hits.push(HitRegion { id, rect: kr, action: Action::SelectKeyboardLayout(lay.hkl), right_action: None, tooltip: None, slider: None });
            ky += 34.0;
        }
        y += kh + 10.0;
    }

    // 5. ILLOGICAL-IMPULSE BOTTOM TABBED PRODUCTIVITY DOCK ([ To-Do ] | [ Pomodoro ] | [ Mezclador ] | [ Apps ])
    let dock_tabs = [(0u8, "To-Do"), (1u8, "Pomodoro"), (2u8, "Mezclador"), (3u8, "Apps 2º")];
    let tab_bar = Rect::new(lx, y, cw, 28.0);
    ctx.fill_card(tab_bar, 11.0, p.card_bg, p.border);
    let tw = (cw - 8.0) / 4.0;
    for (ti, lbl) in dock_tabs {
        let tr = Rect::new(lx + 4.0 + ti as f32 * tw, y + 3.0, tw, 22.0);
        let active = state.cc_bottom_tab == ti;
        if active {
            ctx.fill_rrect(tr, 8.0, p.accent);
        }
        ctx.draw_text_center(lbl, tr, TextStyle::Small10Bold, if active { theme::CRISP_WHITE } else { p.text_secondary });
        hits.push(HitRegion { id: format!("cc_btab_{}", ti), rect: tr, action: Action::SetCcBottomTab(ti), right_action: None, tooltip: None, slider: None });
    }
    y += 34.0;

    match state.cc_bottom_tab {
        0 => {
            // Interactive Persistent To-Do Checklist (visible above the fold on frame 1!)
            let rows = state.todos.len().min(4);
            let card_h = 52.0 + rows as f32 * 32.0;
            ctx.fill_card(Rect::new(lx, y, cw, card_h), 16.0, p.card_bg, p.border);

            let inp_r = Rect::new(lx + 10.0, y + 10.0, cw - 66.0, 28.0);
            ctx.fill_card(inp_r, 9.0, p.elevated, if state.todo_input_active { p.accent } else { p.border });
            let placeholder = if state.todo_input_active {
                format!("{}|", state.todo_input_text)
            } else if state.todo_input_text.is_empty() {
                "Escribe una tarea y pulsa Enter...".to_string()
            } else {
                state.todo_input_text.clone()
            };
            ctx.draw_text(&placeholder, Rect::new(inp_r.x + 10.0, inp_r.y, inp_r.w - 16.0, 28.0), TextStyle::Body11Reg, if state.todo_input_active { p.text_primary } else { p.text_tertiary });
            hits.push(HitRegion { id: "cc_todo_inp".into(), rect: inp_r, action: Action::StartTodoInput, right_action: None, tooltip: None, slider: None });

            let add_r = Rect::new(lx + cw - 50.0, y + 10.0, 40.0, 28.0);
            ctx.fill_rrect(add_r, 9.0, p.accent);
            ctx.draw_vector_icon(VectorIcon::Plus, add_r.x + 20.0, add_r.y + 14.0, 12.0, theme::CRISP_WHITE);
            hits.push(HitRegion { id: "cc_todo_add".into(), rect: add_r, action: Action::SubmitTodoInput, right_action: None, tooltip: Some("Añadir tarea".into()), slider: None });

            let mut ty = y + 44.0;
            for item in state.todos.iter().take(4) {
                let rr = Rect::new(lx + 10.0, ty, cw - 20.0, 26.0);
                let chk = Rect::new(rr.x + 4.0, rr.y + 4.0, 18.0, 18.0);
                ctx.fill_card(chk, 6.0, if item.done { p.accent } else { p.elevated }, if item.done { p.accent } else { p.border_strong });
                if item.done {
                    ctx.draw_vector_icon(VectorIcon::Check, chk.x + 9.0, chk.y + 9.0, 10.0, theme::CRISP_WHITE);
                }
                ctx.draw_text(&item.text, Rect::new(rr.x + 30.0, rr.y, rr.w - 58.0, 26.0), TextStyle::Body11Med, if item.done { p.text_tertiary } else { p.text_primary });
                hits.push(HitRegion { id: format!("cc_td_tog_{}", item.id), rect: Rect::new(rr.x, rr.y, rr.w - 26.0, 26.0), action: Action::ToggleTodo(item.id), right_action: None, tooltip: None, slider: None });

                let del_r = Rect::new(rr.x + rr.w - 22.0, rr.y + 4.0, 18.0, 18.0);
                ctx.fill_rrect(del_r, 6.0, p.elevated);
                ctx.draw_vector_icon(VectorIcon::Close, del_r.x + 9.0, del_r.y + 9.0, 8.0, p.text_tertiary);
                hits.push(HitRegion { id: format!("cc_td_del_{}", item.id), rect: del_r, action: Action::DeleteTodo(item.id), right_action: None, tooltip: Some("Eliminar".into()), slider: None });
                ty += 32.0;
            }
            y += card_h + 12.0;
        }
        1 => {
            // Live Pomodoro Focus Widget inside Sidebar!
            let card_h = 112.0;
            ctx.fill_card(Rect::new(lx, y, cw, card_h), 16.0, p.card_bg, p.border);
            let mins = state.pomodoro_secs_left / 60;
            let secs = state.pomodoro_secs_left % 60;
            let mode_lbl = if state.pomodoro_is_break { "Descanso corto (5m)" } else { "Enfoque profundo (25m)" };
            ctx.draw_text(mode_lbl, Rect::new(lx + 16.0, y + 14.0, cw - 130.0, 18.0), TextStyle::Body11Med, p.accent_light);
            ctx.draw_text(&format!("{:02}:{:02}", mins, secs), Rect::new(lx + 16.0, y + 36.0, 130.0, 36.0), TextStyle::Title22Bold, p.text_primary);

            let btn_tog = Rect::new(lx + cw - 116.0, y + 28.0, 100.0, 32.0);
            ctx.fill_rrect(btn_tog, 11.0, if state.pomodoro_active { theme::CORAL } else { p.accent });
            ctx.draw_text_center(if state.pomodoro_active { "Pausar" } else { "Iniciar" }, btn_tog, TextStyle::Body11Med, theme::CRISP_WHITE);
            hits.push(HitRegion { id: "cc_pomo_tog".into(), rect: btn_tog, action: Action::PomodoroToggle, right_action: None, tooltip: None, slider: None });

            let b_foc = Rect::new(lx + 16.0, y + 76.0, (cw - 40.0) * 0.5, 24.0);
            let b_brk = Rect::new(b_foc.x + b_foc.w + 8.0, y + 76.0, b_foc.w, 24.0);
            ctx.fill_rrect(b_foc, 8.0, if !state.pomodoro_is_break { p.accent_tint } else { p.elevated });
            ctx.fill_rrect(b_brk, 8.0, if state.pomodoro_is_break { p.accent_tint } else { p.elevated });
            ctx.draw_text_center("25 min Enfoque", b_foc, TextStyle::Small10Bold, p.text_primary);
            ctx.draw_text_center("5 min Descanso", b_brk, TextStyle::Small10Bold, p.text_primary);
            hits.push(HitRegion { id: "cc_pomo_25".into(), rect: b_foc, action: Action::PomodoroMode(false), right_action: None, tooltip: None, slider: None });
            hits.push(HitRegion { id: "cc_pomo_5".into(), rect: b_brk, action: Action::PomodoroMode(true), right_action: None, tooltip: None, slider: None });
            y += card_h + 12.0;
        }
        2 => {
            // Per-App Volume Mixer Tab!
            let mixer_h = if state.app_sessions.is_empty() { 44.0 } else { 14.0 + state.app_sessions.len().min(5) as f32 * 42.0 };
            ctx.fill_card(Rect::new(lx, y, cw, mixer_h), 16.0, p.card_bg, p.border);
            if state.app_sessions.is_empty() {
                ctx.draw_text("Sin apps reproduciendo sonido ahora.", Rect::new(lx + 14.0, y + 12.0, cw - 28.0, 20.0), TextStyle::Body11Reg, p.text_tertiary);
            } else {
                let mut my = y + 8.0;
                for (i, sess) in state.app_sessions.iter().take(5).enumerate() {
                    let ic_r = Rect::new(lx + 12.0, my + 1.0, 18.0, 18.0);
                    if sess.exe_path.is_empty() || !ctx.draw_app_icon(&sess.exe_path, ic_r) {
                        ctx.fill_rrect(ic_r, 5.0, p.elevated);
                        ctx.draw_vector_icon(VectorIcon::Volume, ic_r.x + 9.0, ic_r.y + 9.0, 10.0, p.text_secondary);
                    }
                    ctx.draw_text(&sess.display_name, Rect::new(lx + 38.0, my, cw - 118.0, 20.0), TextStyle::Body11Med, p.text_primary);
                    let p_str = if sess.is_muted { "MUTE".into() } else { format!("{}%", sess.volume_percent) };
                    ctx.draw_text_right(&p_str, Rect::new(lx + cw - 78.0, my, 42.0, 20.0), TextStyle::Mono11Med, if sess.is_muted { theme::CORAL } else { p.text_secondary });
                    let mb = Rect::new(lx + cw - 32.0, my, 20.0, 20.0);
                    ctx.fill_rrect(mb, 10.0, if sess.is_muted { theme::CORAL } else { p.elevated });
                    ctx.draw_vector_icon(if sess.is_muted { VectorIcon::VolumeMute } else { VectorIcon::Volume }, mb.x + 10.0, mb.y + 10.0, 10.0, theme::CRISP_WHITE);
                    hits.push(HitRegion { id: format!("cc_app_mute_{}", i), rect: mb, action: Action::ToggleAppMute(sess.pids.clone(), !sess.is_muted), right_action: None, tooltip: None, slider: None });
                    let sr = Rect::new(lx + 12.0, my + 24.0, cw - 24.0, 8.0);
                    ctx.fill_rrect(sr, 4.0, p.elevated);
                    if !sess.is_muted {
                        let fw = ((sess.volume_percent as f32 / 100.0) * sr.w).max(8.0);
                        ctx.fill_rrect(Rect::new(sr.x, sr.y, fw, 8.0), 4.0, p.accent);
                    }
                    hits.push(HitRegion { id: format!("cc_app_vol_{}", i), rect: Rect::new(sr.x, sr.y - 4.0, sr.w, 16.0), action: Action::SetAppVolume(sess.pids.clone(), sess.volume_percent), right_action: None, tooltip: None, slider: Some(SliderDragKind::AppVolume { x: sr.x, w: sr.w, pids: sess.pids.clone() }) });
                    my += 42.0;
                }
            }
            y += mixer_h + 12.0;
        }
        _ => {
            // Dynamic Background Tray Apps Grid
            let tray_rows = ((state.tray_apps.len().max(1) + 3) / 4) as f32;
            let tray_h = 16.0 + tray_rows * 56.0;
            ctx.fill_card(Rect::new(lx, y, cw, tray_h), 16.0, p.card_bg, p.border);
            let cell_w = (cw - 24.0) / 4.0;
            for (idx, app) in state.tray_apps.iter().enumerate() {
                let col = (idx % 4) as f32;
                let row = (idx / 4) as f32;
                let cr = Rect::new(lx + 9.0 + col * (cell_w + 2.0), y + 9.0 + row * 56.0, cell_w - 4.0, 50.0);
                let id = format!("cc_tray_{}", idx);
                let hov = state.hovered_id.as_deref() == Some(&id);
                ctx.fill_rrect(cr, 11.0, if hov { p.elevated_hover } else { p.pill_subtle });
                let ir = Rect::new(cr.x + (cr.w - 22.0) * 0.5, cr.y + 6.0, 22.0, 22.0);
                if app.exe_path.is_empty() || !ctx.draw_app_icon(&app.exe_path, ir) {
                    ctx.fill_rrect(ir, 7.0, p.accent);
                    let ch = app.display_name.chars().next().unwrap_or('A').to_string();
                    ctx.draw_text_center(&ch, ir, TextStyle::Small10Bold, theme::CRISP_WHITE);
                }
                ctx.draw_text_center(&app.display_name, Rect::new(cr.x + 2.0, cr.y + 30.0, cr.w - 4.0, 15.0), TextStyle::Small9Reg, p.text_secondary);
                let open_act = if app.main_hwnd != 0 {
                    Action::FocusWindow(app.main_hwnd)
                } else if !app.fallback_launch.is_empty() {
                    Action::LaunchTarget(app.fallback_launch.clone())
                } else {
                    Action::LaunchTarget(app.exe_path.clone())
                };
                hits.push(HitRegion { id, rect: cr, action: open_act, right_action: Some(Action::KillBackgroundApp(app.process_key.clone())), tooltip: Some(format!("{} · Clic abrir · Clic der. cerrar", app.display_name)), slider: None });
            }
            y += tray_h + 12.0;
        }
    }

    // 7. FOOTER
    let btn_set = Rect::new(lx, y, 76.0, 30.0);
    ctx.fill_rrect(btn_set, 11.0, if state.hovered_id.as_deref() == Some("cc_set") { p.elevated_hover } else { p.elevated });
    ctx.draw_text_center("Ajustes", btn_set, TextStyle::Body11Med, p.text_primary);
    hits.push(HitRegion { id: "cc_set".into(), rect: btn_set, action: Action::OpenSettings, right_action: None, tooltip: None, slider: None });

    let has_upd = state.update_available_ver.is_some();
    let upd_lbl = state.update_available_ver.as_ref().map(|v| format!("Actualizar v{}", v)).unwrap_or_else(|| "Actualizar".to_string());
    let btn_upd = Rect::new(lx + 82.0, y, 114.0, 30.0);
    ctx.fill_rrect(btn_upd, 11.0, if has_upd { p.accent } else { p.elevated });
    ctx.draw_text_center(&upd_lbl, btn_upd, TextStyle::Body11Med, p.text_primary);
    hits.push(HitRegion { id: "cc_upd".into(), rect: btn_upd, action: Action::LaunchUpdater, right_action: None, tooltip: None, slider: None });

    let btn_res = Rect::new(lx + cw - 134.0, y, 134.0, 30.0);
    ctx.fill_rrect(btn_res, 11.0, theme::CORAL_BG);
    ctx.draw_text_center("Restaurar Windows", btn_res, TextStyle::Body11Med, theme::CRISP_WHITE);
    hits.push(HitRegion { id: "cc_restore".into(), rect: btn_res, action: Action::RestoreWindowsAndExit, right_action: None, tooltip: None, slider: None });

    let total_content_h = (y + scroll) + 46.0;
    ctx.pop_clip();

    if total_content_h > win_h - 32.0 {
        let track_h = win_h - 40.0;
        let thumb_h = ((win_h - 34.0) / total_content_h * track_h).clamp(32.0, track_h);
        let max_s = (total_content_h - (win_h - 34.0)).max(1.0);
        let thumb_y = 20.0 + (scroll / max_s).clamp(0.0, 1.0) * (track_h - thumb_h);
        ctx.fill_rrect(Rect::new(CC_WIDTH - 15.0, thumb_y, 5.0, thumb_h), 2.5, p.border_strong);
    }

    ctx.reset_transform();
    if _layer.is_some() {
        ctx.pop_layer();
    }
    total_content_h
}
