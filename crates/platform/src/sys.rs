use np_core::settings::PowerMode;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicU8, Ordering};
use std::time::{Duration, Instant};
use windows::core::w;
use windows::Win32::Foundation::FILETIME;
use windows::Win32::NetworkManagement::IpHelper::{FreeMibTable, GetIfTable2, MIB_IF_TABLE2};
use windows::Win32::NetworkManagement::Ndis::IfOperStatusUp;
use windows::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};
use windows::Win32::System::Registry::{
    RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_DWORD,
};
use windows::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
use windows::Win32::System::Threading::GetSystemTimes;
use windows::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_REMOTESESSION};

#[derive(Clone, Copy, Debug)]
pub struct Conditions {
    pub transparency: bool,
    pub battery_saver: bool,
    pub remote_session: bool,
}

pub fn conditions() -> Conditions {
    let mut v = 1u32;
    let mut size = 4u32;
    let transparency = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            w!("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize"),
            w!("EnableTransparency"),
            RRF_RT_REG_DWORD,
            None,
            Some(&mut v as *mut u32 as *mut _),
            Some(&mut size),
        )
        .is_err()
            || v != 0
    };
    let mut ps = SYSTEM_POWER_STATUS::default();
    let battery_saver =
        unsafe { GetSystemPowerStatus(&mut ps).is_ok() && (ps.SystemStatusFlag & 1) != 0 };
    let remote_session = unsafe { GetSystemMetrics(SM_REMOTESESSION) != 0 };
    Conditions {
        transparency,
        battery_saver,
        remote_session,
    }
}

pub fn battery() -> Option<(u8, bool)> {
    let mut ps = SYSTEM_POWER_STATUS::default();
    unsafe { GetSystemPowerStatus(&mut ps).ok()? };
    if (ps.BatteryFlag & 128) != 0 || ps.BatteryLifePercent > 100 {
        return None;
    }
    Some((
        ps.BatteryLifePercent,
        (ps.BatteryFlag & 8) != 0 || ps.ACLineStatus == 1,
    ))
}

pub fn cpu_times() -> Option<(u64, u64)> {
    let (mut idle, mut kernel, mut user) =
        (FILETIME::default(), FILETIME::default(), FILETIME::default());
    unsafe { GetSystemTimes(Some(&mut idle), Some(&mut kernel), Some(&mut user)).ok()? };
    let v = |f: FILETIME| ((f.dwHighDateTime as u64) << 32) | f.dwLowDateTime as u64;
    Some((v(idle), v(kernel) + v(user)))
}

pub fn cpu_percent(prev: (u64, u64), now: (u64, u64)) -> u8 {
    let idle = now.0.saturating_sub(prev.0);
    let total = now.1.saturating_sub(prev.1);
    if total == 0 {
        return 0;
    }
    (100 - (idle * 100 / total).min(100)) as u8
}

pub fn memory_load() -> Option<u8> {
    let mut m = MEMORYSTATUSEX {
        dwLength: std::mem::size_of::<MEMORYSTATUSEX>() as u32,
        ..Default::default()
    };
    unsafe { GlobalMemoryStatusEx(&mut m).ok()? };
    Some(m.dwMemoryLoad.min(100) as u8)
}

pub fn net_totals() -> (u64, u64) {
    unsafe {
        let mut table: *mut MIB_IF_TABLE2 = std::ptr::null_mut();
        if GetIfTable2(&mut table).0 != 0 || table.is_null() {
            return (0, 0);
        }
        let n = (*table).NumEntries as usize;
        let rows = std::slice::from_raw_parts((*table).Table.as_ptr(), n);
        let (mut rx, mut tx) = (0u64, 0u64);
        for r in rows {
            if r.OperStatus == IfOperStatusUp && r.Type != 24 && r.Type != 131 {
                rx = rx.saturating_add(r.InOctets);
                tx = tx.saturating_add(r.OutOctets);
            }
        }
        FreeMibTable(table as *const _);
        (rx, tx)
    }
}

static GPU_UTIL: AtomicU8 = AtomicU8::new(0);
static GPU_TEMP: AtomicU8 = AtomicU8::new(0);
static HAS_GPU_TEMP: AtomicBool = AtomicBool::new(false);
static NET_DOWN: AtomicU64 = AtomicU64::new(0);
static NET_UP: AtomicU64 = AtomicU64::new(0);
static CPU_PCT: AtomicU8 = AtomicU8::new(0);
static RAM_PCT: AtomicU8 = AtomicU8::new(0);
static SAMPLER_INIT: std::sync::OnceLock<()> = std::sync::OnceLock::new();

pub fn start_telemetry_sampler() {
    SAMPLER_INIT.get_or_init(|| {
        std::thread::Builder::new()
            .name("np-telemetry".into())
            .spawn(|| {
                let mut prev_cpu = cpu_times();
                let mut prev_net = net_totals();
                let mut prev_t = Instant::now();
                let mut ticks = 0u32;
                loop {
                    std::thread::sleep(Duration::from_millis(1000));
                    ticks = ticks.wrapping_add(1);
                    if let (Some(p), Some(n)) = (prev_cpu, cpu_times()) {
                        CPU_PCT.store(cpu_percent(p, n), Ordering::Relaxed);
                        prev_cpu = Some(n);
                    }
                    if let Some(r) = memory_load() {
                        RAM_PCT.store(r, Ordering::Relaxed);
                    }
                    let now_net = net_totals();
                    let dt = prev_t.elapsed().as_secs_f64().max(0.2);
                    prev_t = Instant::now();
                    let d_rx = now_net.0.saturating_sub(prev_net.0);
                    let d_tx = now_net.1.saturating_sub(prev_net.1);
                    prev_net = now_net;
                    NET_DOWN.store((d_rx as f64 / dt) as u64, Ordering::Relaxed);
                    NET_UP.store((d_tx as f64 / dt) as u64, Ordering::Relaxed);

                    if ticks % 2 == 1 {
                        if let Some((u, temp)) = query_nvidia_smi() {
                            GPU_UTIL.store(u, Ordering::Relaxed);
                            if let Some(t) = temp {
                                GPU_TEMP.store(t, Ordering::Relaxed);
                                HAS_GPU_TEMP.store(true, Ordering::Relaxed);
                            }
                        }
                    }
                }
            })
            .ok();
    });
}

fn query_nvidia_smi() -> Option<(u8, Option<u8>)> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let out = std::process::Command::new("nvidia-smi")
        .args([
            "--query-gpu=utilization.gpu,temperature.gpu",
            "--format=csv,noheader,nounits",
        ])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&out.stdout);
    let line = s.lines().next()?.trim();
    let mut parts = line.split(',');
    let u = parts.next()?.trim().parse::<u8>().ok()?.min(100);
    let t = parts.next().and_then(|x| x.trim().parse::<u8>().ok());
    Some((u, t))
}

pub fn telemetry_snapshot() -> (u8, u8, Option<u8>, u8, u64, u64) {
    (
        CPU_PCT.load(Ordering::Relaxed),
        GPU_UTIL.load(Ordering::Relaxed),
        HAS_GPU_TEMP
            .load(Ordering::Relaxed)
            .then(|| GPU_TEMP.load(Ordering::Relaxed)),
        RAM_PCT.load(Ordering::Relaxed),
        NET_DOWN.load(Ordering::Relaxed),
        NET_UP.load(Ordering::Relaxed),
    )
}

pub fn privacy_mic_cam() -> (bool, bool) {
    fn check(cap: &str) -> bool {
        use windows::Win32::System::Registry::{RegEnumKeyExW, RegOpenKeyExW, RegCloseKey, HKEY, KEY_READ};
        let sub = format!("Software\\Microsoft\\Windows\\CurrentVersion\\CapabilityAccessManager\\ConsentStore\\{cap}");
        let sub_w: Vec<u16> = sub.encode_utf16().chain(std::iter::once(0)).collect();
        let mut h = HKEY::default();
        if unsafe { RegOpenKeyExW(HKEY_CURRENT_USER, windows::core::PCWSTR(sub_w.as_ptr()), Some(0), KEY_READ, &mut h) }.is_err() {
            return false;
        }
        let mut idx = 0u32;
        let mut active = false;
        loop {
            let mut name = [0u16; 256];
            let mut len = name.len() as u32;
            if unsafe { RegEnumKeyExW(h, idx, Some(windows::core::PWSTR(name.as_mut_ptr())), &mut len, None, None, None, None) }.is_err() {
                break;
            }
            idx += 1;
            let kname = String::from_utf16_lossy(&name[..len as usize]);
            if kname == "NonPackaged" {
                continue;
            }
            let mut stop: u64 = 1;
            let mut sz = 8u32;
            let full = format!("{sub}\\{kname}");
            let full_w: Vec<u16> = full.encode_utf16().chain(std::iter::once(0)).collect();
            if unsafe {
                RegGetValueW(
                    HKEY_CURRENT_USER,
                    windows::core::PCWSTR(full_w.as_ptr()),
                    w!("LastUsedTimeStop"),
                    windows::Win32::System::Registry::RRF_RT_REG_QWORD,
                    None,
                    Some(&mut stop as *mut u64 as *mut _),
                    Some(&mut sz),
                )
            }
            .is_ok()
                && stop == 0
            {
                active = true;
                break;
            }
        }
        unsafe {
            let _ = RegCloseKey(h);
        }
        active
    }
    (check("microphone"), check("webcam"))
}

pub fn set_power_mode(mode: PowerMode) {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let guid = match mode {
        PowerMode::Battery => "a1841308-3541-4fab-bc81-f71556f20b4a",
        PowerMode::Balanced => "381b4222-f694-41f0-9685-ff5bb260df2e",
        PowerMode::Performance => "8c5e7fda-e8bf-4a96-9a85-a6e23a8c635c",
    };
    let _ = std::process::Command::new("powercfg")
        .args(["/setactive", guid])
        .creation_flags(CREATE_NO_WINDOW)
        .spawn();
}

pub fn lock_workstation() {
    let _ = unsafe { windows::Win32::System::Shutdown::LockWorkStation() };
}
