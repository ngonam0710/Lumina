use np_core::dock_layout::WinInfo;
use np_core::marker::{Marker, Rect};
use std::ffi::c_void;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, MutexGuard};
use windows::core::{w, BOOL, HSTRING, PCWSTR};
use windows::Win32::Foundation::{CloseHandle, HWND, LPARAM, RECT};
use windows::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_CLOAKED, DWMWA_EXTENDED_FRAME_BOUNDS};
use windows::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::Accessibility::{SetWinEventHook, UnhookWinEvent, HWINEVENTHOOK};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, VK_LWIN,
};
use windows::Win32::UI::Shell::{
    SHAppBarMessage, SHQueryUserNotificationState, ShellExecuteW, ABM_GETSTATE, ABM_GETTASKBARPOS,
    ABM_SETSTATE, APPBARDATA, QUNS_RUNNING_D3D_FULL_SCREEN,
};
use windows::Win32::UI::WindowsAndMessaging::*;

const PRIMARY_CLASS: &str = "Shell_TrayWnd";
const SECONDARY_CLASS: &str = "Shell_SecondaryTrayWnd";
const PARK_X: i32 = -10000;
const ABS_AUTOHIDE: i32 = 1;
const ABS_ALWAYSONTOP: i32 = 2;

static HIDDEN: AtomicBool = AtomicBool::new(false);
static GUARD_STARTED: Mutex<bool> = Mutex::new(false);

struct Active {
    marker: Marker,
    path: PathBuf,
}
static ACTIVE: Mutex<Option<Active>> = Mutex::new(None);
static HOOKS: Mutex<Vec<isize>> = Mutex::new(Vec::new());

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

pub fn is_hidden() -> bool {
    HIDDEN.load(Ordering::Relaxed)
}

fn class_of(hwnd: HWND) -> String {
    let mut buf = [0u16; 64];
    let n = unsafe { GetClassNameW(hwnd, &mut buf) };
    String::from_utf16_lossy(&buf[..n.max(0) as usize])
}

fn primary() -> Option<HWND> {
    unsafe { FindWindowW(w!("Shell_TrayWnd"), None).ok() }
}

fn secondaries() -> Vec<HWND> {
    unsafe extern "system" fn each(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let out = &mut *(lparam.0 as *mut Vec<HWND>);
        if class_of(hwnd) == SECONDARY_CLASS {
            out.push(hwnd);
        }
        BOOL(1)
    }
    let mut out: Vec<HWND> = Vec::new();
    unsafe {
        let _ = EnumWindows(Some(each), LPARAM(&mut out as *mut Vec<HWND> as isize));
    }
    out
}

fn rect_of(h: HWND) -> Option<Rect> {
    let mut r = RECT::default();
    unsafe { GetWindowRect(h, &mut r).ok()? };
    Some(Rect {
        left: r.left,
        top: r.top,
        right: r.right,
        bottom: r.bottom,
    })
}

fn abm_data(h: HWND) -> APPBARDATA {
    APPBARDATA {
        cbSize: std::mem::size_of::<APPBARDATA>() as u32,
        hWnd: h,
        ..Default::default()
    }
}

fn abm_get_state(h: HWND) -> i32 {
    let mut d = abm_data(h);
    unsafe { SHAppBarMessage(ABM_GETSTATE, &mut d) as i32 }
}

fn abm_set_state(h: HWND, state: i32) {
    let mut d = abm_data(h);
    d.lParam = LPARAM(state as isize);
    unsafe {
        SHAppBarMessage(ABM_SETSTATE, &mut d);
    }
}

fn abm_taskbar_pos(h: HWND) -> Option<Rect> {
    let mut d = abm_data(h);
    if unsafe { SHAppBarMessage(ABM_GETTASKBARPOS, &mut d) } == 0 {
        return None;
    }
    Some(Rect {
        left: d.rc.left,
        top: d.rc.top,
        right: d.rc.right,
        bottom: d.rc.bottom,
    })
}

fn park(h: HWND) {
    unsafe {
        let _ = ShowWindow(h, SW_HIDE);
        let _ = SetWindowPos(h, None, PARK_X, PARK_X, 0, 0, SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE);
    }
}

fn place_and_show(h: HWND, rect: Option<Rect>) {
    unsafe {
        if let Some(r) = rect {
            let _ = SetWindowPos(h, None, r.left, r.top, 0, 0, SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE);
        }
        let _ = ShowWindow(h, SW_SHOW);
    }
}

pub fn read_marker(path: &Path) -> Option<Marker> {
    Marker::parse(&std::fs::read_to_string(path).ok()?)
}

fn write_marker(path: &Path, m: &Marker) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(path, m.serialize())
}

pub fn restore_marker(m: &Marker) -> bool {
    let Some(p) = primary() else { return false };
    HIDDEN.store(false, Ordering::Relaxed);
    abm_set_state(p, if m.abstate >= 0 { m.abstate } else { ABS_ALWAYSONTOP });
    place_and_show(p, m.primary);
    for s in secondaries() {
        place_and_show(s, m.secondary_rect(s.0 as isize));
    }
    true
}

pub fn recover_leftover(path: &Path) -> bool {
    if !path.exists() {
        return true;
    }
    let m = read_marker(path).unwrap_or_else(|| Marker::new(-1, 0));
    let ok = restore_marker(&m);
    if ok {
        let _ = std::fs::remove_file(path);
    }
    ok
}

fn spawn_guard_once(guard_exe: &Path, pid: u32, marker: &Path) {
    let mut started = lock(&GUARD_STARTED);
    if *started || !guard_exe.exists() {
        return;
    }
    const DETACHED_PROCESS: u32 = 0x0000_0008;
    const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    if Command::new(guard_exe)
        .arg(pid.to_string())
        .arg(marker)
        .creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP | CREATE_NO_WINDOW)
        .spawn()
        .is_ok()
    {
        *started = true;
    }
}

pub fn hide(marker_path: &Path, guard_exe: &Path) -> Result<(), String> {
    if is_hidden() {
        return Ok(());
    }
    let p = primary().ok_or("Shell_TrayWnd not found")?;
    let pid = std::process::id();
    let mut m = Marker::new(abm_get_state(p), pid);
    m.primary = rect_of(p)
        .filter(|r| r.is_placed())
        .or_else(|| abm_taskbar_pos(p).filter(|r| r.is_placed()));
    for s in secondaries() {
        if let Some(r) = rect_of(s).filter(|r| r.is_placed()) {
            m.secondaries.push((s.0 as isize, r));
        }
    }
    write_marker(marker_path, &m).map_err(|e| e.to_string())?;
    spawn_guard_once(guard_exe, pid, marker_path);

    *lock(&ACTIVE) = Some(Active {
        marker: m,
        path: marker_path.to_path_buf(),
    });
    abm_set_state(p, ABS_AUTOHIDE);
    park(p);
    for s in secondaries() {
        park(s);
    }
    HIDDEN.store(true, Ordering::Relaxed);
    install_enforcer();
    Ok(())
}

pub fn restore() -> bool {
    let active = lock(&ACTIVE).take();
    remove_enforcer();
    let Some(a) = active else {
        HIDDEN.store(false, Ordering::Relaxed);
        return true;
    };
    let ok = restore_marker(&a.marker);
    if ok {
        let _ = std::fs::remove_file(&a.path);
    }
    ok
}

unsafe extern "system" fn enforcer(
    _: HWINEVENTHOOK,
    _: u32,
    hwnd: HWND,
    id_object: i32,
    _: i32,
    _: u32,
    _: u32,
) {
    if !is_hidden() || id_object != 0 || hwnd.0.is_null() {
        return;
    }
    let c = class_of(hwnd);
    if (c == PRIMARY_CLASS || c == SECONDARY_CLASS) && IsWindowVisible(hwnd).as_bool() {
        park(hwnd);
    }
}

fn install_enforcer() {
    remove_enforcer();
    let Some(p) = primary() else { return };
    let mut pid = 0u32;
    unsafe { GetWindowThreadProcessId(p, Some(&mut pid)) };
    if pid == 0 {
        return;
    }
    let mut hooks = lock(&HOOKS);
    for ev in [EVENT_OBJECT_SHOW, EVENT_OBJECT_LOCATIONCHANGE] {
        let h = unsafe { SetWinEventHook(ev, ev, None, Some(enforcer), pid, 0, WINEVENT_OUTOFCONTEXT) };
        if !h.is_invalid() {
            hooks.push(h.0 as isize);
        }
    }
}

fn remove_enforcer() {
    for h in lock(&HOOKS).drain(..) {
        unsafe {
            let _ = UnhookWinEvent(HWINEVENTHOOK(h as *mut _));
        }
    }
}

pub fn trigger_start_menu() {
    unsafe {
        let down = INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: VK_LWIN,
                    ..Default::default()
                },
            },
        };
        let up = INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: VK_LWIN,
                    dwFlags: KEYEVENTF_KEYUP,
                    ..Default::default()
                },
            },
        };
        SendInput(&[down, up], std::mem::size_of::<INPUT>() as i32);
    }
}

pub fn launch_path(path: &str) {
    unsafe {
        ShellExecuteW(
            None,
            w!("open"),
            &HSTRING::from(path),
            PCWSTR::null(),
            PCWSTR::null(),
            SW_SHOWNORMAL,
        );
    }
}

pub fn activate_or_toggle_window(hwnd_raw: isize) {
    let hwnd = HWND(hwnd_raw as *mut _);
    unsafe {
        if !IsWindow(Some(hwnd)).as_bool() {
            return;
        }
        let fg = GetForegroundWindow();
        if fg == hwnd && !IsIconic(hwnd).as_bool() {
            let _ = ShowWindow(hwnd, SW_MINIMIZE);
        } else {
            if IsIconic(hwnd).as_bool() {
                let _ = ShowWindow(hwnd, SW_RESTORE);
            }
            let _ = SetForegroundWindow(hwnd);
        }
    }
}

pub fn enumerate_windows(own_pid: u32) -> Vec<WinInfo> {
    const SKIP: &[&str] = &[
        "conhost.exe",
        "shellexperiencehost.exe",
        "searchhost.exe",
        "textinputhost.exe",
        "startmenuexperiencehost.exe",
        "systemsettingsbroker.exe",
    ];
    struct Ctx {
        own_pid: u32,
        out: Vec<WinInfo>,
    }
    unsafe extern "system" fn each(hwnd: HWND, lp: LPARAM) -> BOOL {
        let ctx = &mut *(lp.0 as *mut Ctx);
        if !IsWindowVisible(hwnd).as_bool() {
            return BOOL(1);
        }
        let mut cloaked = 0u32;
        if DwmGetWindowAttribute(hwnd, DWMWA_CLOAKED, &mut cloaked as *mut u32 as *mut c_void, 4).is_ok()
            && cloaked != 0
        {
            return BOOL(1);
        }
        let mut tbuf = [0u16; 256];
        let tn = GetWindowTextW(hwnd, &mut tbuf);
        let title = String::from_utf16_lossy(&tbuf[..tn.max(0) as usize]);
        if title.is_empty() || title == "Program Manager" {
            return BOOL(1);
        }
        let ex = GetWindowLongW(hwnd, GWL_EXSTYLE) as u32;
        let app_window = (ex & WS_EX_APPWINDOW.0) != 0;
        if (ex & WS_EX_TOOLWINDOW.0) != 0 && !app_window {
            return BOOL(1);
        }
        if !app_window {
            if let Ok(owner) = GetWindow(hwnd, GW_OWNER) {
                if !owner.0.is_null() && IsWindowVisible(owner).as_bool() {
                    return BOOL(1);
                }
            }
        }
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        if pid == 0 || pid == ctx.own_pid {
            return BOOL(1);
        }
        let Ok(proc_h) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else {
            return BOOL(1);
        };
        let mut pbuf = [0u16; 1024];
        let mut plen = pbuf.len() as u32;
        let ok = QueryFullProcessImageNameW(
            proc_h,
            PROCESS_NAME_WIN32,
            windows::core::PWSTR(pbuf.as_mut_ptr()),
            &mut plen,
        )
        .is_ok();
        let _ = CloseHandle(proc_h);
        if !ok {
            return BOOL(1);
        }
        let path = String::from_utf16_lossy(&pbuf[..plen as usize]);
        let file = path.rsplit('\\').next().unwrap_or("").to_ascii_lowercase();
        if SKIP.contains(&file.as_str()) {
            return BOOL(1);
        }
        if file == "explorer.exe" {
            let c = class_of(hwnd);
            if c != "CabinetWClass" && c != "ExploreWClass" {
                return BOOL(1);
            }
        }
        let name = std::path::Path::new(&path)
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| file.clone());
        ctx.out.push(WinInfo {
            hwnd: hwnd.0 as isize,
            path,
            title,
            name,
        });
        BOOL(1)
    }
    let mut ctx = Ctx {
        own_pid,
        out: Vec::new(),
    };
    unsafe {
        let _ = EnumWindows(Some(each), LPARAM(&mut ctx as *mut Ctx as isize));
    }
    ctx.out
}

pub fn foreground_is_fullscreen(own_pid: u32, area: crate::monitors::Area) -> bool {
    unsafe {
        if SHQueryUserNotificationState().is_ok_and(|s| s == QUNS_RUNNING_D3D_FULL_SCREEN) {
            return true;
        }
        let fg = GetForegroundWindow();
        if fg.0.is_null() || !IsWindowVisible(fg).as_bool() || IsIconic(fg).as_bool() {
            return false;
        }
        let mut pid = 0u32;
        GetWindowThreadProcessId(fg, Some(&mut pid));
        if pid == own_pid {
            return false;
        }
        let cls = class_of(fg);
        if matches!(
            cls.as_str(),
            "Progman" | "WorkerW" | "Shell_TrayWnd" | "Shell_SecondaryTrayWnd"
        ) {
            return false;
        }
        let mut r = RECT::default();
        if DwmGetWindowAttribute(
            fg,
            DWMWA_EXTENDED_FRAME_BOUNDS,
            &mut r as *mut RECT as *mut c_void,
            std::mem::size_of::<RECT>() as u32,
        )
        .is_err()
            && GetWindowRect(fg, &mut r).is_err()
        {
            return false;
        }
        let covers = r.left <= area.x + 2
            && r.top <= area.y + 2
            && r.right >= area.right() - 2
            && r.bottom >= area.bottom() - 2;
        if !covers {
            return false;
        }
        let zoomed = IsZoomed(fg).as_bool();
        let caption = (GetWindowLongW(fg, GWL_STYLE) as u32 & WS_CAPTION.0) == WS_CAPTION.0;
        !zoomed || !caption
    }
}

pub fn foreground_hwnd() -> isize {
    unsafe { GetForegroundWindow().0 as isize }
}

pub fn foreground_is_maximized(own_pid: u32) -> bool {
    unsafe {
        let fg = GetForegroundWindow();
        if fg.0.is_null() || !IsWindowVisible(fg).as_bool() || IsIconic(fg).as_bool() {
            return false;
        }
        let mut pid = 0u32;
        GetWindowThreadProcessId(fg, Some(&mut pid));
        if pid == own_pid {
            return false;
        }
        let cls = class_of(fg);
        if matches!(
            cls.as_str(),
            "Progman" | "WorkerW" | "Shell_TrayWnd" | "Shell_SecondaryTrayWnd"
        ) {
            return false;
        }
        IsZoomed(fg).as_bool()
    }
}

pub fn open_system_taskbar_settings() {
    launch_path("ms-settings:taskbar");
}

pub fn pick_file_dialog(owner: HWND) -> Option<String> {
    use windows::Win32::System::Com::{CoCreateInstance, CoTaskMemFree, CLSCTX_ALL};
    use windows::Win32::UI::Shell::{FileOpenDialog, IFileOpenDialog, SIGDN_FILESYSPATH};
    unsafe {
        let dlg: IFileOpenDialog = CoCreateInstance(&FileOpenDialog, None, CLSCTX_ALL).ok()?;
        dlg.Show(Some(owner)).ok()?;
        let item = dlg.GetResult().ok()?;
        let p = item.GetDisplayName(SIGDN_FILESYSPATH).ok()?;
        let s = p.to_string().ok();
        CoTaskMemFree(Some(p.0 as *const _));
        s
    }
}
