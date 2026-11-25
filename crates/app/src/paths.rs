use std::path::PathBuf;

pub fn config_dir() -> PathBuf {
    if let Some(p) = std::env::var_os("LUMINA_CONFIG_DIR") {
        return PathBuf::from(p);
    }
    let base = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir());
    base.join("LuminaRS")
}

pub fn settings_file() -> PathBuf {
    config_dir().join("settings.json")
}

pub fn widgets_file() -> PathBuf {
    config_dir().join("widgets.json")
}

pub fn marker_file() -> PathBuf {
    config_dir().join("taskbar_marker.json")
}

pub fn log_file() -> PathBuf {
    config_dir().join("lumina.log")
}

pub fn guard_exe() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("lumina-guard.exe")))
        .unwrap_or_else(|| PathBuf::from("lumina-guard.exe"))
}
