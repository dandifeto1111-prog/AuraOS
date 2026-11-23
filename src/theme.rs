//! Drool Dark Mode Dashboard + Impeccable Design Tokens
//! Matte obsidian surfaces, crisp 1px borders, Electric Violet focal accent, zero AI slop

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

// Core Matte Obsidian Surfaces
pub const CANVAS: D2D1_COLOR_F = rgb(13, 13, 18);           // #0D0D12
pub const TOPBAR_BG: D2D1_COLOR_F = rgba(14, 14, 20, 0.95); // #0E0E14
pub const SURFACE: D2D1_COLOR_F = rgb(20, 20, 27);          // #14141B
pub const CARD_BG: D2D1_COLOR_F = rgb(24, 24, 32);          // #181820
pub const ELEVATED: D2D1_COLOR_F = rgb(34, 34, 44);         // #22222C
pub const ELEVATED_HOVER: D2D1_COLOR_F = rgb(44, 44, 58);   // #2C2C3A
pub const PILL_SUBTLE: D2D1_COLOR_F = rgb(28, 28, 37);      // #1C1C25
pub const BORDER: D2D1_COLOR_F = rgb(42, 42, 54);           // #2A2A36
pub const BORDER_STRONG: D2D1_COLOR_F = rgb(58, 58, 76);    // #3A3A4C

// Focal Accents (Drool)
pub const VIOLET: D2D1_COLOR_F = rgb(124, 92, 255);         // #7C5CFF
pub const VIOLET_LIGHT: D2D1_COLOR_F = rgb(146, 118, 255);  // #9276FF
pub const VIOLET_DEEP: D2D1_COLOR_F = rgb(94, 64, 226);     // #5E40E2
pub const VIOLET_TINT: D2D1_COLOR_F = rgb(36, 31, 62);      // #241F3E
pub const VIOLET_BORDER: D2D1_COLOR_F = rgb(106, 82, 214);  // #6A52D6

// High-contrast Editorial White Card
pub const CRISP_WHITE: D2D1_COLOR_F = rgb(248, 249, 252);   // #F8F9FC
pub const DARK_INK: D2D1_COLOR_F = rgb(18, 19, 28);         // #12131C
pub const DARK_MUTED: D2D1_COLOR_F = rgb(100, 106, 126);    // #646A7E

// Semantic Micro-Accents
pub const EMERALD: D2D1_COLOR_F = rgb(34, 197, 94);         // #22C55E
pub const AMBER: D2D1_COLOR_F = rgb(245, 183, 49);          // #F5B731
pub const CORAL: D2D1_COLOR_F = rgb(242, 109, 125);         // #F26D7D
pub const CORAL_BG: D2D1_COLOR_F = rgba(242, 109, 125, 0.22);
pub const BLUE: D2D1_COLOR_F = rgb(59, 130, 246);           // #3B82F6

// Typography Hierarchy
pub const TEXT_PRIMARY: D2D1_COLOR_F = rgb(248, 249, 252);
pub const TEXT_SECONDARY: D2D1_COLOR_F = rgb(188, 192, 210);
pub const TEXT_TERTIARY: D2D1_COLOR_F = rgb(124, 128, 148);

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
}
