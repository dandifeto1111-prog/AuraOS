//! Windows Power Menu Popover (Apple Circular Reveal anchored at top-right Power Icon!)

use crate::renderer::{DrawCtx, Rect, TextStyle};
use crate::theme::{self, VectorIcon};
use crate::ui_state::{Action, AppState, HitRegion};

pub const POWER_MENU_W: f32 = 256.0;
pub const POWER_MENU_H: f32 = 294.0;

pub fn render_power_menu(ctx: &mut DrawCtx<'_>, state: &AppState, hits: &mut Vec<HitRegion>) {
    hits.clear();
    let t = state.power_anim.value.clamp(0.0, 1.0);
    if t <= 0.01 {
        return;
    }

    let ox = POWER_MENU_W - 18.0;
    let oy = 12.0;
    let max_r = (POWER_MENU_W * POWER_MENU_W + POWER_MENU_H * POWER_MENU_H).sqrt() + 18.0;
    let cur_r = 16.0 + (max_r - 16.0) * t;
    let alpha = (t * 2.2).clamp(0.0, 1.0);

    let full = Rect::new(0.0, 0.0, POWER_MENU_W, POWER_MENU_H);
    let _layer = if t < 0.99 {
        ctx.push_circle_mask_layer(full, ox, oy, cur_r, alpha)
    } else {
        ctx.push_opacity_layer(full, 1.0)
    };

    let scale = 0.94 + 0.06 * t;
    let dy = (1.0 - t) * -5.0;
    ctx.set_transform_scale_origin(scale, ox, oy, 0.0, dy);

    let card = Rect::new(8.0, 6.0, POWER_MENU_W - 16.0, POWER_MENU_H - 14.0);
    ctx.fill_card(card, 18.0, theme::SURFACE, theme::BORDER_STRONG);

    // Unified macOS Traffic Lights in Power Menu header
    let p_lights = [
        ("pm_tl_close", 24.0f32, theme::CORAL, "×", Action::CloseAllOverlays),
        ("pm_tl_sleep", 40.0f32, theme::AMBER, "−", Action::PowerSleep),
        ("pm_tl_cfg", 56.0f32, theme::EMERALD, "+", Action::LaunchAuraSettings(None)),
    ];
    let pm_tl_hov = matches!(
        state.hovered_id.as_deref(),
        Some("pm_tl_close") | Some("pm_tl_sleep") | Some("pm_tl_cfg")
    );
    for (id, cx, col, glyph, act) in p_lights {
        ctx.fill_circle(cx, 25.0, if pm_tl_hov { 5.4 } else { 4.8 }, col);
        if pm_tl_hov {
            ctx.draw_text_center(glyph, Rect::new(cx - 5.5, 18.5, 11.0, 12.0), TextStyle::Small9Reg, theme::DARK_INK);
        }
        hits.push(HitRegion { id: id.into(), rect: Rect::new(cx - 7.5, 17.0, 15.0, 16.0), action: act, right_action: None, tooltip: None, slider: None });
    }
    ctx.draw_text("ENERGÍA DEL SISTEMA", Rect::new(72.0, 17.0, 160.0, 18.0), TextStyle::Small10Bold, theme::TEXT_TERTIARY);

    let mut y = 42.0;
    let rows: [(&str, &str, &str, VectorIcon, windows::Win32::Graphics::Direct2D::Common::D2D1_COLOR_F, Action); 5] = [
        ("pwr_sleep", "Suspender", "Ahorro rápido de energía", VectorIcon::Sleep, theme::VIOLET, Action::PowerSleep),
        ("pwr_lock", "Bloquear con AuraLock", "Pantalla de bloqueo Aura (Ctrl+Alt+L)", VectorIcon::Lock, theme::BLUE, Action::LaunchAuraLock),
        (
            "pwr_restart",
            if state.power_confirm == Some("restart") { "Clic otra vez para reiniciar" } else { "Reiniciar PC" },
            if state.power_confirm == Some("restart") { "Confirmación de seguridad" } else { "Cierra sesiones y reinicia" },
            VectorIcon::Restart,
            theme::AMBER,
            Action::PowerRestartConfirm,
        ),
        (
            "pwr_shutdown",
            if state.power_confirm == Some("shutdown") { "Clic otra vez para apagar" } else { "Apagar PC" },
            if state.power_confirm == Some("shutdown") { "Confirmación de seguridad" } else { "Apagado seguro del equipo" },
            VectorIcon::Power,
            theme::CORAL,
            Action::PowerShutdownConfirm,
        ),
        ("pwr_restore", "Restaurar Windows", "Salir de AuraOS (Ctrl+Alt+W)", VectorIcon::Refresh, theme::TEXT_SECONDARY, Action::RestoreWindowsAndExit),
    ];

    for (idx, (id, title, sub, icon, accent, action)) in rows.into_iter().enumerate() {
        if idx == 4 {
            ctx.draw_line(22.0, y + 2.0, POWER_MENU_W - 22.0, y + 2.0, theme::BORDER, 1.0);
            y += 10.0;
        }
        let r = Rect::new(16.0, y, POWER_MENU_W - 32.0, 40.0);
        let hov = state.hovered_id.as_deref() == Some(id);
        let is_confirm = (id == "pwr_shutdown" && state.power_confirm == Some("shutdown"))
            || (id == "pwr_restart" && state.power_confirm == Some("restart"));
        let bg = if is_confirm {
            theme::CORAL_BG
        } else if hov {
            theme::ELEVATED
        } else {
            theme::rgba(0, 0, 0, 0.0)
        };
        ctx.fill_rrect(r, 11.0, bg);
        if is_confirm {
            ctx.stroke_rrect(r, 11.0, theme::CORAL, 1.0);
        }

        let ib = Rect::new(r.x + 8.0, r.y + 7.0, 26.0, 26.0);
        ctx.fill_rrect(ib, 8.0, windows::Win32::Graphics::Direct2D::Common::D2D1_COLOR_F { a: 0.18, ..accent });
        ctx.draw_vector_icon(icon, ib.x + 13.0, ib.y + 13.0, 12.5, accent);

        ctx.draw_text(
            title,
            Rect::new(r.x + 42.0, r.y + 4.0, r.w - 48.0, 17.0),
            TextStyle::Body12Semi,
            if is_confirm { theme::CORAL } else { theme::TEXT_PRIMARY },
        );
        ctx.draw_text(sub, Rect::new(r.x + 42.0, r.y + 21.0, r.w - 48.0, 14.0), TextStyle::Small9Reg, theme::TEXT_TERTIARY);

        hits.push(HitRegion { id: id.into(), rect: r, action, right_action: None, tooltip: None, slider: None });
        y += 44.0;
    }

    ctx.reset_transform();
    if _layer.is_some() {
        ctx.pop_layer();
    }
}
