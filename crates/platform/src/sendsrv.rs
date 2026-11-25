use std::io::{BufRead, BufReader, Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream, UdpSocket};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Progress {
    pub sent: u64,
    pub total: u64,
    pub files_done: usize,
    pub active: bool,
}

pub struct Server {
    pub url: String,
    pub progress: Arc<Mutex<Progress>>,
    stop: Arc<AtomicBool>,
    addr: SocketAddr,
}

impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        let _ = TcpStream::connect_timeout(&self.addr, Duration::from_millis(250));
    }
}

fn is_private(ip: [u8; 4]) -> bool {
    ip[0] == 10
        || (ip[0] == 172 && (16..=31).contains(&ip[1]))
        || (ip[0] == 192 && ip[1] == 168)
}

fn lan_ip() -> Option<Ipv4Addr> {
    let s = UdpSocket::bind("0.0.0.0:0").ok()?;
    s.connect("8.8.8.8:80").ok()?;
    match s.local_addr().ok()? {
        SocketAddr::V4(a) if is_private(a.ip().octets()) => Some(*a.ip()),
        _ => None,
    }
}

fn token() -> String {
    let mut b = [0u8; 8];
    unsafe {
        let _ = windows::Win32::Security::Cryptography::BCryptGenRandom(
            None,
            &mut b,
            windows::Win32::Security::Cryptography::BCRYPT_USE_SYSTEM_PREFERRED_RNG,
        );
    }
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn html_page(tok: &str, files: &[(String, String, u64)]) -> String {
    let mut list = String::new();
    for (i, (_, name, size)) in files.iter().enumerate() {
        let kb = (*size as f64 / 1024.0).max(0.1);
        list.push_str(&format!(
            "<a class='card' href='/{tok}/f/{i}'><span>{name}</span><small>{kb:.1} KB</small></a>"
        ));
    }
    format!(
        "<!doctype html><html><head><meta charset='utf-8'><meta name='viewport' content='width=device-width,initial-scale=1'><title>Lumina Send</title><style>body{{font-family:system-ui;background:#0d0e12;color:#fff;margin:0;padding:24px}}h1{{font-size:20px;margin:0 0 16px}}.card{{display:flex;justify-content:space-between;align-items:center;background:#1a1c24;padding:14px 16px;border-radius:14px;color:#fff;text-decoration:none;margin-bottom:10px;border:1px solid rgba(255,255,255,.1)}}small{{opacity:.65}}</style></head><body><h1>Lumina File Share</h1>{list}</body></html>"
    )
}

fn respond(s: &mut TcpStream, status: &str, ctype: &str, body: &[u8]) {
    let head = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n",
        body.len()
    );
    let _ = s.write_all(head.as_bytes());
    let _ = s.write_all(body);
}

fn serve(
    mut s: TcpStream,
    tok: &str,
    files: &[(String, String, u64)],
    prog: &Arc<Mutex<Progress>>,
    notify: &(dyn Fn() + Send + Sync),
) {
    let _ = s.set_read_timeout(Some(Duration::from_secs(5)));
    let _ = s.set_write_timeout(Some(Duration::from_secs(30)));
    let Ok(clone) = s.try_clone() else { return };
    let mut r = BufReader::new(clone.take(8192));
    let mut line = String::new();
    if r.read_line(&mut line).is_err() {
        return;
    }
    let mut parts = line.split_whitespace();
    let (method, path) = (parts.next().unwrap_or(""), parts.next().unwrap_or(""));
    if method != "GET" {
        respond(&mut s, "405 Method Not Allowed", "text/plain", b"");
        return;
    }
    if path == format!("/{tok}") || path == format!("/{tok}/") {
        respond(
            &mut s,
            "200 OK",
            "text/html; charset=utf-8",
            html_page(tok, files).as_bytes(),
        );
        return;
    }
    let Some(i) = path
        .strip_prefix(&format!("/{tok}/f/"))
        .and_then(|n| n.parse::<usize>().ok())
        .filter(|i| *i < files.len())
    else {
        respond(&mut s, "404 Not Found", "text/plain", b"");
        return;
    };
    let (fpath, name, size) = &files[i];
    let Ok(mut f) = std::fs::File::open(fpath) else {
        respond(&mut s, "404 Not Found", "text/plain", b"");
        return;
    };
    let head = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nContent-Length: {size}\r\nContent-Disposition: attachment; filename=\"{name}\"\r\nConnection: close\r\n\r\n"
    );
    if s.write_all(head.as_bytes()).is_err() {
        return;
    }
    if let Ok(mut p) = prog.lock() {
        p.active = true;
    }
    notify();
    let mut buf = vec![0u8; 64 * 1024];
    let mut last = Instant::now();
    loop {
        let Ok(n) = f.read(&mut buf) else { break };
        if n == 0 || s.write_all(&buf[..n]).is_err() {
            break;
        }
        if let Ok(mut p) = prog.lock() {
            p.sent = (p.sent + n as u64).min(p.total);
        }
        if last.elapsed() > Duration::from_millis(200) {
            last = Instant::now();
            notify();
        }
    }
    if let Ok(mut p) = prog.lock() {
        p.files_done += 1;
        p.active = false;
    }
    notify();
}

pub fn start(paths: &[String], notify: impl Fn() + Send + Sync + 'static) -> Result<Server, String> {
    let files: Vec<(String, String, u64)> = paths
        .iter()
        .filter_map(|p| {
            let md = std::fs::metadata(p).ok().filter(|m| m.is_file())?;
            let name = std::path::Path::new(p).file_name()?.to_string_lossy().to_string();
            Some((p.clone(), name, md.len()))
        })
        .collect();
    if files.is_empty() {
        return Err("No valid files selected".into());
    }
    let ip = lan_ip().unwrap_or(Ipv4Addr::LOCALHOST);
    let listener = TcpListener::bind((ip, 0)).map_err(|e| e.to_string())?;
    let addr = listener.local_addr().map_err(|e| e.to_string())?;
    let tok = token();
    let url = format!("http://{ip}:{}/{tok}/", addr.port());
    let total: u64 = files.iter().map(|f| f.2).sum();
    let prog = Arc::new(Mutex::new(Progress {
        sent: 0,
        total,
        files_done: 0,
        active: false,
    }));
    let stop = Arc::new(AtomicBool::new(false));
    let stop2 = stop.clone();
    let prog2 = prog.clone();
    let tok2 = tok.clone();
    let notify = Arc::new(notify);
    std::thread::spawn(move || {
        for conn in listener.incoming() {
            if stop2.load(Ordering::Relaxed) {
                break;
            }
            let Ok(c) = conn else { continue };
            let files = files.clone();
            let prog = prog2.clone();
            let tok = tok2.clone();
            let notify = notify.clone();
            std::thread::spawn(move || serve(c, &tok, &files, &prog, &*notify));
        }
    });
    Ok(Server {
        url,
        progress: prog,
        stop,
        addr,
    })
}
