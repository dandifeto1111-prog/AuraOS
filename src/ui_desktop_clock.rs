//! Ultra-light Desktop-Layer Clock & Weather over Wallpaper Engine (HWND_BOTTOM)

use crate::renderer::{DrawCtx, Rect, TextStyle};
use crate::theme;
use crate::ui_state::AppState;

pub const CLOCK_WIN_W: f32 = 420.0;
pub const CLOCK_WIN_H: f32 = 166.0;

pub fn render_desktop_clock(ctx: &mut DrawCtx<'_>, state: &AppState) {
    use windows::Win32::System::SystemInformation::GetLocalTime;
    let st = unsafe { GetLocalTime() };
    let time_str = format!("{:02}:{:02}", st.wHour, st.wMinute);
    let dow = match st.wDayOfWeek {
        0 => "Domingo",
        1 => "Lunes",
        2 => "Martes",
        3 => "Miércoles",
        4 => "Jueves",
        5 => "Viernes",
        _ => "Sábado",
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
    let weather_str = format!("{}°C · {} · {}", state.weather.temp_c, state.weather.condition, state.weather.city);

    // Subtle shadow offset for legibility on any bright or dark live wallpaper
    ctx.draw_text_center(&time_str, Rect::new(2.0, 4.0, CLOCK_WIN_W, 86.0), TextStyle::Clock72Light, theme::rgba(0, 0, 0, 0.45));
    ctx.draw_text_center(&time_str, Rect::new(0.0, 2.0, CLOCK_WIN_W, 86.0), TextStyle::Clock72Light, theme::TEXT_PRIMARY);

    ctx.draw_text_center(&date_str, Rect::new(1.0, 91.0, CLOCK_WIN_W, 24.0), TextStyle::Title14Semi, theme::rgba(0, 0, 0, 0.45));
    ctx.draw_text_center(&date_str, Rect::new(0.0, 90.0, CLOCK_WIN_W, 24.0), TextStyle::Title14Semi, theme::TEXT_PRIMARY);

    let pill = Rect::new((CLOCK_WIN_W - 230.0) * 0.5, 122.0, 230.0, 26.0);
    ctx.fill_rrect(pill, 13.0, theme::rgba(14, 14, 20, 0.62));
    ctx.draw_text_center(&weather_str, pill, TextStyle::Small10Med, theme::TEXT_PRIMARY);
}
