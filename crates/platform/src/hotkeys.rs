use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    RegisterHotKey, UnregisterHotKey, HOT_KEY_MODIFIERS, MOD_ALT, MOD_CONTROL, MOD_NOREPEAT,
};

pub const HK_TOGGLE_ISLAND: i32 = 101;
pub const HK_MUTE_MIC: i32 = 102;
pub const HK_CLIPBOARD: i32 = 103;

pub fn register_all(hwnd: HWND) {
    unsafe {
        let mods = HOT_KEY_MODIFIERS(MOD_CONTROL.0 | MOD_ALT.0 | MOD_NOREPEAT.0);
        let _ = RegisterHotKey(Some(hwnd), HK_TOGGLE_ISLAND, mods, 'N' as u32);
        let _ = RegisterHotKey(Some(hwnd), HK_MUTE_MIC, mods, 'M' as u32);
        let _ = RegisterHotKey(Some(hwnd), HK_CLIPBOARD, mods, 'V' as u32);
    }
}

pub fn unregister_all(hwnd: HWND) {
    unsafe {
        let _ = UnregisterHotKey(Some(hwnd), HK_TOGGLE_ISLAND);
        let _ = UnregisterHotKey(Some(hwnd), HK_MUTE_MIC);
        let _ = UnregisterHotKey(Some(hwnd), HK_CLIPBOARD);
    }
}
