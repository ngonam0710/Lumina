use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use windows::Devices::Radios::{Radio, RadioKind, RadioState};
use windows::Win32::Foundation::HANDLE;
use windows::Win32::NetworkManagement::WiFi::*;

static WIFI_ON: AtomicBool = AtomicBool::new(true);
static BT_ON: AtomicBool = AtomicBool::new(false);
static WIFI_SSID: Mutex<String> = Mutex::new(String::new());

pub fn wifi_on() -> bool {
    WIFI_ON.load(Ordering::Relaxed)
}

pub fn bt_on() -> bool {
    BT_ON.load(Ordering::Relaxed)
}

pub fn wifi_ssid() -> String {
    WIFI_SSID.lock().map(|g| g.clone()).unwrap_or_default()
}

pub fn poll_status() {
    if let Some(s) = current_ssid() {
        WIFI_ON.store(true, Ordering::Relaxed);
        if let Ok(mut g) = WIFI_SSID.lock() {
            *g = s;
        }
    }
    if let Ok(radios) = Radio::GetRadiosAsync().and_then(|o| o.join()) {
        for r in radios {
            let Ok(kind) = r.Kind() else { continue };
            let on = r.State().is_ok_and(|s| s == RadioState::On);
            if kind == RadioKind::WiFi {
                WIFI_ON.store(on, Ordering::Relaxed);
            } else if kind == RadioKind::Bluetooth {
                BT_ON.store(on, Ordering::Relaxed);
            }
        }
    }
}

pub fn toggle_radio(kind: RadioKind) {
    std::thread::spawn(move || {
        if let Ok(radios) = Radio::GetRadiosAsync().and_then(|o| o.join()) {
            for r in radios {
                if r.Kind() == Ok(kind) {
                    let on = r.State().is_ok_and(|s| s == RadioState::On);
                    let target = if on { RadioState::Off } else { RadioState::On };
                    let _ = r.SetStateAsync(target).and_then(|o| o.join());
                    if kind == RadioKind::WiFi {
                        WIFI_ON.store(!on, Ordering::Relaxed);
                    } else if kind == RadioKind::Bluetooth {
                        BT_ON.store(!on, Ordering::Relaxed);
                    }
                }
            }
        }
    });
}

pub fn toggle_wifi() {
    toggle_radio(RadioKind::WiFi);
}

pub fn toggle_bluetooth() {
    toggle_radio(RadioKind::Bluetooth);
}

pub fn current_ssid() -> Option<String> {
    unsafe {
        let mut ver = 0u32;
        let mut h = HANDLE::default();
        if WlanOpenHandle(2, None, &mut ver, &mut h) != 0 {
            return None;
        }
        let mut result = None;
        let mut list: *mut WLAN_INTERFACE_INFO_LIST = std::ptr::null_mut();
        if WlanEnumInterfaces(h, None, &mut list) == 0 && !list.is_null() {
            let l = &*list;
            for i in 0..l.dwNumberOfItems as usize {
                let info = &*l.InterfaceInfo.as_ptr().add(i);
                if info.isState != wlan_interface_state_connected {
                    continue;
                }
                let mut size = 0u32;
                let mut data: *mut std::ffi::c_void = std::ptr::null_mut();
                let rc = WlanQueryInterface(
                    h,
                    &info.InterfaceGuid,
                    wlan_intf_opcode_current_connection,
                    None,
                    &mut size,
                    &mut data,
                    None,
                );
                if rc == 0 && !data.is_null() {
                    let attrs = &*(data as *const WLAN_CONNECTION_ATTRIBUTES);
                    let s = &attrs.wlanAssociationAttributes.dot11Ssid;
                    let n = (s.uSSIDLength as usize).min(32);
                    let name = String::from_utf8_lossy(&s.ucSSID[..n]).to_string();
                    if !name.is_empty() {
                        result = Some(name);
                    }
                    WlanFreeMemory(data);
                }
                if result.is_some() {
                    break;
                }
            }
            WlanFreeMemory(list as *const _);
        }
        let _ = WlanCloseHandle(h, None);
        result
    }
}
