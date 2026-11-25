use crate::paths;
use np_core::settings::Settings;
use np_platform::{log, sys};
use np_ui::glass::{GlassParams, Tier};
use std::sync::Mutex;
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_APP};

pub const WM_PREFS: u32 = WM_APP + 40;

static CUR: Mutex<Option<Settings>> = Mutex::new(None);
static LISTENERS: Mutex<Vec<isize>> = Mutex::new(Vec::new());

pub fn init() {
    let s = std::fs::read_to_string(paths::settings_file())
        .map(|t| Settings::from_json(&t))
        .unwrap_or_default();
    *CUR.lock().unwrap_or_else(|e| e.into_inner()) = Some(s);
}

pub fn get() -> Settings {
    let mut g = CUR.lock().unwrap_or_else(|e| e.into_inner());
    g.get_or_insert_with(Settings::default).clone()
}

pub fn listen(hwnd: HWND) {
    let mut g = LISTENERS.lock().unwrap_or_else(|e| e.into_inner());
    let raw = hwnd.0 as isize;
    if !g.contains(&raw) {
        g.push(raw);
    }
}

pub fn unlisten(hwnd: HWND) {
    let mut g = LISTENERS.lock().unwrap_or_else(|e| e.into_inner());
    let raw = hwnd.0 as isize;
    g.retain(|h| *h != raw);
}

pub fn update(f: impl FnOnce(&mut Settings)) {
    let before = get();
    let mut next = before.clone();
    f(&mut next);
    if next == before {
        return;
    }
    *CUR.lock().unwrap_or_else(|e| e.into_inner()) = Some(next.clone());
    let _ = std::fs::create_dir_all(paths::config_dir());
    if let Err(e) = std::fs::write(paths::settings_file(), next.to_json()) {
        log::line("prefs", &format!("save failed: {e}"));
    }
    broadcast();
}

pub fn broadcast() {
    let list = LISTENERS.lock().unwrap_or_else(|e| e.into_inner()).clone();
    for h in list {
        unsafe {
            let _ = PostMessageW(Some(HWND(h as *mut _)), WM_PREFS, WPARAM(0), LPARAM(0));
        }
    }
}

pub fn auto_tier() -> Tier {
    let c = sys::conditions();
    if !c.transparency || c.battery_saver || c.remote_session {
        Tier::Solid
    } else {
        Tier::Full
    }
}

pub fn glass_params(size: (f32, f32), radius: f32) -> GlassParams {
    GlassParams::from_theme(get().surface, size, radius)
}

pub fn is_vi() -> bool {
    match get().lang {
        np_core::settings::Lang::Vi => true,
        np_core::settings::Lang::En => false,
        np_core::settings::Lang::System => np_platform::startup::system_is_vietnamese(),
    }
}

pub fn tr<'a>(en: &'a str, vi: &'a str) -> &'a str {
    if is_vi() {
        vi
    } else {
        en
    }
}
