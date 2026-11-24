//! Drool Dark Mode Control Center v3.1 (Apple Circular Reveal + Android/iOS In-Place Controls!)
//! - Expanding circular clip reveal anchored at the Top Bar trigger pill (210ms enter / 135ms exit)
//! - Hero SMTC Media Player (Real Song Title + Artist + Prev / Play-Pause / Next)
//! - Master Volume + Drool Hatched Slider + Inline [ Salida de audio | Micrófono ] 1-Click Switcher
//! - Grouped Per-App Volume Mixer (Real Core Audio + HD Icons + Draggable Sliders)
//! - Native Android/iOS In-Place 2x2 Quick Toggles: Wi-Fi Radio ON/OFF, Bluetooth Radio ON/OFF,
//!   Inline Keyboard Layout Picker (no Windows Settings popup!), and Fondos & Widgets Studio!
//! - Complete Dynamic Background Tray Apps Grid (auto-discovers ALL running tray apps!)

use crate::models::ShellMode;
use crate::renderer::{DrawCtx, Rect, TextStyle};
use crate::theme::{self, VectorIcon};
use crate::ui_state::{Action, AppState, HitRegion, SliderDragKind};

pub const CC_WIDTH: f32 = 420.0;
pub const CC_MAX_H: f32 = 840.0;

pub fn render_control_center(ctx: &mut DrawCtx<'_>, state: &AppState, win_h: f32, hits: &mut Vec<HitRegion>) -> f32 {
    hits.clear();
    let t = state.cc_anim.value.clamp(0.0, 1.0);
    if t <= 0.01 {
        return 0.0;
    }

    // Apple circular expansion reveal anchored at the Top Bar Control Center trigger!
    let ox = CC_WIDTH - 66.0;
    let oy = 14.0;
    let max_r = (CC_WIDTH * CC_WIDTH + win_h * win_h).sqrt() + 24.0;
    let cur_r = 22.0 + (max_r - 22.0) * t;
    let layer_alpha = (t * 2.2).clamp(0.0, 1.0);

    let full_r = Rect::new(0.0, 0.0, CC_WIDTH, win_h);
    // Only allocate circle reveal layer during active transition; ZERO offscreen layer at rest!
    let _layer = if t < 0.96 {
        ctx.push_circle_mask_layer(full_r, ox, oy, cur_r, layer_alpha)
    } else {
        None
    };

    let scale = 0.95 + 0.05 * t;
    let dy = (1.0 - t) * -6.0;
    ctx.set_transform_scale_origin(scale, ox, oy, 0.0, dy);

    let outer = Rect::new(6.0, 6.0, CC_WIDTH - 12.0, win_h - 12.0);
    ctx.fill_card(outer, 22.0, theme::SURFACE, theme::BORDER_STRONG);

    let content_rect = Rect::new(20.0, 18.0, CC_WIDTH - 46.0, win_h - 34.0);
    ctx.push_clip(content_rect);

    let lx = 22.0;
    let cw = CC_WIDTH - 50.0;
    let scroll = state.cc_scroll.max(0.0);
    let mut y = 20.0 - scroll;

    // 1. HEADER WITH UNIFIED macOS TRAFFIC LIGHTS + NATIVE AURA SETTINGS LAUNCHER!
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
        hits.push(HitRegion {
            id: id.into(),
            rect: Rect::new(cx - 8.0, y + 2.0, 16.0, 18.0),
            action: act,
            right_action: None,
            tooltip: None,
            slider: None,
        });
    }
    ctx.draw_text("Centro de Control", Rect::new(lx + 56.0, y, 164.0, 22.0), TextStyle::Title14Semi, theme::TEXT_PRIMARY);
    let aura_cfg_r = Rect::new(lx + cw - 122.0, y, 122.0, 22.0);
    ctx.draw_text_right("Configuración Aura", aura_cfg_r, TextStyle::Body11Med, theme::VIOLET_LIGHT);
    hits.push(HitRegion {
        id: "cc_wincfg".into(),
        rect: aura_cfg_r,
        action: Action::LaunchAuraSettings(None),
        right_action: None,
        tooltip: Some("Abrir Configuración nativa de AuraOS".into()),
        slider: None,
    });
    y += 32.0;

    // 2. ELECTRIC VIOLET HERO MEDIA PLAYER (REAL WINDOWS SMTC!)
    let m_card = Rect::new(lx, y, cw, 64.0);
    ctx.fill_gradient_rrect(m_card, 18.0, theme::VIOLET_LIGHT, theme::VIOLET_DEEP);
    let m_ic = Rect::new(lx + 12.0, y + 13.0, 38.0, 38.0);
    ctx.fill_rrect(m_ic, 12.0, theme::rgba(255, 255, 255, 0.22));
    ctx.draw_vector_icon(VectorIcon::MusicNote, m_ic.x + 19.0, m_ic.y + 19.0, 17.0, theme::TEXT_PRIMARY);

    let sub_line = if !state.now_playing.artist.is_empty() {
        format!("{} · {}", state.now_playing.source_app, state.now_playing.artist)
    } else {
        state.now_playing.source_app.clone()
    };
    ctx.draw_text(&sub_line, Rect::new(lx + 60.0, y + 13.0, cw - 164.0, 16.0), TextStyle::Small10Med, theme::rgba(240, 235, 255, 0.88));
    ctx.draw_text(&state.now_playing.title, Rect::new(lx + 60.0, y + 30.0, cw - 164.0, 20.0), TextStyle::Body12Semi, theme::TEXT_PRIMARY);

    let m_btns = [
        ("cc_m_prev", VectorIcon::Prev, false, Action::MediaPrev),
        ("cc_m_play", VectorIcon::PlayPause, true, Action::MediaPlayPause),
        ("cc_m_next", VectorIcon::Next, false, Action::MediaNext),
    ];
    let mut mbx = lx + cw - 102.0;
    for (id, ic, hero, act) in m_btns {
        let sz = if hero { 32.0 } else { 26.0 };
        let by = y + (64.0 - sz) * 0.5;
        let br = Rect::new(mbx, by, sz, sz);
        let bg = if hero { theme::CRISP_WHITE } else { theme::rgba(0, 0, 0, 0.24) };
        ctx.fill_rrect(br, sz * 0.5, bg);
        ctx.draw_vector_icon(ic, br.x + sz * 0.5, br.y + sz * 0.5, if hero { 13.0 } else { 11.0 }, if hero { theme::DARK_INK } else { theme::TEXT_PRIMARY });
        hits.push(HitRegion { id: id.into(), rect: br, action: act, right_action: None, tooltip: None, slider: None });
        mbx += sz + 5.0;
    }
    y += 74.0;

    // 3. MASTER VOLUME + INLINE OUTPUT / MICROPHONE SELECTOR CARD
    let devices = if state.cc_showing_mics { &state.mic_devices } else { &state.output_devices };
    let dev_rows = devices.len().max(1) as f32;
    let vol_card_h = 122.0 + dev_rows * 44.0;
    let vol_card = Rect::new(lx, y, cw, vol_card_h);
    ctx.fill_card(vol_card, 16.0, theme::ELEVATED, theme::BORDER);

    let mute_r = Rect::new(lx + 12.0, y + 11.0, 30.0, 30.0);
    ctx.fill_rrect(mute_r, 15.0, if state.master_muted { theme::CORAL } else { theme::VIOLET });
    ctx.draw_vector_icon(
        if state.master_muted { VectorIcon::VolumeMute } else { VectorIcon::Volume },
        mute_r.x + 15.0,
        mute_r.y + 15.0,
        13.5,
        theme::TEXT_PRIMARY,
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
    ctx.draw_text("Volumen Maestro", Rect::new(lx + 52.0, y + 10.0, cw - 120.0, 17.0), TextStyle::Body12Semi, theme::TEXT_PRIMARY);
    ctx.draw_text(active_out_name, Rect::new(lx + 52.0, y + 27.0, cw - 120.0, 14.0), TextStyle::Small9Reg, theme::TEXT_TERTIARY);
    let vol_txt = if state.master_muted { "MUTE".into() } else { format!("{}%", state.master_vol) };
    ctx.draw_text_right(&vol_txt, Rect::new(lx + cw - 66.0, y + 14.0, 52.0, 22.0), TextStyle::Mono12Bold, theme::TEXT_PRIMARY);

    // Drool Diagonal-Hatched Master Volume Track
    let tr = Rect::new(lx + 12.0, y + 48.0, cw - 24.0, 22.0);
    ctx.fill_hatched_track(tr, 11.0);
    let frac = if state.master_muted { 0.0 } else { (state.master_vol as f32 / 100.0).clamp(0.0, 1.0) };
    let fill_w = (frac * tr.w).max(22.0);
    ctx.fill_rrect(Rect::new(tr.x, tr.y, fill_w, 22.0), 11.0, if state.master_muted { theme::CORAL } else { theme::VIOLET });
    let knob_x = (tr.x + frac * (tr.w - 22.0) + 11.0).clamp(tr.x + 11.0, tr.x + tr.w - 11.0);
    ctx.fill_circle(knob_x, tr.y + 11.0, 10.0, theme::CRISP_WHITE);
    ctx.stroke_circle(knob_x, tr.y + 11.0, 10.0, theme::VIOLET, 2.0);
    hits.push(HitRegion {
        id: "cc_vol_slider".into(),
        rect: tr,
        action: Action::SetMasterVolume(state.master_vol),
        right_action: None,
        tooltip: None,
        slider: Some(SliderDragKind::MasterVolume { x: tr.x, w: tr.w }),
    });

    ctx.draw_line(lx + 12.0, y + 80.0, lx + cw - 12.0, y + 80.0, theme::BORDER_STRONG, 1.0);

    // Segmented [ Salida de audio ] [ Micrófono ] switcher
    let seg_bg = Rect::new(lx + 12.0, y + 88.0, 198.0, 26.0);
    ctx.fill_rrect(seg_bg, 9.0, theme::CARD_BG);
    let out_tab = Rect::new(seg_bg.x + 3.0, seg_bg.y + 3.0, 108.0, 20.0);
    let mic_tab = Rect::new(seg_bg.x + 113.0, seg_bg.y + 3.0, 82.0, 20.0);
    if !state.cc_showing_mics {
        ctx.fill_rrect(out_tab, 7.0, theme::VIOLET);
    } else {
        ctx.fill_rrect(mic_tab, 7.0, theme::VIOLET);
    }
    ctx.draw_text_center("Salida de audio", out_tab, TextStyle::Small10Bold, if !state.cc_showing_mics { theme::TEXT_PRIMARY } else { theme::TEXT_SECONDARY });
    ctx.draw_text_center("Micrófono", mic_tab, TextStyle::Small10Bold, if state.cc_showing_mics { theme::TEXT_PRIMARY } else { theme::TEXT_SECONDARY });
    hits.push(HitRegion { id: "cc_tab_out".into(), rect: out_tab, action: Action::SwitchAudioDeviceTab(false), right_action: None, tooltip: None, slider: None });
    hits.push(HitRegion { id: "cc_tab_mic".into(), rect: mic_tab, action: Action::SwitchAudioDeviceTab(true), right_action: None, tooltip: None, slider: None });

    let panel_lnk = Rect::new(lx + cw - 58.0, y + 88.0, 46.0, 26.0);
    ctx.draw_text_right("Panel", panel_lnk, TextStyle::Small10Med, theme::TEXT_TERTIARY);
    hits.push(HitRegion { id: "cc_snd_panel".into(), rect: panel_lnk, action: Action::LaunchTarget("ms-settings:sound".into()), right_action: None, tooltip: None, slider: None });

    let mut dy_dev = y + 120.0;
    if devices.is_empty() {
        ctx.draw_text("Sin dispositivos detectados.", Rect::new(lx + 14.0, dy_dev, cw - 28.0, 28.0), TextStyle::Body11Reg, theme::TEXT_TERTIARY);
    } else {
        for (i, d) in devices.iter().enumerate() {
            let dr = Rect::new(lx + 12.0, dy_dev, cw - 24.0, 38.0);
            let id = format!("cc_dev_{}", i);
            let hov = state.hovered_id.as_deref() == Some(&id);
            let bg = if d.is_default {
                theme::VIOLET_TINT
            } else if hov {
                theme::ELEVATED_HOVER
            } else {
                theme::CARD_BG
            };
            ctx.fill_card(dr, 11.0, bg, if d.is_default { theme::VIOLET_BORDER } else { theme::BORDER });

            let ic_box = Rect::new(dr.x + 8.0, dr.y + 8.0, 22.0, 22.0);
            ctx.fill_rrect(ic_box, 7.0, if d.is_default { theme::VIOLET } else { theme::ELEVATED });
            ctx.draw_vector_icon(
                if state.cc_showing_mics { VectorIcon::Mic } else { VectorIcon::Volume },
                ic_box.x + 11.0,
                ic_box.y + 11.0,
                11.0,
                if d.is_default { theme::TEXT_PRIMARY } else { theme::TEXT_SECONDARY },
            );

            let tw = if d.is_default { dr.w - 94.0 } else { dr.w - 46.0 };
            ctx.draw_text(
                &d.short_name,
                Rect::new(dr.x + 38.0, dr.y + 4.0, tw, 16.0),
                if d.is_default { TextStyle::Body11Med } else { TextStyle::Body11Reg },
                if d.is_default { theme::TEXT_PRIMARY } else { theme::TEXT_SECONDARY },
            );
            if !d.sub_label.is_empty() {
                ctx.draw_text(&d.sub_label, Rect::new(dr.x + 38.0, dr.y + 20.0, tw, 13.0), TextStyle::Small9Reg, theme::TEXT_TERTIARY);
            }
            if d.is_default {
                let badge = Rect::new(dr.x + dr.w - 50.0, dr.y + 10.0, 42.0, 18.0);
                ctx.fill_rrect(badge, 7.0, theme::VIOLET);
                ctx.draw_text_center("Activo", badge, TextStyle::Small9Reg, theme::TEXT_PRIMARY);
            }
            hits.push(HitRegion {
                id,
                rect: dr,
                action: Action::SelectAudioDevice(d.id.clone()),
                right_action: None,
                tooltip: Some(d.full_name.clone()),
                slider: None,
            });
            dy_dev += 43.0;
        }
    }
    y += vol_card_h + 12.0;

    // 4. PER-APP VOLUME MIXER (REAL GROUPED CORE AUDIO!)
    ctx.draw_text("MEZCLADOR POR APLICACIÓN", Rect::new(lx + 2.0, y, cw, 16.0), TextStyle::Small10Bold, theme::TEXT_TERTIARY);
    y += 20.0;
    let mixer_rows = state.app_sessions.len().max(1) as f32;
    let mixer_h = if state.app_sessions.is_empty() { 40.0 } else { 14.0 + mixer_rows * 44.0 };
    ctx.fill_card(Rect::new(lx, y, cw, mixer_h), 16.0, theme::CARD_BG, theme::BORDER);

    if state.app_sessions.is_empty() {
        ctx.draw_text("Sin apps reproduciendo sonido ahora.", Rect::new(lx + 14.0, y + 10.0, cw - 28.0, 20.0), TextStyle::Body11Reg, theme::TEXT_TERTIARY);
    } else {
        let mut my = y + 10.0;
        for (i, sess) in state.app_sessions.iter().enumerate() {
            let ic_r = Rect::new(lx + 12.0, my + 1.0, 18.0, 18.0);
            if sess.exe_path.is_empty() || !ctx.draw_app_icon(&sess.exe_path, ic_r) {
                ctx.fill_rrect(ic_r, 5.0, theme::ELEVATED);
                ctx.draw_vector_icon(VectorIcon::Volume, ic_r.x + 9.0, ic_r.y + 9.0, 10.0, theme::TEXT_SECONDARY);
            }
            ctx.draw_text(&sess.display_name, Rect::new(lx + 38.0, my, cw - 118.0, 20.0), TextStyle::Body11Med, theme::TEXT_PRIMARY);
            let p_str = if sess.is_muted { "MUTE".into() } else { format!("{}%", sess.volume_percent) };
            ctx.draw_text_right(
                &p_str,
                Rect::new(lx + cw - 78.0, my, 42.0, 20.0),
                TextStyle::Mono11Med,
                if sess.is_muted { theme::CORAL } else { theme::TEXT_SECONDARY },
            );

            let mb = Rect::new(lx + cw - 32.0, my, 20.0, 20.0);
            ctx.fill_rrect(mb, 10.0, if sess.is_muted { theme::CORAL } else { theme::ELEVATED });
            ctx.draw_vector_icon(
                if sess.is_muted { VectorIcon::VolumeMute } else { VectorIcon::Volume },
                mb.x + 10.0,
                mb.y + 10.0,
                10.0,
                theme::TEXT_PRIMARY,
            );
            hits.push(HitRegion {
                id: format!("cc_app_mute_{}", i),
                rect: mb,
                action: Action::ToggleAppMute(sess.pids.clone(), !sess.is_muted),
                right_action: None,
                tooltip: None,
                slider: None,
            });

            let sr = Rect::new(lx + 12.0, my + 24.0, cw - 24.0, 9.0);
            ctx.fill_rrect(sr, 4.5, theme::ELEVATED);
            if !sess.is_muted {
                let fw = ((sess.volume_percent as f32 / 100.0) * sr.w).max(8.0);
                ctx.fill_rrect(Rect::new(sr.x, sr.y, fw, 9.0), 4.5, theme::VIOLET);
            }
            hits.push(HitRegion {
                id: format!("cc_app_vol_{}", i),
                rect: Rect::new(sr.x, sr.y - 4.0, sr.w, 17.0),
                action: Action::SetAppVolume(sess.pids.clone(), sess.volume_percent),
                right_action: None,
                tooltip: None,
                slider: Some(SliderDragKind::AppVolume { x: sr.x, w: sr.w, pids: sess.pids.clone() }),
            });
            my += 44.0;
        }
    }
    y += mixer_h + 12.0;

    // 5. 2x2 ANDROID / iOS-STYLE IN-PLACE QUICK TOGGLES (ZERO WINDOWS SETTINGS POPUPS!)
    ctx.draw_text("CONTROLES RÁPIDOS (CLIC ACTIVA · CLIC DER. AJUSTES)", Rect::new(lx + 2.0, y, cw, 15.0), TextStyle::Small9Reg, theme::TEXT_TERTIARY);
    y += 19.0;

    let wifi_active = state.radios.has_internet || state.radios.wifi_on || state.radios.ethernet_up;
    let (net_title, net_icon, net_accent) = match state.radios.net_kind {
        crate::radios::NetKind::Ethernet => ("Ethernet", VectorIcon::Ethernet, theme::EMERALD),
        crate::radios::NetKind::Wifi => ("Wi-Fi", VectorIcon::Wifi, theme::VIOLET),
        crate::radios::NetKind::Offline => ("Sin internet", VectorIcon::NoInternet, theme::CORAL),
    };
    let wifi_sub = if !state.radios.net_label.is_empty() {
        state.radios.net_label.clone()
    } else {
        state.net_status.clone()
    };
    let bt_active = state.radios.bt_on;
    let bt_sub = if !state.radios.bt_available {
        "No disponible".to_string()
    } else if bt_active {
        "Activado · Clic apagar".to_string()
    } else {
        "Desactivado · Clic activar".to_string()
    };
    let kbd_sub = format!(
        "{} · {} idiomas",
        state.keyboard_lang,
        state.keyboard_layouts.len().max(1)
    );

    let tile_w = (cw - 8.0) * 0.5;
    let tiles = [
        (
            "cc_q_net",
            net_title,
            wifi_sub,
            net_icon,
            wifi_active,
            net_accent,
            Action::ToggleWifiRadio,
            Some(Action::LaunchTarget("ms-settings:network".into())),
        ),
        (
            "cc_q_bt",
            "Bluetooth",
            bt_sub,
            VectorIcon::Bluetooth,
            bt_active,
            theme::BLUE,
            Action::ToggleBluetoothRadio,
            Some(Action::LaunchTarget("ms-settings:bluetooth".into())),
        ),
        (
            "cc_q_lang",
            "Teclado",
            kbd_sub,
            VectorIcon::Keyboard,
            state.cc_keyboard_menu_open,
            theme::AMBER,
            Action::ToggleKeyboardMenu,
            Some(Action::CycleKeyboardLayout),
        ),
        (
            "cc_q_studio",
            "Fondos y Widgets",
            "Personalizar escritorio".to_string(),
            VectorIcon::Sparkle,
            false,
            theme::EMERALD,
            Action::ClickModeTab(ShellMode::Studio),
            Some(Action::LaunchTarget("ms-settings:apps-volume".into())),
        ),
        (
            "cc_q_shot",
            "Capturar Pantalla",
            "AuraShot (Impr Pant)".to_string(),
            VectorIcon::Maximize,
            false,
            theme::VIOLET_LIGHT,
            Action::LaunchAuraShot,
            None,
        ),
        (
            "cc_q_clip",
            "Portapapeles",
            format!("{} clips (Win+V)", state.clipboard_items.len()),
            VectorIcon::FileText,
            state.clipboard_open,
            theme::CORAL,
            Action::ToggleClipboard,
            None,
        ),
    ];

    for (idx, (id, title, sub, icon, is_on, accent, act, right_act)) in tiles.into_iter().enumerate() {
        let col = (idx % 2) as f32;
        let row = (idx / 2) as f32;
        let tr = Rect::new(lx + col * (tile_w + 8.0), y + row * 56.0, tile_w, 48.0);
        let hov = state.hovered_id.as_deref() == Some(id);
        let bg = if is_on {
            theme::rgba(accent.r as u8, accent.g as u8, accent.b as u8, 0.22)
        } else if hov {
            theme::ELEVATED_HOVER
        } else {
            theme::ELEVATED
        };
        let border = if is_on {
            windows::Win32::Graphics::Direct2D::Common::D2D1_COLOR_F { a: 0.55, ..accent }
        } else {
            theme::BORDER
        };
        // Smooth Android pill tile fill
        let tile_bg = if is_on {
            windows::Win32::Graphics::Direct2D::Common::D2D1_COLOR_F {
                r: accent.r * 0.28 + 0.08,
                g: accent.g * 0.28 + 0.08,
                b: accent.b * 0.28 + 0.11,
                a: 0.96,
            }
        } else {
            bg
        };
        ctx.fill_card(tr, 14.0, tile_bg, border);

        let ib = Rect::new(tr.x + 10.0, tr.y + 10.0, 28.0, 28.0);
        let offline_tile = id == "cc_q_net" && state.radios.net_kind == crate::radios::NetKind::Offline;
        ctx.fill_rrect(
            ib,
            14.0,
            if offline_tile {
                theme::CORAL_BG
            } else if is_on {
                accent
            } else {
                theme::CARD_BG
            },
        );
        if id == "cc_q_net" && state.radios.net_kind == crate::radios::NetKind::Wifi {
            ctx.draw_wifi_signal(
                ib.x + 14.0,
                ib.y + 13.5,
                13.5,
                state.radios.wifi_bars.max(1),
                theme::TEXT_PRIMARY,
                theme::rgba(255, 255, 255, 0.25),
            );
        } else {
            ctx.draw_vector_icon(
                icon,
                ib.x + 14.0,
                ib.y + 14.0,
                13.0,
                if offline_tile {
                    theme::CORAL
                } else if is_on {
                    theme::TEXT_PRIMARY
                } else {
                    theme::TEXT_SECONDARY
                },
            );
        }
        ctx.draw_text(title, Rect::new(tr.x + 46.0, tr.y + 8.0, tr.w - 52.0, 17.0), TextStyle::Body11Med, theme::TEXT_PRIMARY);
        ctx.draw_text(
            &sub,
            Rect::new(tr.x + 46.0, tr.y + 25.0, tr.w - 52.0, 14.0),
            TextStyle::Small9Reg,
            if is_on { theme::rgba(235, 235, 250, 0.86) } else { theme::TEXT_TERTIARY },
        );
        hits.push(HitRegion {
            id: id.into(),
            rect: tr,
            action: act,
            right_action: right_act,
            tooltip: None,
            slider: None,
        });
    }
    y += 172.0;

    // INLINE KEYBOARD LAYOUT SELECTOR DRAWER (expands seamlessly right inside Control Center!)
    if state.cc_keyboard_menu_open {
        let kh = 16.0 + state.keyboard_layouts.len().max(1) as f32 * 34.0;
        ctx.fill_card(Rect::new(lx, y, cw, kh), 14.0, theme::CARD_BG, theme::VIOLET_BORDER);
        let mut ky = y + 8.0;
        for (i, lay) in state.keyboard_layouts.iter().enumerate() {
            let kr = Rect::new(lx + 8.0, ky, cw - 16.0, 28.0);
            let id = format!("cc_kbd_sel_{}", i);
            let active = if state.active_keyboard_hkl != 0 {
                lay.hkl == state.active_keyboard_hkl
            } else {
                i == 0
            };
            let hov = state.hovered_id.as_deref() == Some(&id);
            ctx.fill_rrect(kr, 9.0, if active { theme::VIOLET } else if hov { theme::ELEVATED } else { theme::rgba(0, 0, 0, 0.0) });
            ctx.draw_text(&lay.short_code, Rect::new(kr.x + 10.0, kr.y, 48.0, 28.0), TextStyle::Mono11Med, theme::TEXT_PRIMARY);
            ctx.draw_text(&lay.label, Rect::new(kr.x + 60.0, kr.y, kr.w - 70.0, 28.0), TextStyle::Body11Med, if active { theme::TEXT_PRIMARY } else { theme::TEXT_SECONDARY });
            hits.push(HitRegion {
                id,
                rect: kr,
                action: Action::SelectKeyboardLayout(lay.hkl),
                right_action: None,
                tooltip: None,
                slider: None,
            });
            ky += 34.0;
        }
        y += kh + 10.0;
    }

    // 6. COMPLETE DYNAMIC BACKGROUND TRAY APPS (4-Column Grid)
    let tray_hdr = format!("APPS EN SEGUNDO PLANO ({} DETECTADAS)", state.tray_apps.len());
    ctx.draw_text(&tray_hdr, Rect::new(lx + 2.0, y, cw, 16.0), TextStyle::Small10Bold, theme::TEXT_TERTIARY);
    y += 20.0;
    let tray_rows = ((state.tray_apps.len().max(1) + 3) / 4) as f32;
    let tray_h = 16.0 + tray_rows * 58.0;
    ctx.fill_card(Rect::new(lx, y, cw, tray_h), 16.0, theme::CARD_BG, theme::BORDER);

    let cell_w = (cw - 24.0) / 4.0;
    for (idx, app) in state.tray_apps.iter().enumerate() {
        let col = (idx % 4) as f32;
        let row = (idx / 4) as f32;
        let cr = Rect::new(lx + 9.0 + col * (cell_w + 2.0), y + 9.0 + row * 58.0, cell_w - 4.0, 52.0);
        let id = format!("cc_tray_{}", idx);
        let hov = state.hovered_id.as_deref() == Some(&id);
        ctx.fill_rrect(cr, 11.0, if hov { theme::ELEVATED_HOVER } else { theme::PILL_SUBTLE });
        let ir = Rect::new(cr.x + (cr.w - 22.0) * 0.5, cr.y + 7.0, 22.0, 22.0);
        if app.exe_path.is_empty() || !ctx.draw_app_icon(&app.exe_path, ir) {
            ctx.fill_rrect(ir, 7.0, theme::VIOLET);
            let ch = app.display_name.chars().next().unwrap_or('A').to_string();
            ctx.draw_text_center(&ch, ir, TextStyle::Small10Bold, theme::TEXT_PRIMARY);
        }
        ctx.draw_text_center(&app.display_name, Rect::new(cr.x + 2.0, cr.y + 32.0, cr.w - 4.0, 15.0), TextStyle::Small9Reg, theme::TEXT_SECONDARY);

        let open_act = if app.main_hwnd != 0 {
            Action::FocusWindow(app.main_hwnd)
        } else if !app.fallback_launch.is_empty() {
            Action::LaunchTarget(app.fallback_launch.clone())
        } else {
            Action::LaunchTarget(app.exe_path.clone())
        };
        hits.push(HitRegion {
            id,
            rect: cr,
            action: open_act,
            right_action: Some(Action::KillBackgroundApp(app.process_key.clone())),
            tooltip: Some(format!("{} · Clic abrir · Clic derecho cerrar", app.display_name)),
            slider: None,
        });
    }
    y += tray_h + 12.0;

    // 7. FOOTER (Ajustes · Actualizar · Restaurar Windows)
    let btn_set = Rect::new(lx, y, 76.0, 30.0);
    ctx.fill_rrect(btn_set, 11.0, if state.hovered_id.as_deref() == Some("cc_set") { theme::ELEVATED_HOVER } else { theme::ELEVATED });
    ctx.draw_text_center("Ajustes", btn_set, TextStyle::Body11Med, theme::TEXT_PRIMARY);
    hits.push(HitRegion { id: "cc_set".into(), rect: btn_set, action: Action::OpenSettings, right_action: None, tooltip: None, slider: None });

    let has_upd = state.update_available_ver.is_some();
    let upd_lbl = state
        .update_available_ver
        .as_ref()
        .map(|v| format!("Actualizar v{}", v))
        .unwrap_or_else(|| "Actualizar".to_string());
    let btn_upd = Rect::new(lx + 82.0, y, 112.0, 30.0);
    ctx.fill_rrect(btn_upd, 11.0, if has_upd { theme::VIOLET } else { theme::ELEVATED });
    ctx.draw_text_center(&upd_lbl, btn_upd, TextStyle::Body11Med, theme::TEXT_PRIMARY);
    hits.push(HitRegion { id: "cc_upd".into(), rect: btn_upd, action: Action::LaunchUpdater, right_action: None, tooltip: None, slider: None });

    let btn_res = Rect::new(lx + cw - 134.0, y, 134.0, 30.0);
    ctx.fill_rrect(btn_res, 11.0, theme::CORAL_BG);
    ctx.draw_text_center("Restaurar Windows", btn_res, TextStyle::Body11Med, theme::TEXT_PRIMARY);
    hits.push(HitRegion { id: "cc_restore".into(), rect: btn_res, action: Action::RestoreWindowsAndExit, right_action: None, tooltip: None, slider: None });

    let total_content_h = (y + scroll) + 46.0;
    ctx.pop_clip();

    if total_content_h > win_h - 32.0 {
        let track_h = win_h - 40.0;
        let thumb_h = ((win_h - 34.0) / total_content_h * track_h).clamp(32.0, track_h);
        let max_s = (total_content_h - (win_h - 34.0)).max(1.0);
        let thumb_y = 20.0 + (scroll / max_s).clamp(0.0, 1.0) * (track_h - thumb_h);
        ctx.fill_rrect(Rect::new(CC_WIDTH - 15.0, thumb_y, 5.0, thumb_h), 2.5, theme::rgb(58, 58, 78));
    }

    ctx.reset_transform();
    if _layer.is_some() {
        ctx.pop_layer();
    }
    total_content_h
}
