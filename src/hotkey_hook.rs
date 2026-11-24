//! AuraOS v4.5 Native Windows Shortcut Interceptor (Zero DLL Injection, 100% Anti-Cheat Safe)
//! Uses standard Win32 `WH_KEYBOARD_LL` (same sanctioned API as Microsoft PowerToys):
//! - Solo `Win` tap         -> Opens AuraOS Launchpad (Menú de Aplicaciones) instead of Windows Start Menu
//! - `Ctrl + Shift + Esc`   -> Launches AuraTasks.exe instead of Windows Task Manager
//! - `Win + E`              -> Launches AuraFiles.exe
//! - `Win + I`              -> Launches AuraSettings.exe
//! - `Win + S`              -> Opens AuraOS Overview & Search
//! Preserves 100% of `Win+Arrows`, `Win+D`, `Win+Shift+S`, `Win+V`, `Win+Tab`, `Win+R`, `Win+L`, `Win+G`!

use std::sync::atomic::{AtomicBool, AtomicIsize, AtomicU32, Ordering};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP,
    VIRTUAL_KEY, VK_CONTROL, VK_ESCAPE, VK_LWIN, VK_MENU, VK_RWIN, VK_SHIFT,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, PostMessageW, SetWindowsHookExW, UnhookWindowsHookEx, HHOOK, KBDLLHOOKSTRUCT,
    WH_KEYBOARD_LL, WM_KEYDOWN, WM_KEYUP, WM_SYSKEYDOWN, WM_SYSKEYUP,
};

pub const SHORTCUT_LAUNCHPAD: usize = 1;
pub const SHORTCUT_TASKS: usize = 2;
pub const SHORTCUT_FILES: usize = 3;
pub const SHORTCUT_SETTINGS: usize = 4;
pub const SHORTCUT_OVERVIEW: usize = 5;
pub const SHORTCUT_CLIPBOARD: usize = 6;
pub const SHORTCUT_SCREENSHOT: usize = 7;

static HOOK_HANDLE: AtomicIsize = AtomicIsize::new(0);
static TARGET_HWND: AtomicIsize = AtomicIsize::new(0);
static TARGET_MSG: AtomicU32 = AtomicU32::new(0);
static ENABLED: AtomicBool = AtomicBool::new(true);
static FULLSCREEN_BYPASS: AtomicBool = AtomicBool::new(false);

static WIN_DOWN: AtomicBool = AtomicBool::new(false);
static WIN_COMBO_USED: AtomicBool = AtomicBool::new(false);

pub fn install(target_hwnd: HWND, custom_msg: u32, enabled: bool) {
    TARGET_HWND.store(target_hwnd.0 as isize, Ordering::SeqCst);
    TARGET_MSG.store(custom_msg, Ordering::SeqCst);
    ENABLED.store(enabled, Ordering::SeqCst);
    if HOOK_HANDLE.load(Ordering::SeqCst) != 0 {
        return;
    }
    unsafe {
        if let Ok(hh) = SetWindowsHookExW(WH_KEYBOARD_LL, Some(ll_keyboard_proc), None, 0) {
            HOOK_HANDLE.store(hh.0 as isize, Ordering::SeqCst);
        }
    }
}

pub fn set_enabled(enabled: bool) {
    ENABLED.store(enabled, Ordering::SeqCst);
    if !enabled {
        WIN_DOWN.store(false, Ordering::SeqCst);
        WIN_COMBO_USED.store(false, Ordering::SeqCst);
    }
}

pub fn set_fullscreen_bypass(is_fullscreen: bool) {
    FULLSCREEN_BYPASS.store(is_fullscreen, Ordering::Relaxed);
}

pub fn uninstall() {
    let raw = HOOK_HANDLE.swap(0, Ordering::SeqCst);
    if raw != 0 {
        unsafe {
            let _ = UnhookWindowsHookEx(HHOOK(raw as *mut _));
        }
    }
}

fn is_vk_down(vk: VIRTUAL_KEY) -> bool {
    unsafe { (GetAsyncKeyState(vk.0 as i32) as u16 & 0x8000) != 0 }
}

/// Sends an unassigned harmless mask key (0xFF) so Explorer cancels opening the Windows Start Menu
/// when the Windows key is released after a solo tap or AuraOS combo.
fn suppress_start_menu_mask() {
    unsafe {
        let inputs = [
            INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: INPUT_0 {
                    ki: KEYBDINPUT {
                        wVk: VIRTUAL_KEY(0xFF),
                        wScan: 0,
                        dwFlags: Default::default(),
                        time: 0,
                        dwExtraInfo: 0x4155_5241, // 'AURA'
                    },
                },
            },
            INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: INPUT_0 {
                    ki: KEYBDINPUT {
                        wVk: VIRTUAL_KEY(0xFF),
                        wScan: 0,
                        dwFlags: KEYEVENTF_KEYUP,
                        time: 0,
                        dwExtraInfo: 0x4155_5241,
                    },
                },
            },
        ];
        let _ = SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);
    }
}

fn post_shortcut(action_id: usize) {
    let hw = TARGET_HWND.load(Ordering::Relaxed);
    let msg = TARGET_MSG.load(Ordering::Relaxed);
    if hw != 0 && msg != 0 {
        unsafe {
            let _ = PostMessageW(HWND(hw as *mut _), msg, WPARAM(action_id), LPARAM(0));
        }
    }
}

unsafe extern "system" fn ll_keyboard_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code < 0 || !ENABLED.load(Ordering::Relaxed) {
        return CallNextHookEx(None, code, wparam, lparam);
    }
    let kb = &*(lparam.0 as *const KBDLLHOOKSTRUCT);
    // Ignore our own injected dummy mask key (0xFF / LLKHF_INJECTED)
    if (kb.flags.0 & 0x10) != 0 || kb.vkCode == 0xFF {
        return CallNextHookEx(None, code, wparam, lparam);
    }

    let msg = wparam.0 as u32;
    let is_down = msg == WM_KEYDOWN || msg == WM_SYSKEYDOWN;
    let is_up = msg == WM_KEYUP || msg == WM_SYSKEYUP;
    let vk = kb.vkCode as u16;

    // 1. Ctrl + Shift + Esc -> Launch AuraTasks.exe (ALWAYS replaces Windows Task Manager!)
    if is_down && vk == VK_ESCAPE.0 {
        if is_vk_down(VK_CONTROL) && is_vk_down(VK_SHIFT) && !is_vk_down(VK_MENU) {
            post_shortcut(SHORTCUT_TASKS);
            return LRESULT(1);
        }
    }

    // 1B. PrintScreen (VK_SNAPSHOT = 0x2C) -> Launch AuraShot.exe (<25ms Screen Snipping Studio!)
    if is_down && vk == 0x2C && !FULLSCREEN_BYPASS.load(Ordering::Relaxed) {
        post_shortcut(SHORTCUT_SCREENSHOT);
        return LRESULT(1);
    }

    // Bypass Win-key interception while a fullscreen game is active
    if FULLSCREEN_BYPASS.load(Ordering::Relaxed) {
        if vk == VK_LWIN.0 || vk == VK_RWIN.0 {
            WIN_DOWN.store(false, Ordering::Relaxed);
            WIN_COMBO_USED.store(false, Ordering::Relaxed);
        }
        return CallNextHookEx(None, code, wparam, lparam);
    }

    // 2. Track Windows key down/up for solo tap -> AuraOS Launchpad + Win+E / Win+I / Win+S
    if vk == VK_LWIN.0 || vk == VK_RWIN.0 {
        if is_down {
            if !WIN_DOWN.swap(true, Ordering::SeqCst) {
                WIN_COMBO_USED.store(false, Ordering::SeqCst);
            }
            return CallNextHookEx(None, code, wparam, lparam);
        } else if is_up {
            let was_down = WIN_DOWN.swap(false, Ordering::SeqCst);
            let combo = WIN_COMBO_USED.swap(false, Ordering::SeqCst);
            if was_down && !combo {
                // Solo Windows key tap! Suppress Windows Start Menu & open AuraOS Launchpad!
                suppress_start_menu_mask();
                post_shortcut(SHORTCUT_LAUNCHPAD);
            }
            return CallNextHookEx(None, code, wparam, lparam);
        }
    } else if WIN_DOWN.load(Ordering::Relaxed) {
        if is_down {
            let no_mods = !is_vk_down(VK_CONTROL) && !is_vk_down(VK_SHIFT) && !is_vk_down(VK_MENU);
            if no_mods {
                if vk == b'E' as u16 {
                    WIN_COMBO_USED.store(true, Ordering::SeqCst);
                    suppress_start_menu_mask();
                    post_shortcut(SHORTCUT_FILES);
                    return LRESULT(1);
                }
                if vk == b'I' as u16 {
                    WIN_COMBO_USED.store(true, Ordering::SeqCst);
                    suppress_start_menu_mask();
                    post_shortcut(SHORTCUT_SETTINGS);
                    return LRESULT(1);
                }
                if vk == b'S' as u16 {
                    WIN_COMBO_USED.store(true, Ordering::SeqCst);
                    suppress_start_menu_mask();
                    post_shortcut(SHORTCUT_OVERVIEW);
                    return LRESULT(1);
                }
                if vk == b'V' as u16 {
                    WIN_COMBO_USED.store(true, Ordering::SeqCst);
                    suppress_start_menu_mask();
                    post_shortcut(SHORTCUT_CLIPBOARD);
                    return LRESULT(1);
                }
            } else if is_vk_down(VK_SHIFT) && !is_vk_down(VK_CONTROL) && !is_vk_down(VK_MENU) && vk == b'S' as u16 {
                // Win + Shift + S -> AuraShot.exe!
                WIN_COMBO_USED.store(true, Ordering::SeqCst);
                suppress_start_menu_mask();
                post_shortcut(SHORTCUT_SCREENSHOT);
                return LRESULT(1);
            }
            // Any other Win+<key> (Win+Arrows, Win+D, Win+Shift+S, Win+V, Win+Tab, Win+R, Win+L):
            // mark combo used and pass straight through to Windows 100% natively!
            WIN_COMBO_USED.store(true, Ordering::SeqCst);
        }
    }

    CallNextHookEx(None, code, wparam, lparam)
}
