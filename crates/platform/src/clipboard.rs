use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use std::sync::Mutex;
use windows::core::w;
use windows::Win32::Foundation::{HANDLE, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::DataExchange::{
    AddClipboardFormatListener, CloseClipboard, EmptyClipboard, GetClipboardData,
    IsClipboardFormatAvailable, OpenClipboard, RegisterClipboardFormatW,
    RemoveClipboardFormatListener, SetClipboardData,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock, GMEM_MOVEABLE};
use windows::Win32::UI::WindowsAndMessaging::*;

const CF_UNICODETEXT: u32 = 13;

#[derive(Clone, Debug)]
pub struct ClipEntry {
    pub text: String,
    pub pinned: bool,
    pub at: i64,
}

static HISTORY: Mutex<Vec<ClipEntry>> = Mutex::new(Vec::new());
static ENABLED: AtomicBool = AtomicBool::new(true);
static HWND_LISTENER: AtomicIsize = AtomicIsize::new(0);

fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

pub fn set_enabled(on: bool) {
    ENABLED.store(on, Ordering::Relaxed);
    if !on {
        if let Ok(mut g) = HISTORY.lock() {
            g.retain(|e| e.pinned);
        }
    }
}

pub fn history() -> Vec<ClipEntry> {
    HISTORY.lock().map(|g| g.clone()).unwrap_or_default()
}

pub fn toggle_pin(index: usize) {
    if let Ok(mut g) = HISTORY.lock() {
        if let Some(item) = g.get_mut(index) {
            item.pinned = !item.pinned;
        }
    }
}

pub fn clear_unpinned() {
    if let Ok(mut g) = HISTORY.lock() {
        g.retain(|e| e.pinned);
    }
}

fn push_text(text: &str) {
    let t = text.trim_end_matches(['\r', '\n', '\0']);
    if t.trim().is_empty() {
        return;
    }
    let t: String = t.chars().take(2000).collect();
    if let Ok(mut g) = HISTORY.lock() {
        let was_pinned = g.iter().find(|e| e.text == t).is_some_and(|e| e.pinned);
        g.retain(|e| e.text != t);
        g.insert(
            0,
            ClipEntry {
                text: t,
                pinned: was_pinned,
                at: now_secs(),
            },
        );
        if g.len() > 40 {
            let mut kept = Vec::new();
            let mut extra = 0usize;
            for item in g.drain(..) {
                if item.pinned || extra < 30 {
                    if !item.pinned {
                        extra += 1;
                    }
                    kept.push(item);
                }
            }
            *g = kept;
        }
    }
}

unsafe fn read_clip_text(hwnd: HWND) -> Option<String> {
    for _ in 0..5 {
        if OpenClipboard(Some(hwnd)).is_ok() {
            let out = (|| {
                let f_excl = RegisterClipboardFormatW(w!("ExcludeClipboardContentFromMonitorProcessing"));
                if f_excl != 0 && IsClipboardFormatAvailable(f_excl).is_ok() {
                    return None;
                }
                if IsClipboardFormatAvailable(CF_UNICODETEXT).is_err() {
                    return None;
                }
                let h = GetClipboardData(CF_UNICODETEXT).ok()?;
                let g = windows::Win32::Foundation::HGLOBAL(h.0);
                let sz = GlobalSize(g);
                if sz == 0 {
                    return None;
                }
                let p = GlobalLock(g) as *const u16;
                if p.is_null() {
                    return None;
                }
                let mut n = 0usize;
                while n < (sz / 2).min(100_000) && *p.add(n) != 0 {
                    n += 1;
                }
                let s = String::from_utf16_lossy(std::slice::from_raw_parts(p, n));
                let _ = GlobalUnlock(g);
                Some(s)
            })();
            let _ = CloseClipboard();
            return out;
        }
        std::thread::sleep(std::time::Duration::from_millis(12));
    }
    None
}

unsafe extern "system" fn proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if msg == WM_CLIPBOARDUPDATE {
        if ENABLED.load(Ordering::Relaxed) {
            if let Some(s) = read_clip_text(hwnd) {
                push_text(&s);
            }
        }
        return LRESULT(0);
    }
    DefWindowProcW(hwnd, msg, wp, lp)
}

pub fn init() -> windows_core::Result<()> {
    unsafe {
        let hi = GetModuleHandleW(None)?.into();
        let class = w!("LuminaClipboard");
        RegisterClassW(&WNDCLASSW {
            lpfnWndProc: Some(proc),
            hInstance: hi,
            lpszClassName: class,
            ..Default::default()
        });
        let hwnd = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            class,
            w!("np-clip"),
            WS_POPUP,
            0,
            0,
            0,
            0,
            Some(HWND_MESSAGE),
            None,
            Some(hi),
            None,
        )?;
        AddClipboardFormatListener(hwnd)?;
        HWND_LISTENER.store(hwnd.0 as isize, Ordering::Relaxed);
    }
    Ok(())
}

pub fn shutdown() {
    let h = HWND_LISTENER.swap(0, Ordering::Relaxed);
    if h != 0 {
        let hwnd = HWND(h as *mut _);
        unsafe {
            let _ = RemoveClipboardFormatListener(hwnd);
            let _ = DestroyWindow(hwnd);
        }
    }
}

pub fn copy_text(text: &str) -> bool {
    unsafe {
        let hwnd = HWND(HWND_LISTENER.load(Ordering::Relaxed) as *mut _);
        for _ in 0..6 {
            if OpenClipboard(Some(hwnd)).is_ok() {
                let wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
                let ok = (|| {
                    let _ = EmptyClipboard();
                    let h = GlobalAlloc(GMEM_MOVEABLE, wide.len() * 2).ok()?;
                    let p = GlobalLock(h) as *mut u16;
                    if p.is_null() {
                        return None;
                    }
                    std::ptr::copy_nonoverlapping(wide.as_ptr(), p, wide.len());
                    let _ = GlobalUnlock(h);
                    SetClipboardData(CF_UNICODETEXT, Some(HANDLE(h.0))).ok()?;
                    Some(())
                })()
                .is_some();
                let _ = CloseClipboard();
                return ok;
            }
            std::thread::sleep(std::time::Duration::from_millis(12));
        }
        false
    }
}
