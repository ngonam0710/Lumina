#![windows_subsystem = "windows"]

use np_platform::{brightness, taskbar};
use std::path::PathBuf;
use std::time::Duration;
use windows::Win32::Foundation::{CloseHandle, GetLastError, ERROR_INVALID_PARAMETER, WAIT_OBJECT_0};
use windows::Win32::System::Threading::{OpenProcess, WaitForSingleObject, INFINITE, PROCESS_SYNCHRONIZE};

fn wait_for_exit(pid: u32) -> bool {
    unsafe {
        match OpenProcess(PROCESS_SYNCHRONIZE, false, pid) {
            Ok(h) => {
                let r = WaitForSingleObject(h, INFINITE);
                let _ = CloseHandle(h);
                r == WAIT_OBJECT_0
            }
            Err(_) => GetLastError() == ERROR_INVALID_PARAMETER,
        }
    }
}

fn main() {
    let mut args = std::env::args_os().skip(1);
    let (Some(pid), Some(marker)) = (args.next(), args.next()) else {
        return;
    };
    let Ok(pid) = pid.to_string_lossy().parse::<u32>() else {
        return;
    };
    let marker = PathBuf::from(marker);
    if !wait_for_exit(pid) {
        return;
    }
    brightness::restore();
    for _ in 0..80 {
        match taskbar::read_marker(&marker) {
            Some(m) if m.pid != pid => return,
            None if !marker.exists() => return,
            _ => {}
        }
        if taskbar::recover_leftover(&marker) && !marker.exists() {
            return;
        }
        std::thread::sleep(Duration::from_millis(250));
    }
}
