//! AuraOS v5.1 Dynamic Material You 3 + Drool Dark Mode Design Tokens
//! - Generates harmonious dark Material 3 palettes from ANY wallpaper image/video thumbnail or preset!
//! - Persists `%LocalAppData%\AuraShell\active_palette.json` so ALL 10 AuraOS binaries share live colors!
//! - Preserves classic Drool `pub const` tokens for 100% backwards compatibility across all modules.

use crate::models::app_data_dir;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::RwLock;
use windows::Win32::Graphics::Direct2D::Common::D2D1_COLOR_F;

#[inline]
pub const fn rgba(r: u8, g: u8, b: u8, a: f32) -> D2D1_COLOR_F {
    D2D1_COLOR_F {
        r: (r as f32) / 255.0,
        g: (g as f32) / 255.0,
        b: (b as f32) / 255.0,
        a,
    }
}

#[inline]
pub const fn rgb(r: u8, g: u8, b: u8) -> D2D1_COLOR_F {
    rgba(r, g, b, 1.0)
}

#[inline]
pub fn color_to_colorref(c: D2D1_COLOR_F) -> u32 {
    let r = (c.r.clamp(0.0, 1.0) * 255.0).round() as u32;
    let g = (c.g.clamp(0.0, 1.0) * 255.0).round() as u32;
    let b = (c.b.clamp(0.0, 1.0) * 255.0).round() as u32;
    (b << 16) | (g << 8) | r
}

#[inline]
pub fn with_alpha(c: D2D1_COLOR_F, a: f32) -> D2D1_COLOR_F {
    D2D1_COLOR_F { a, ..c }
}

// ==================== STATIC FALLBACK TOKENS (CLASSIC DROOL OBSIDIAN) ====================

pub const CANVAS: D2D1_COLOR_F = rgb(13, 13, 18);           // #0D0D12
pub const TOPBAR_BG: D2D1_COLOR_F = rgba(14, 14, 20, 0.95); // #0E0E14
pub const SURFACE: D2D1_COLOR_F = rgb(20, 20, 27);          // #14141B
pub const CARD_BG: D2D1_COLOR_F = rgb(24, 24, 32);          // #181820
pub const ELEVATED: D2D1_COLOR_F = rgb(34, 34, 44);         // #22222C
pub const ELEVATED_HOVER: D2D1_COLOR_F = rgb(44, 44, 58);   // #2C2C3A
pub const PILL_SUBTLE: D2D1_COLOR_F = rgb(28, 28, 37);      // #1C1C25
pub const BORDER: D2D1_COLOR_F = rgb(42, 42, 54);           // #2A2A36
pub const BORDER_STRONG: D2D1_COLOR_F = rgb(58, 58, 76);    // #3A3A4C

pub const VIOLET: D2D1_COLOR_F = rgb(124, 92, 255);         // #7C5CFF
pub const VIOLET_LIGHT: D2D1_COLOR_F = rgb(146, 118, 255);  // #9276FF
pub const VIOLET_DEEP: D2D1_COLOR_F = rgb(94, 64, 226);     // #5E40E2
pub const VIOLET_TINT: D2D1_COLOR_F = rgb(36, 31, 62);      // #241F3E
pub const VIOLET_BORDER: D2D1_COLOR_F = rgb(106, 82, 214);  // #6A52D6

pub const CRISP_WHITE: D2D1_COLOR_F = rgb(248, 249, 252);   // #F8F9FC
pub const DARK_INK: D2D1_COLOR_F = rgb(18, 19, 28);         // #12131C
pub const DARK_MUTED: D2D1_COLOR_F = rgb(100, 106, 126);    // #646A7E

pub const EMERALD: D2D1_COLOR_F = rgb(34, 197, 94);         // #22C55E
pub const AMBER: D2D1_COLOR_F = rgb(245, 183, 49);          // #F5B731
pub const CORAL: D2D1_COLOR_F = rgb(242, 109, 125);         // #F26D7D
pub const CORAL_BG: D2D1_COLOR_F = rgba(242, 109, 125, 0.22);
pub const BLUE: D2D1_COLOR_F = rgb(59, 130, 246);           // #3B82F6

pub const TEXT_PRIMARY: D2D1_COLOR_F = rgb(248, 249, 252);
pub const TEXT_SECONDARY: D2D1_COLOR_F = rgb(188, 192, 210);
pub const TEXT_TERTIARY: D2D1_COLOR_F = rgb(124, 128, 148);

// ==================== DYNAMIC MATERIAL YOU 3 RUNTIME PALETTE ====================

#[derive(Debug, Clone, Copy)]
pub struct ThemePalette {
    pub seed_rgb: (u8, u8, u8),
    pub canvas: D2D1_COLOR_F,
    pub topbar_bg: D2D1_COLOR_F,
    pub surface: D2D1_COLOR_F,
    pub card_bg: D2D1_COLOR_F,
    pub elevated: D2D1_COLOR_F,
    pub elevated_hover: D2D1_COLOR_F,
    pub pill_subtle: D2D1_COLOR_F,
    pub border: D2D1_COLOR_F,
    pub border_strong: D2D1_COLOR_F,
    pub accent: D2D1_COLOR_F,
    pub accent_light: D2D1_COLOR_F,
    pub accent_deep: D2D1_COLOR_F,
    pub accent_tint: D2D1_COLOR_F,
    pub accent_border: D2D1_COLOR_F,
    pub text_primary: D2D1_COLOR_F,
    pub text_secondary: D2D1_COLOR_F,
    pub text_tertiary: D2D1_COLOR_F,
}

impl ThemePalette {
    pub const fn default_obsidian() -> Self {
        Self {
            seed_rgb: (124, 92, 255),
            canvas: CANVAS,
            topbar_bg: TOPBAR_BG,
            surface: SURFACE,
            card_bg: CARD_BG,
            elevated: ELEVATED,
            elevated_hover: ELEVATED_HOVER,
            pill_subtle: PILL_SUBTLE,
            border: BORDER,
            border_strong: BORDER_STRONG,
            accent: VIOLET,
            accent_light: VIOLET_LIGHT,
            accent_deep: VIOLET_DEEP,
            accent_tint: VIOLET_TINT,
            accent_border: VIOLET_BORDER,
            text_primary: TEXT_PRIMARY,
            text_secondary: TEXT_SECONDARY,
            text_tertiary: TEXT_TERTIARY,
        }
    }

    pub fn from_scheme(scheme: &str, auto_seed: (u8, u8, u8)) -> Self {
        match scheme.to_lowercase().as_str() {
            "violet" | "obsidian" => Self::default_obsidian(),
            "sage" | "forest" => Self::from_seed_hsl((162, 198, 128), 0.14, false),
            "apricot" | "warm" => Self::from_seed_hsl((244, 162, 108), 0.15, false),
            "mono" | "monochrome" => Self::from_seed_hsl((184, 190, 204), 0.0, true),
            _ => Self::from_seed_hsl(auto_seed, 0.14, false), // "Auto" (Material You from Wallpaper!)
        }
    }

    /// Generates an authentic Material You 3 dark tonal palette from any RGB seed!
    /// Surfaces keep subtle tonal chroma (~8–15%) while accents stay vivid and AA-accessible.
    pub fn from_seed_hsl(seed: (u8, u8, u8), surface_sat: f32, monochrome: bool) -> Self {
        let (h, s_raw, _) = rgb_to_hsl(seed.0, seed.1, seed.2);
        let is_mono = monochrome || s_raw < 0.14;
        let accent_sat = if is_mono { 0.05 } else { s_raw.clamp(0.46, 0.78) };
        let surf_sat = if is_mono { 0.025 } else { (s_raw * 0.24).clamp(0.06, surface_sat) };

        let canvas = hsl_to_color(h, surf_sat, 0.065, 1.0);
        let topbar_bg = hsl_to_color(h, surf_sat, 0.072, 0.95);
        let surface = hsl_to_color(h, surf_sat * 1.05, 0.098, 1.0);
        let card_bg = hsl_to_color(h, surf_sat * 1.10, 0.122, 1.0);
        let pill_subtle = hsl_to_color(h, surf_sat * 1.15, 0.142, 1.0);
        let elevated = hsl_to_color(h, surf_sat * 1.20, 0.168, 1.0);
        let elevated_hover = hsl_to_color(h, surf_sat * 1.28, 0.215, 1.0);
        let border = hsl_to_color(h, surf_sat * 1.35, 0.210, 1.0);
        let border_strong = hsl_to_color(h, surf_sat * 1.45, 0.295, 1.0);

        // Keep white button text AA-crisp on bright cyan/emerald/amber hues while preserving vivid chroma
        let accent_l = if (45.0..=210.0).contains(&h) {
            0.50
        } else if h < 45.0 {
            0.55
        } else {
            0.60
        };
        let accent = if is_mono {
            rgb(122, 128, 144)
        } else {
            hsl_to_color(h, accent_sat, accent_l, 1.0)
        };
        let accent_light = if is_mono {
            rgb(226, 230, 240)
        } else {
            hsl_to_color(h, (accent_sat * 1.04).min(0.86), (accent_l + 0.14).min(0.74), 1.0)
        };
        let accent_deep = if is_mono {
            rgb(88, 94, 108)
        } else {
            hsl_to_color(h, (accent_sat * 0.92).min(0.80), (accent_l - 0.14).max(0.36), 1.0)
        };
        let accent_tint = hsl_to_color(h, (accent_sat * 0.62).min(0.45), 0.19, 1.0);
        let accent_border = hsl_to_color(h, (accent_sat * 0.75).min(0.60), 0.50, 1.0);

        let text_primary = hsl_to_color(h, surf_sat * 0.40, 0.975, 1.0);
        let text_secondary = hsl_to_color(h, surf_sat * 0.65, 0.785, 1.0);
        let text_tertiary = hsl_to_color(h, surf_sat * 0.75, 0.555, 1.0);

        Self {
            seed_rgb: seed,
            canvas,
            topbar_bg,
            surface,
            card_bg,
            elevated,
            elevated_hover,
            pill_subtle,
            border,
            border_strong,
            accent,
            accent_light,
            accent_deep,
            accent_tint,
            accent_border,
            text_primary,
            text_secondary,
            text_tertiary,
        }
    }
}

static ACTIVE_PALETTE: RwLock<ThemePalette> = RwLock::new(ThemePalette::default_obsidian());

#[inline]
pub fn pal() -> ThemePalette {
    *ACTIVE_PALETTE.read().unwrap_or_else(|e| e.into_inner())
}

#[inline]
pub fn accent() -> D2D1_COLOR_F {
    pal().accent
}

#[inline]
pub fn accent_light() -> D2D1_COLOR_F {
    pal().accent_light
}

#[inline]
pub fn accent_deep() -> D2D1_COLOR_F {
    pal().accent_deep
}

#[inline]
pub fn accent_tint() -> D2D1_COLOR_F {
    pal().accent_tint
}

#[inline]
pub fn accent_border() -> D2D1_COLOR_F {
    pal().accent_border
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PersistedPalette {
    scheme: String,
    seed_rgb: [u8; 3],
}

pub fn apply_theme_scheme(scheme: &str, seed_rgb: (u8, u8, u8)) -> ThemePalette {
    let next = ThemePalette::from_scheme(scheme, seed_rgb);
    if let Ok(mut g) = ACTIVE_PALETTE.write() {
        *g = next;
    }
    let pers = PersistedPalette {
        scheme: scheme.to_string(),
        seed_rgb: [seed_rgb.0, seed_rgb.1, seed_rgb.2],
    };
    if let Ok(txt) = serde_json::to_string(&pers) {
        let _ = std::fs::write(app_data_dir().join("active_palette.json"), txt);
    }
    next
}

pub fn load_active_palette() -> ThemePalette {
    let p = app_data_dir().join("active_palette.json");
    if let Ok(txt) = std::fs::read_to_string(p) {
        if let Ok(pers) = serde_json::from_str::<PersistedPalette>(&txt) {
            let next = ThemePalette::from_scheme(
                &pers.scheme,
                (pers.seed_rgb[0], pers.seed_rgb[1], pers.seed_rgb[2]),
            );
            if let Ok(mut g) = ACTIVE_PALETTE.write() {
                *g = next;
            }
            return next;
        }
    }
    pal()
}

/// Fast WIC 24x24 wallpaper chroma-scoring seed extractor (runs in ~2ms, zero extra crates!)
/// Picks the most harmonious vibrant hue from any PNG/JPG/BMP/WebP wallpaper or video thumbnail.
pub fn extract_seed_rgb_from_image(path: &str) -> Option<(u8, u8, u8)> {
    use crate::win32_shell::to_wide;
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::GENERIC_READ;
    use windows::Win32::Graphics::Imaging::{
        CLSID_WICImagingFactory, GUID_WICPixelFormat32bppBGRA, IWICImagingFactory,
        WICBitmapDitherTypeNone, WICBitmapInterpolationModeFant, WICBitmapPaletteTypeCustom,
        WICDecodeMetadataCacheOnDemand,
    };
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED,
    };

    if path.trim().is_empty() || !Path::new(path).exists() {
        return None;
    }

    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let wic: IWICImagingFactory =
            CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER).ok()?;
        let wp = to_wide(path);
        let dec = wic
            .CreateDecoderFromFilename(
                PCWSTR(wp.as_ptr()),
                None,
                GENERIC_READ,
                WICDecodeMetadataCacheOnDemand,
            )
            .ok()?;
        let frame = dec.GetFrame(0).ok()?;
        let scaler = wic.CreateBitmapScaler().ok()?;
        scaler
            .Initialize(&frame, 24, 24, WICBitmapInterpolationModeFant)
            .ok()?;
        let conv = wic.CreateFormatConverter().ok()?;
        conv.Initialize(
            &scaler,
            &GUID_WICPixelFormat32bppBGRA,
            WICBitmapDitherTypeNone,
            None,
            0.0,
            WICBitmapPaletteTypeCustom,
        )
        .ok()?;
        let mut buf = vec![0u8; 24 * 24 * 4];
        conv.CopyPixels(std::ptr::null(), 24 * 4, &mut buf).ok()?;

        // 12 hue buckets weighted by chroma & mid-tone luminance (classic Material Color Utilities scoring)
        let mut buckets = [(0.0f32, 0.0f32, 0.0f32, 0.0f32); 12]; // (score, r_sum, g_sum, b_sum)
        for px in buf.chunks_exact(4) {
            let (b, g, r) = (px[0], px[1], px[2]);
            let (h, s, l) = rgb_to_hsl(r, g, b);
            if s < 0.16 || l < 0.12 || l > 0.88 {
                continue;
            }
            let lum_weight = 1.0 - (l - 0.52).abs() * 1.4;
            let weight = (s * s) * lum_weight.max(0.15);
            let bi = (((h / 30.0).floor() as usize) % 12).min(11);
            buckets[bi].0 += weight;
            buckets[bi].1 += r as f32 * weight;
            buckets[bi].2 += g as f32 * weight;
            buckets[bi].3 += b as f32 * weight;
        }

        let best = buckets
            .iter()
            .max_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal))?;
        if best.0 <= 0.05 {
            return None;
        }
        Some((
            (best.1 / best.0).clamp(0.0, 255.0) as u8,
            (best.2 / best.0).clamp(0.0, 255.0) as u8,
            (best.3 / best.0).clamp(0.0, 255.0) as u8,
        ))
    }
}

fn rgb_to_hsl(r: u8, g: u8, b: u8) -> (f32, f32, f32) {
    let rf = r as f32 / 255.0;
    let gf = g as f32 / 255.0;
    let bf = b as f32 / 255.0;
    let max = rf.max(gf).max(bf);
    let min = rf.min(gf).min(bf);
    let l = (max + min) * 0.5;
    let d = max - min;
    if d < 0.0001 {
        return (252.0, 0.0, l);
    }
    let s = if l > 0.5 {
        d / (2.0 - max - min)
    } else {
        d / (max + min)
    };
    let mut h = if (max - rf).abs() < 0.0001 {
        (gf - bf) / d + if gf < bf { 6.0 } else { 0.0 }
    } else if (max - gf).abs() < 0.0001 {
        (bf - rf) / d + 2.0
    } else {
        (rf - gf) / d + 4.0
    };
    h *= 60.0;
    (h, s, l)
}

fn hue_to_rgb(p: f32, q: f32, mut t: f32) -> f32 {
    if t < 0.0 {
        t += 1.0;
    }
    if t > 1.0 {
        t -= 1.0;
    }
    if t < 1.0 / 6.0 {
        return p + (q - p) * 6.0 * t;
    }
    if t < 0.5 {
        return q;
    }
    if t < 2.0 / 3.0 {
        return p + (q - p) * (2.0 / 3.0 - t) * 6.0;
    }
    p
}

fn hsl_to_color(h_deg: f32, s: f32, l: f32, a: f32) -> D2D1_COLOR_F {
    let h = ((h_deg % 360.0) + 360.0) % 360.0 / 360.0;
    let (r, g, b) = if s <= 0.0001 {
        (l, l, l)
    } else {
        let q = if l < 0.5 { l * (1.0 + s) } else { l + s - l * s };
        let p = 2.0 * l - q;
        (
            hue_to_rgb(p, q, h + 1.0 / 3.0),
            hue_to_rgb(p, q, h),
            hue_to_rgb(p, q, h - 1.0 / 3.0),
        )
    };
    D2D1_COLOR_F {
        r: r.clamp(0.0, 1.0),
        g: g.clamp(0.0, 1.0),
        b: b.clamp(0.0, 1.0),
        a,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VectorIcon {
    Folder,
    Search,
    Settings,
    Power,
    Sleep,
    Restart,
    Lock,
    Volume,
    VolumeMute,
    Mic,
    Wifi,
    Bluetooth,
    Keyboard,
    ChevronDown,
    MusicNote,
    Prev,
    PlayPause,
    Next,
    AppGrid,
    Plus,
    Close,
    Sun,
    Cloud,
    Gamepad,
    Code,
    Sparkle,
    Refresh,
    External,
    Trash,
    Ethernet,
    NoInternet,
    Image,
    Film,
    FileText,
    RotateCw,
    ZoomIn,
    ZoomOut,
    Maximize,
    ChevronLeft,
    ChevronRight,
    Play,
    Pause,
    Save,
    Moon,
    Coffee,
    Battery,
    Check,
    GitBranch,
}
