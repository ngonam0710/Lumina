use windows::core::{w, PCWSTR};
use windows::Win32::System::Registry::{
    RegCloseKey, RegCreateKeyExW, RegDeleteValueW, RegGetValueW, RegOpenKeyExW, RegSetValueExW,
    HKEY, HKEY_CURRENT_USER, KEY_SET_VALUE, REG_OPTION_NON_VOLATILE, REG_SZ, RRF_RT_REG_SZ,
};

const RUN_SUBKEY: PCWSTR = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Run");
const VAL_NAME: PCWSTR = w!("LuminaRS");

pub fn is_enabled() -> bool {
    let mut len = 0u32;
    unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            RUN_SUBKEY,
            VAL_NAME,
            RRF_RT_REG_SZ,
            None,
            None,
            Some(&mut len),
        )
        .is_ok()
            && len > 2
    }
}

pub fn set_enabled(on: bool) -> bool {
    unsafe {
        if on {
            let Ok(exe) = std::env::current_exe() else {
                return false;
            };
            let cmd = format!("\"{}\"", exe.to_string_lossy());
            let mut wide: Vec<u16> = cmd.encode_utf16().chain(std::iter::once(0)).collect();
            let mut key = HKEY::default();
            if RegCreateKeyExW(
                HKEY_CURRENT_USER,
                RUN_SUBKEY,
                None,
                None,
                REG_OPTION_NON_VOLATILE,
                KEY_SET_VALUE,
                None,
                &mut key,
                None,
            )
            .is_err()
            {
                return false;
            }
            let bytes = std::slice::from_raw_parts(wide.as_ptr() as *const u8, wide.len() * 2);
            let ok = RegSetValueExW(key, VAL_NAME, None, REG_SZ, Some(bytes)).is_ok();
            let _ = RegCloseKey(key);
            let _ = &mut wide;
            ok
        } else {
            let mut key = HKEY::default();
            if RegOpenKeyExW(HKEY_CURRENT_USER, RUN_SUBKEY, None, KEY_SET_VALUE, &mut key).is_err() {
                return true;
            }
            let _ = RegDeleteValueW(key, VAL_NAME);
            let _ = RegCloseKey(key);
            true
        }
    }
}

pub fn system_is_vietnamese() -> bool {
    let mut buf = [0u16; 32];
    let mut bytes = (buf.len() * 2) as u32;
    unsafe {
        if RegGetValueW(
            HKEY_CURRENT_USER,
            w!("Control Panel\\International"),
            w!("LocaleName"),
            RRF_RT_REG_SZ,
            None,
            Some(buf.as_mut_ptr() as *mut _),
            Some(&mut bytes),
        )
        .is_ok()
            && bytes >= 4
        {
            let len = (bytes as usize / 2).saturating_sub(1).min(buf.len());
            let s = String::from_utf16_lossy(&buf[..len]).to_ascii_lowercase();
            return s.starts_with("vi");
        }
    }
    false
}
