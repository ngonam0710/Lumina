use windows::core::BOOL;
use windows::Win32::Foundation::{LPARAM, POINT, RECT};
use windows::Win32::Graphics::Gdi::{EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITORINFO};
use windows::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
use windows::Win32::UI::WindowsAndMessaging::{
    GetCursorPos, GetSystemMetrics, MONITORINFOF_PRIMARY, SM_CXSCREEN, SM_CYSCREEN,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Area {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl Area {
    pub fn new(x: i32, y: i32, w: i32, h: i32) -> Self {
        Self { x, y, w, h }
    }

    pub fn right(self) -> i32 {
        self.x + self.w
    }

    pub fn bottom(self) -> i32 {
        self.y + self.h
    }

    pub fn size(self) -> (i32, i32) {
        (self.w, self.h)
    }

    pub fn contains(self, px: i32, py: i32) -> bool {
        px >= self.x && px < self.right() && py >= self.y && py < self.bottom()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Monitor {
    pub area: Area,
    pub work: Area,
    pub dpi: u32,
    pub primary: bool,
}

impl Monitor {
    pub fn scale(self) -> f32 {
        self.dpi.max(96) as f32 / 96.0
    }
}

fn rect_to_area(r: RECT) -> Area {
    Area::new(r.left, r.top, r.right - r.left, r.bottom - r.top)
}

unsafe extern "system" fn collect(h: HMONITOR, _: HDC, _: *mut RECT, lp: LPARAM) -> BOOL {
    let out = &mut *(lp.0 as *mut Vec<Monitor>);
    let mut mi = MONITORINFO {
        cbSize: std::mem::size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    if GetMonitorInfoW(h, &mut mi).as_bool() {
        let (mut dx, mut dy) = (96u32, 96u32);
        if GetDpiForMonitor(h, MDT_EFFECTIVE_DPI, &mut dx, &mut dy).is_err() {
            dx = 96;
        }
        out.push(Monitor {
            area: rect_to_area(mi.rcMonitor),
            work: rect_to_area(mi.rcWork),
            dpi: dx.max(96),
            primary: mi.dwFlags & MONITORINFOF_PRIMARY != 0,
        });
    }
    BOOL(1)
}

pub fn list() -> Vec<Monitor> {
    let mut out: Vec<Monitor> = Vec::new();
    unsafe {
        let _ = EnumDisplayMonitors(None, None, Some(collect), LPARAM(&mut out as *mut Vec<Monitor> as isize));
    }
    if out.is_empty() {
        let (w, h) = unsafe { (GetSystemMetrics(SM_CXSCREEN), GetSystemMetrics(SM_CYSCREEN)) };
        out.push(Monitor {
            area: Area::new(0, 0, w, h),
            work: Area::new(0, 0, w, h),
            dpi: 96,
            primary: true,
        });
    }
    out.sort_by_key(|m| (!m.primary, m.area.x, m.area.y));
    out
}

pub fn primary() -> Monitor {
    let l = list();
    l.iter().copied().find(|m| m.primary).unwrap_or(l[0])
}

pub fn at(x: i32, y: i32) -> Monitor {
    let l = list();
    l.iter().copied().find(|m| m.area.contains(x, y)).unwrap_or_else(primary)
}

pub fn under_cursor() -> Monitor {
    let mut p = POINT::default();
    unsafe {
        let _ = GetCursorPos(&mut p);
    }
    at(p.x, p.y)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn monitor_list_has_primary_and_valid_bounds() {
        let m = primary();
        assert!(m.area.w > 0 && m.area.h > 0 && m.dpi >= 96);
    }
}
