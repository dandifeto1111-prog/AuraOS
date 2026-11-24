//! AuraCalendar v5.0 — Interactive Spanish Monthly Calendar + Pomodoro Focus Timer Flyout
//! - Opens from TopBar center clock (`top_clock`), replacing Windows 11's sluggish calendar flyout
//! - Origin-aware popover (`scale(0.95 -> 1.0)` from top-center, 155ms `EASE_OUT` enter / 115ms exit)
//! - Includes built-in 25m / 5m Pomodoro Focus Timer with live countdown!

use crate::renderer::{DrawCtx, Rect, TextStyle};
use crate::theme::{self, VectorIcon};
use crate::ui_state::{Action, AppState, HitRegion};
use windows::Win32::System::SystemInformation::GetLocalTime;

pub const CAL_WIDTH: f32 = 356.0;
pub const CAL_HEIGHT: f32 = 448.0;

fn is_leap(y: i32) -> bool {
    (y % 4 == 0 && y % 100 != 0) || (y % 400 == 0)
}

pub fn days_in_month(year: i32, month: i32) -> i32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => if is_leap(year) { 29 } else { 28 },
        _ => 30,
    }
}

/// Sakamoto's algorithm: returns Monday-first index `0..=6` (`0 = Lun`, `6 = Dom`) for day 1 of `(year, month)`.
fn first_weekday_monday0(mut y: i32, m: i32) -> usize {
    let t = [0, 3, 2, 5, 0, 3, 5, 1, 4, 6, 2, 4];
    if m < 3 {
        y -= 1;
    }
    let dow_sun0 = (y + y / 4 - y / 100 + y / 400 + t[(m - 1).clamp(0, 11) as usize] + 1) % 7;
    ((dow_sun0 + 6) % 7) as usize
}

fn month_name_es(m: i32) -> &'static str {
    match m {
        1 => "Enero",
        2 => "Febrero",
        3 => "Marzo",
        4 => "Abril",
        5 => "Mayo",
        6 => "Junio",
        7 => "Julio",
        8 => "Agosto",
        9 => "Septiembre",
        10 => "Octubre",
        11 => "Noviembre",
        _ => "Diciembre",
    }
}

pub fn render_calendar(ctx: &mut DrawCtx<'_>, state: &AppState, hits: &mut Vec<HitRegion>) {
    hits.clear();
    let t = state.calendar_anim.value.clamp(0.0, 1.0);
    if t <= 0.01 {
        return;
    }

    let ox = CAL_WIDTH * 0.5;
    let oy = 6.0;
    let scale = 0.95 + 0.05 * t;
    let dy = (1.0 - t) * -6.0;
    ctx.set_transform_scale_origin(scale, ox, oy, 0.0, dy);
    let _layer = if t < 0.99 {
        ctx.push_opacity_layer(Rect::new(0.0, 0.0, CAL_WIDTH, CAL_HEIGHT), t)
    } else {
        None
    };

    let outer = Rect::new(6.0, 6.0, CAL_WIDTH - 12.0, CAL_HEIGHT - 12.0);
    ctx.fill_card(outer, 22.0, theme::SURFACE, theme::BORDER_STRONG);

    let st = unsafe { GetLocalTime() };
    let cur_y = if state.cal_year > 0 { state.cal_year } else { st.wYear as i32 };
    let cur_m = if state.cal_month > 0 { state.cal_month } else { st.wMonth as i32 };

    let lx = 22.0;
    let cw = CAL_WIDTH - 44.0;
    let mut y = 20.0;

    // 1. Live Time + Weather Summary Header
    let time_str = format!("{:02}:{:02}:{:02}", st.wHour, st.wMinute, st.wSecond);
    ctx.draw_text(&time_str, Rect::new(lx, y, 160.0, 26.0), TextStyle::Title22Bold, theme::CRISP_WHITE);
    if state.weather.loaded && !state.weather.city.is_empty() {
        let w_txt = format!("{}°C · {}", state.weather.temp_c, state.weather.city);
        ctx.draw_text_right(&w_txt, Rect::new(lx + 150.0, y + 4.0, cw - 150.0, 20.0), TextStyle::Body11Med, theme::VIOLET_LIGHT);
    }
    y += 34.0;

    ctx.draw_line(lx, y, lx + cw, y, theme::BORDER, 1.0);
    y += 12.0;

    // 2. Month Navigation Bar (`◀   Noviembre 2026   Hoy   ▶`)
    let prev_r = Rect::new(lx, y, 28.0, 28.0);
    let p_hov = state.hovered_id.as_deref() == Some("cal_prev");
    ctx.fill_card(prev_r, 8.0, if p_hov { theme::ELEVATED_HOVER } else { theme::ELEVATED }, theme::BORDER);
    ctx.draw_vector_icon(VectorIcon::ChevronLeft, prev_r.x + 14.0, prev_r.y + 14.0, 12.0, theme::TEXT_PRIMARY);
    hits.push(HitRegion { id: "cal_prev".into(), rect: prev_r, action: Action::CalPrevMonth, right_action: None, tooltip: None, slider: None });

    let m_title = format!("{} {}", month_name_es(cur_m), cur_y);
    ctx.draw_text(&m_title, Rect::new(lx + 36.0, y + 3.0, 160.0, 22.0), TextStyle::Title14Semi, theme::TEXT_PRIMARY);

    let today_r = Rect::new(lx + cw - 78.0, y, 44.0, 28.0);
    ctx.fill_card(today_r, 8.0, theme::ELEVATED, theme::BORDER);
    ctx.draw_text_center("Hoy", today_r, TextStyle::Small10Bold, theme::VIOLET_LIGHT);
    hits.push(HitRegion { id: "cal_today".into(), rect: today_r, action: Action::CalToday, right_action: None, tooltip: None, slider: None });

    let next_r = Rect::new(lx + cw - 28.0, y, 28.0, 28.0);
    let n_hov = state.hovered_id.as_deref() == Some("cal_next");
    ctx.fill_card(next_r, 8.0, if n_hov { theme::ELEVATED_HOVER } else { theme::ELEVATED }, theme::BORDER);
    ctx.draw_vector_icon(VectorIcon::ChevronRight, next_r.x + 14.0, next_r.y + 14.0, 12.0, theme::TEXT_PRIMARY);
    hits.push(HitRegion { id: "cal_next".into(), rect: next_r, action: Action::CalNextMonth, right_action: None, tooltip: None, slider: None });
    y += 36.0;

    // 3. Weekday Headers (`LU MA MI JU VI SA DO`)
    let cell_w = cw / 7.0;
    for (i, dw) in ["LU", "MA", "MI", "JU", "VI", "SA", "DO"].iter().enumerate() {
        let r = Rect::new(lx + i as f32 * cell_w, y, cell_w, 18.0);
        ctx.draw_text_center(dw, r, TextStyle::Small9Reg, if i >= 5 { theme::VIOLET_LIGHT } else { theme::TEXT_TERTIARY });
    }
    y += 22.0;

    // 4. Days Grid (6 rows max)
    let start_col = first_weekday_monday0(cur_y, cur_m);
    let total_days = days_in_month(cur_y, cur_m);
    let is_this_month = cur_y == st.wYear as i32 && cur_m == st.wMonth as i32;

    for day in 1..=total_days {
        let slot = start_col + (day - 1) as usize;
        let col = (slot % 7) as f32;
        let row = (slot / 7) as f32;
        let cr = Rect::new(lx + col * cell_w + 3.0, y + row * 31.0, cell_w - 6.0, 27.0);
        let is_today = is_this_month && day == st.wDay as i32;
        if is_today {
            ctx.fill_card(cr, 9.0, theme::VIOLET, theme::VIOLET_LIGHT);
        } else {
            ctx.fill_rrect(cr, 8.0, theme::rgba(255, 255, 255, 0.03));
        }
        ctx.draw_text_center(
            &day.to_string(),
            cr,
            if is_today { TextStyle::Body11Med } else { TextStyle::Body11Reg },
            if is_today { theme::CRISP_WHITE } else { theme::TEXT_PRIMARY },
        );
    }
    y += 6.0 * 31.0 + 10.0;

    // 5. Built-in macOS Focus / Pomodoro Timer Card
    let pomo = Rect::new(lx, y, cw, 92.0);
    ctx.fill_card(pomo, 16.0, theme::CARD_BG, theme::VIOLET_BORDER);

    let mins = state.pomodoro_secs_left / 60;
    let secs = state.pomodoro_secs_left % 60;
    let mode_lbl = if state.pomodoro_is_break {
        "POMODORO · DESCANSO (5M)"
    } else {
        "POMODORO · ENFOQUE PROFUNDO (25M)"
    };
    ctx.draw_text(mode_lbl, Rect::new(pomo.x + 14.0, pomo.y + 10.0, pomo.w - 28.0, 15.0), TextStyle::Small9Reg, theme::VIOLET_LIGHT);
    ctx.draw_text(
        &format!("{:02}:{:02}", mins, secs),
        Rect::new(pomo.x + 14.0, pomo.y + 28.0, 96.0, 28.0),
        TextStyle::Title22Bold,
        theme::CRISP_WHITE,
    );

    let play_r = Rect::new(pomo.x + 114.0, pomo.y + 28.0, 84.0, 28.0);
    ctx.fill_card(
        play_r,
        9.0,
        if state.pomodoro_active { theme::AMBER } else { theme::VIOLET },
        theme::BORDER,
    );
    ctx.draw_text_center(
        if state.pomodoro_active { "Pausar" } else { "Iniciar" },
        play_r,
        TextStyle::Body11Med,
        if state.pomodoro_active { theme::DARK_INK } else { theme::CRISP_WHITE },
    );
    hits.push(HitRegion { id: "pomo_play".into(), rect: play_r, action: Action::PomodoroToggle, right_action: None, tooltip: None, slider: None });

    let rst_r = Rect::new(pomo.x + 204.0, pomo.y + 28.0, 46.0, 28.0);
    ctx.fill_card(rst_r, 9.0, theme::ELEVATED, theme::BORDER);
    ctx.draw_text_center("25m", rst_r, TextStyle::Small10Bold, theme::TEXT_PRIMARY);
    hits.push(HitRegion { id: "pomo_25".into(), rect: rst_r, action: Action::PomodoroMode(false), right_action: None, tooltip: None, slider: None });

    let brk_r = Rect::new(pomo.x + 254.0, pomo.y + 28.0, 44.0, 28.0);
    ctx.fill_card(brk_r, 9.0, theme::ELEVATED, theme::BORDER);
    ctx.draw_text_center("5m", brk_r, TextStyle::Small10Bold, theme::TEXT_SECONDARY);
    hits.push(HitRegion { id: "pomo_5".into(), rect: brk_r, action: Action::PomodoroMode(true), right_action: None, tooltip: None, slider: None });

    ctx.draw_text(
        "Contador en vivo en la barra superior mientras está activo.",
        Rect::new(pomo.x + 14.0, pomo.y + 64.0, pomo.w - 28.0, 16.0),
        TextStyle::Small9Reg,
        theme::TEXT_TERTIARY,
    );

    if _layer.is_some() {
        ctx.pop_layer();
    }
    ctx.reset_transform();
}
