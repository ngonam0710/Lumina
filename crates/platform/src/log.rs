use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;

static LOG_FILE: Mutex<Option<PathBuf>> = Mutex::new(None);

pub fn init(path: PathBuf) {
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    *LOG_FILE.lock().unwrap_or_else(|e| e.into_inner()) = Some(path);
}

pub fn line(tag: &str, msg: &str) {
    let path = LOG_FILE.lock().unwrap_or_else(|e| e.into_inner()).clone();
    let Some(path) = path else { return };
    let t = crate::time::local_now();
    let row = format!(
        "[{:04}-{:02}-{:02} {:02}:{:02}:{:02}] [{tag}] {msg}\n",
        t.year, t.month, t.day, t.hour, t.minute, t.second
    );
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
        let _ = f.write_all(row.as_bytes());
    }
}
