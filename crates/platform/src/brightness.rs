use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::mpsc::{channel, Sender};
use windows::core::{BOOL, PCWSTR};
use windows::Win32::Devices::Display::{
    DestroyPhysicalMonitor, GetMonitorBrightness, GetNumberOfPhysicalMonitorsFromHMONITOR,
    GetPhysicalMonitorsFromHMONITOR, SetMonitorBrightness, PHYSICAL_MONITOR,
};
use windows::Win32::Foundation::{HANDLE, LPARAM, RECT};
use windows::Win32::Graphics::Gdi::{
    CreateDCW, DeleteDC, EnumDisplayMonitors, GetDC, GetMonitorInfoW, ReleaseDC, HDC, HMONITOR,
    MONITORINFO, MONITORINFOEXW,
};
use windows::Win32::UI::ColorSystem::SetDeviceGammaRamp;

static BRIGHTNESS: AtomicU8 = AtomicU8::new(85);
static NIGHT_LIGHT: AtomicBool = AtomicBool::new(false);

pub fn current() -> u8 {
    BRIGHTNESS.load(Ordering::Relaxed)
}

pub fn night_light() -> bool {
    NIGHT_LIGHT.load(Ordering::Relaxed)
}

pub fn ramp_for(p: u8, warm: bool) -> [[u16; 256]; 3] {
    let mult = 0.35 + 0.65 * (p.min(100) as f32 / 100.0);
    let (rk, gk, bk) = if warm { (1.0, 0.84, 0.66) } else { (1.0, 1.0, 1.0) };
    let mut ramp = [[0u16; 256]; 3];
    for i in 0..256 {
        let base = i as f32 * 257.0 * mult;
        ramp[0][i] = (base * rk).round().clamp(0.0, 65535.0) as u16;
        ramp[1][i] = (base * gk).round().clamp(0.0, 65535.0) as u16;
        ramp[2][i] = (base * bk).round().clamp(0.0, 65535.0) as u16;
    }
    ramp
}

unsafe extern "system" fn gather(h: HMONITOR, _: HDC, _: *mut RECT, lp: LPARAM) -> BOOL {
    let out = &mut *(lp.0 as *mut Vec<HMONITOR>);
    out.push(h);
    BOOL(1)
}

fn handles() -> Vec<HMONITOR> {
    let mut out = Vec::new();
    unsafe {
        let _ = EnumDisplayMonitors(None, None, Some(gather), LPARAM(&mut out as *mut Vec<HMONITOR> as isize));
    }
    out
}

fn devices() -> Vec<Vec<u16>> {
    handles()
        .into_iter()
        .filter_map(|h| unsafe {
            let mut mi = MONITORINFOEXW::default();
            mi.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;
            if !GetMonitorInfoW(h, &mut mi as *mut MONITORINFOEXW as *mut MONITORINFO).as_bool() {
                return None;
            }
            let n = mi.szDevice.iter().position(|c| *c == 0).unwrap_or(mi.szDevice.len());
            if n == 0 {
                return None;
            }
            let mut v = mi.szDevice[..n].to_vec();
            v.push(0);
            Some(v)
        })
        .collect()
}

pub fn apply_gamma(p: u8, warm: bool) -> bool {
    let ramp = ramp_for(p, warm);
    let devs = devices();
    if devs.is_empty() {
        unsafe {
            let dc = GetDC(None);
            if dc.is_invalid() {
                return false;
            }
            let ok = SetDeviceGammaRamp(dc, ramp.as_ptr() as *const _).as_bool();
            ReleaseDC(None, dc);
            return ok;
        }
    }
    devs.into_iter().fold(false, |acc, d| unsafe {
        let dc = CreateDCW(PCWSTR::null(), PCWSTR(d.as_ptr()), PCWSTR::null(), None);
        if dc.is_invalid() {
            return acc;
        }
        let ok = SetDeviceGammaRamp(dc, ramp.as_ptr() as *const _).as_bool();
        let _ = DeleteDC(dc);
        acc | ok
    })
}

pub fn restore() {
    let _ = apply_gamma(100, false);
}

pub enum Cmd {
    SetBrightness(u8),
    SetNightLight(bool),
}

pub struct DisplayCtl {
    pub commands: Sender<Cmd>,
}

struct DdcMon {
    handle: HANDLE,
    lo: u32,
    hi: u32,
}

pub fn spawn(notify: impl Fn() + Send + 'static) -> DisplayCtl {
    let (tx, rx) = channel::<Cmd>();
    std::thread::Builder::new()
        .name("np-display".into())
        .spawn(move || {
            let mut ddcs: Vec<DdcMon> = Vec::new();
            for h in handles() {
                unsafe {
                    let mut n = 0u32;
                    if GetNumberOfPhysicalMonitorsFromHMONITOR(h, &mut n).is_ok() && n > 0 && n <= 8 {
                        let mut arr = vec![PHYSICAL_MONITOR::default(); n as usize];
                        if GetPhysicalMonitorsFromHMONITOR(h, &mut arr).is_ok() {
                            for pm in arr {
                                let (mut lo, mut cur, mut hi) = (0u32, 0u32, 0u32);
                                if GetMonitorBrightness(pm.hPhysicalMonitor, &mut lo, &mut cur, &mut hi) != 0 && hi > lo {
                                    let pct = ((cur.clamp(lo, hi) - lo) * 100 / (hi - lo)) as u8;
                                    BRIGHTNESS.store(pct, Ordering::Relaxed);
                                    ddcs.push(DdcMon {
                                        handle: pm.hPhysicalMonitor,
                                        lo,
                                        hi,
                                    });
                                } else {
                                    let _ = DestroyPhysicalMonitor(pm.hPhysicalMonitor);
                                }
                            }
                        }
                    }
                }
            }
            notify();
            while let Ok(mut cmd) = rx.recv() {
                while let Ok(next) = rx.try_recv() {
                    cmd = next;
                }
                match cmd {
                    Cmd::SetBrightness(p) => {
                        let p = p.min(100);
                        BRIGHTNESS.store(p, Ordering::Relaxed);
                        if !ddcs.is_empty() {
                            for d in &ddcs {
                                let v = d.lo + (d.hi - d.lo) * p as u32 / 100;
                                unsafe {
                                    SetMonitorBrightness(d.handle, v);
                                }
                            }
                            if NIGHT_LIGHT.load(Ordering::Relaxed) {
                                apply_gamma(100, true);
                            }
                        } else {
                            apply_gamma(p, NIGHT_LIGHT.load(Ordering::Relaxed));
                        }
                        notify();
                    }
                    Cmd::SetNightLight(on) => {
                        NIGHT_LIGHT.store(on, Ordering::Relaxed);
                        let p = if ddcs.is_empty() {
                            BRIGHTNESS.load(Ordering::Relaxed)
                        } else {
                            100
                        };
                        apply_gamma(p, on);
                        notify();
                    }
                }
            }
            for d in ddcs {
                unsafe {
                    let _ = DestroyPhysicalMonitor(d.handle);
                }
            }
        })
        .ok();
    DisplayCtl { commands: tx }
}
