#![windows_subsystem = "windows"]

mod day_clock;
mod desktop_widgets;
mod island;
mod paths;
mod popup;
mod prefs;
mod settings_win;
mod taskbar_bar;
mod tour;

use np_platform::{brightness, clipboard, hotkeys, log, taskbar};
use np_ui::host::Host;
use std::sync::atomic::{AtomicIsize, Ordering};
use windows::core::{w, BOOL, PCWSTR};
use windows::Win32::Foundation::{CloseHandle, HANDLE, HINSTANCE, HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows::Win32::System::Console::{
    SetConsoleCtrlHandler, CTRL_BREAK_EVENT, CTRL_CLOSE_EVENT, CTRL_C_EVENT,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::CreateMutexW;
use windows::Win32::UI::HiDpi::{
    SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
};
use windows::Win32::UI::Shell::{
    Shell_NotifyIconW, NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NOTIFYICONDATAW,
};
use windows::Win32::UI::WindowsAndMessaging::*;

const WM_TRAY: u32 = WM_APP + 1;
const ID_SETTINGS: usize = 1;
const ID_TOGGLE_TASKBAR: usize = 2;
const ID_TOGGLE_ISLAND: usize = 3;
const ID_QUIT: usize = 4;
const T_EXIT: usize = 1;

static MAIN_HWND: AtomicIsize = AtomicIsize::new(0);
static COMP: std::sync::Mutex<Option<windows::UI::Composition::Compositor>> =
    std::sync::Mutex::new(None);

pub fn open_tour_window() {
    if let Ok(g) = COMP.lock() {
        if let Some(c) = g.as_ref() {
            tour::open(c);
        }
    }
}

struct Args {
    keep_taskbar: bool,
    open_settings: bool,
    open_tour: bool,
    verify_windows: bool,
    demo_media: bool,
    exit_after: Option<u32>,
}

fn parse_args() -> Args {
    let v: Vec<String> = std::env::args().skip(1).collect();
    let val = |k: &str| v.iter().position(|a| a == k).and_then(|i| v.get(i + 1));
    Args {
        keep_taskbar: v.iter().any(|a| a == "--keep-taskbar"),
        open_settings: v.iter().any(|a| a == "--open-settings"),
        open_tour: v.iter().any(|a| a == "--open-tour"),
        verify_windows: v.iter().any(|a| a == "--verify-windows"),
        demo_media: v.iter().any(|a| a == "--demo-media"),
        exit_after: val("--exit-after").and_then(|s| s.parse().ok()),
    }
}

fn release_all(why: &str) {
    log::line("exit", why);
    brightness::restore();
    taskbar::restore();
}

struct ExitGuard;
impl Drop for ExitGuard {
    fn drop(&mut self) {
        release_all("drop");
    }
}

unsafe extern "system" fn ctrl_handler(t: u32) -> BOOL {
    if t == CTRL_C_EVENT || t == CTRL_BREAK_EVENT || t == CTRL_CLOSE_EVENT {
        release_all("ctrl");
    }
    BOOL(0)
}

fn open_settings() {
    if let Ok(g) = COMP.lock() {
        if let Some(c) = g.as_ref() {
            settings_win::open(c, request_quit);
        }
    }
}

fn request_quit() {
    let h = MAIN_HWND.load(Ordering::Relaxed);
    if h != 0 {
        unsafe {
            let _ = PostMessageW(
                Some(HWND(h as *mut _)),
                WM_COMMAND,
                WPARAM(ID_QUIT),
                LPARAM(0),
            );
        }
    }
}

fn app_icon() -> HICON {
    unsafe {
        let Ok(module) = GetModuleHandleW(None) else { return HICON::default() };
        LoadIconW(Some(module.into()), PCWSTR(1usize as *const u16)).unwrap_or_default()
    }
}

fn add_tray(hwnd: HWND, icon: HICON) {
    let mut nid = NOTIFYICONDATAW {
        cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: hwnd,
        uID: 1,
        uFlags: NIF_MESSAGE | NIF_ICON | NIF_TIP,
        uCallbackMessage: WM_TRAY,
        hIcon: icon,
        ..Default::default()
    };
    for (i, c) in "Lumina".encode_utf16().enumerate() {
        nid.szTip[i] = c;
    }
    unsafe {
        let _ = Shell_NotifyIconW(NIM_ADD, &nid);
    }
}

fn remove_tray(hwnd: HWND) {
    let nid = NOTIFYICONDATAW {
        cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: hwnd,
        uID: 1,
        ..Default::default()
    };
    unsafe {
        let _ = Shell_NotifyIconW(NIM_DELETE, &nid);
    }
}

fn show_tray_menu(hwnd: HWND) {
    unsafe {
        let Ok(menu) = CreatePopupMenu() else { return };
        let tb_flag = if prefs::get().taskbar_enabled {
            MF_CHECKED
        } else {
            MF_UNCHECKED
        };
        let is_flag = if prefs::get().island_enabled {
            MF_CHECKED
        } else {
            MF_UNCHECKED
        };
        let tr = prefs::tr;
        let to_w = |s: &str| -> Vec<u16> { s.encode_utf16().chain(std::iter::once(0)).collect() };
        let w_set = to_w(tr("Lumina Settings…", "Cài đặt Lumina…"));
        let w_isl = to_w(tr("Enable Dynamic Island", "Bật Dynamic Island"));
        let w_tb = to_w(tr("Lumina Glass Taskbar", "Thanh tác vụ Glass Lumina"));
        let w_quit = to_w(tr("Quit Lumina", "Thoát Lumina"));
        let _ = AppendMenuW(menu, MF_STRING | MF_DEFAULT, ID_SETTINGS, PCWSTR(w_set.as_ptr()));
        let _ = AppendMenuW(menu, MF_STRING | is_flag, ID_TOGGLE_ISLAND, PCWSTR(w_isl.as_ptr()));
        let _ = AppendMenuW(menu, MF_STRING | tb_flag, ID_TOGGLE_TASKBAR, PCWSTR(w_tb.as_ptr()));
        let _ = AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null());
        let _ = AppendMenuW(menu, MF_STRING, ID_QUIT, PCWSTR(w_quit.as_ptr()));
        let mut pt = POINT::default();
        let _ = GetCursorPos(&mut pt);
        let _ = SetForegroundWindow(hwnd);
        let _ = TrackPopupMenu(
            menu,
            TPM_RIGHTBUTTON | TPM_BOTTOMALIGN,
            pt.x,
            pt.y,
            Some(0),
            hwnd,
            None,
        );
        let _ = DestroyMenu(menu);
    }
}

unsafe extern "system" fn main_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_TRAY => {
            match (lp.0 as u32) & 0xFFFF {
                WM_LBUTTONUP => open_settings(),
                WM_RBUTTONUP | WM_CONTEXTMENU => show_tray_menu(hwnd),
                _ => {}
            }
            LRESULT(0)
        }
        WM_HOTKEY => {
            if prefs::get().hotkeys_enabled {
                match wp.0 as i32 {
                    hotkeys::HK_TOGGLE_ISLAND => island::toggle_open(),
                    hotkeys::HK_MUTE_MIC => island::toggle_mic_mute(),
                    hotkeys::HK_CLIPBOARD => island::open_clipboard_tab(),
                    _ => {}
                }
            }
            LRESULT(0)
        }
        WM_COMMAND => {
            match wp.0 & 0xFFFF {
                ID_SETTINGS => open_settings(),
                ID_TOGGLE_ISLAND => prefs::update(|s| s.island_enabled = !s.island_enabled),
                ID_TOGGLE_TASKBAR => prefs::update(|s| s.taskbar_enabled = !s.taskbar_enabled),
                ID_QUIT => {
                    let _ = DestroyWindow(hwnd);
                }
                _ => {}
            }
            LRESULT(0)
        }
        WM_TIMER if wp.0 == T_EXIT => {
            let _ = KillTimer(Some(hwnd), T_EXIT);
            settings_win::hide_if_open();
            let _ = DestroyWindow(hwnd);
            LRESULT(0)
        }
        WM_ENDSESSION => {
            if wp.0 != 0 {
                release_all("endsession");
            }
            LRESULT(0)
        }
        WM_DESTROY => {
            hotkeys::unregister_all(hwnd);
            remove_tray(hwnd);
            release_all("destroy");
            PostQuitMessage(0);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

fn main() {
    let args = parse_args();
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }
    log::init(paths::log_file());
    let mutex: Option<HANDLE> = unsafe {
        CreateMutexW(None, true, w!("Local\\LuminaRS_SingleInstance")).ok()
    };
    if unsafe { windows::Win32::Foundation::GetLastError() }
        == windows::Win32::Foundation::ERROR_ALREADY_EXISTS
    {
        if let Some(m) = mutex {
            unsafe {
                let _ = CloseHandle(m);
            }
        }
        return;
    }

    taskbar::recover_leftover(&paths::marker_file());
    std::panic::set_hook(Box::new(|info| {
        log::line("panic", &info.to_string());
        release_all("panic");
    }));
    unsafe {
        let _ = SetConsoleCtrlHandler(Some(ctrl_handler), true);
    }
    let _exit_guard = ExitGuard;

    unsafe {
        let _ = windows::Win32::System::Com::CoInitializeEx(
            None,
            windows::Win32::System::Com::COINIT_APARTMENTTHREADED,
        );
    }

    prefs::init();
    let host = match Host::new() {
        Ok(h) => h,
        Err(e) => {
            log::line("host", &format!("failed: {e}"));
            return;
        }
    };
    if let Ok(mut g) = COMP.lock() {
        *g = Some(host.compositor.clone());
    }

    let main_hwnd = unsafe {
        let Ok(hi) = GetModuleHandleW(None) else { return };
        let hi: HINSTANCE = hi.into();
        let class = w!("LuminaMain");
        RegisterClassW(&WNDCLASSW {
            hIcon: crate::app_icon(),
            lpfnWndProc: Some(main_proc),
            hInstance: hi,
            lpszClassName: class,
            ..Default::default()
        });
        let Ok(h) = CreateWindowExW(
            WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
            class,
            w!("Lumina"),
            WS_POPUP,
            0,
            0,
            0,
            0,
            None,
            None,
            Some(hi),
            None,
        ) else {
            return;
        };
        h
    };
    MAIN_HWND.store(main_hwnd.0 as isize, Ordering::Relaxed);
    let tray_icon = app_icon();
    add_tray(main_hwnd, tray_icon);
    hotkeys::register_all(main_hwnd);

    let _ = clipboard::init();
    clipboard::set_enabled(prefs::get().clipboard_history);
    let _ = desktop_widgets::init(&host.compositor, open_settings);
    let _ = day_clock::init(&host.compositor, open_settings);
    let _ = island::init(&host.compositor, args.demo_media, open_settings);
    let _ = taskbar_bar::init(&host.compositor, !args.keep_taskbar, open_settings);

    let first_run = !paths::settings_file().exists();
    if args.open_settings || args.verify_windows {
        settings_win::open(&host.compositor, request_quit);
    }
    if args.open_tour || (first_run && !args.verify_windows) {
        tour::open(&host.compositor);
    }
    if args.verify_windows {
        let _ = desktop_widgets::present_all();
        unsafe {
            SetTimer(Some(main_hwnd), T_EXIT, 1200, None);
        }
    } else if let Some(secs) = args.exit_after {
        unsafe {
            SetTimer(Some(main_hwnd), T_EXIT, secs.max(1) * 1000, None);
        }
    }

    unsafe {
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
    clipboard::shutdown();
    release_all("normal-exit");
    tour::shutdown();
    settings_win::shutdown();
    desktop_widgets::shutdown();
    day_clock::shutdown();
    island::shutdown();
    taskbar_bar::shutdown();
    np_ui::gfx::Gfx::clear_shared();
    if let Ok(mut g) = COMP.lock() {
        *g = None;
    }
    host.shutdown();
    unsafe {
        windows::Win32::System::Com::CoUninitialize();
    }
}

