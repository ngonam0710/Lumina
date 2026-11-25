use std::io::{Seek, SeekFrom, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use windows::core::HSTRING;
use windows::Win32::Foundation::{HANDLE, HGLOBAL, HWND, POINT};
use windows::Win32::Graphics::Gdi::{
    BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject, GetDC, GetDIBits,
    GetPixel, ReleaseDC, SelectObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS,
    HGDIOBJ, SRCCOPY,
};
use windows::Win32::Graphics::Imaging::{
    CLSID_WICImagingFactory, GUID_ContainerFormatJpeg, GUID_ContainerFormatPng,
    GUID_WICPixelFormat32bppBGRA, IWICImagingFactory, WICBitmapEncoderNoCache,
};
use windows::Win32::System::Com::StructuredStorage::CreateStreamOnHGlobal;
use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CoUninitialize, IStream, CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, STREAM_SEEK_SET};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData,
};
use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::{
    GetCursorPos, GetSystemMetrics, SetWindowDisplayAffinity, SM_CXSCREEN,
    SM_CXVIRTUALSCREEN, SM_CYSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN,
    SW_SHOWNORMAL, WDA_EXCLUDEFROMCAPTURE, WDA_NONE,
};

pub fn capture_fullscreen_png() -> Option<PathBuf> {
    let t = crate::time::local_now();
    let dir = dirs_pictures().join("Lumina");
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join(format!(
        "Screenshot_{:04}{:02}{:02}_{:02}{:02}{:02}.png",
        t.year, t.month, t.day, t.hour, t.minute, t.second
    ));
    unsafe {
        let vx = GetSystemMetrics(SM_XVIRTUALSCREEN);
        let vy = GetSystemMetrics(SM_YVIRTUALSCREEN);
        let vw = GetSystemMetrics(SM_CXVIRTUALSCREEN).max(1);
        let vh = GetSystemMetrics(SM_CYVIRTUALSCREEN).max(1);

        let screen_dc = GetDC(None);
        let mem_dc = CreateCompatibleDC(Some(screen_dc));
        let bmp = CreateCompatibleBitmap(screen_dc, vw, vh);
        let old = SelectObject(mem_dc, HGDIOBJ(bmp.0));
        let ok = BitBlt(mem_dc, 0, 0, vw, vh, Some(screen_dc), vx, vy, SRCCOPY).is_ok();

        let mut info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: vw,
                biHeight: -vh,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut buf = vec![0u8; (vw as usize) * (vh as usize) * 4];
        if ok {
            GetDIBits(
                mem_dc,
                bmp,
                0,
                vh as u32,
                Some(buf.as_mut_ptr() as *mut _),
                &mut info,
                DIB_RGB_COLORS,
            );
            for px in buf.chunks_exact_mut(4) {
                px[3] = 255;
            }
        }
        SelectObject(mem_dc, old);
        let _ = DeleteObject(HGDIOBJ(bmp.0));
        let _ = DeleteDC(mem_dc);
        ReleaseDC(None, screen_dc);
        if !ok {
            return None;
        }

        let factory: IWICImagingFactory =
            CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER).ok()?;
        let wic_bmp = factory
            .CreateBitmapFromMemory(
                vw as u32,
                vh as u32,
                &GUID_WICPixelFormat32bppBGRA,
                (vw * 4) as u32,
                &buf,
            )
            .ok()?;
        let stream = factory.CreateStream().ok()?;
        stream
            .InitializeFromFilename(
                &HSTRING::from(path.to_string_lossy().as_ref()),
                windows::Win32::Foundation::GENERIC_WRITE.0,
            )
            .ok()?;
        let enc = factory
            .CreateEncoder(&GUID_ContainerFormatPng, std::ptr::null())
            .ok()?;
        enc.Initialize(&stream, WICBitmapEncoderNoCache).ok()?;
        let mut frame = None;
        enc.CreateNewFrame(&mut frame, std::ptr::null_mut()).ok()?;
        let frame = frame?;
        frame.Initialize(None).ok()?;
        frame.WriteSource(&wic_bmp, std::ptr::null()).ok()?;
        frame.Commit().ok()?;
        enc.Commit().ok()?;

        copy_bgra_to_clipboard(vw, vh, &buf);
        Some(path)
    }
}

fn copy_bgra_to_clipboard(w: i32, h: i32, bgra: &[u8]) {
    const CF_DIB: u32 = 8;
    let hdr_sz = std::mem::size_of::<BITMAPINFOHEADER>();
    let mut dib = vec![0u8; hdr_sz + bgra.len()];
    let hdr = BITMAPINFOHEADER {
        biSize: hdr_sz as u32,
        biWidth: w,
        biHeight: -h,
        biPlanes: 1,
        biBitCount: 32,
        biCompression: BI_RGB.0,
        biSizeImage: bgra.len() as u32,
        ..Default::default()
    };
    unsafe {
        std::ptr::copy_nonoverlapping(&hdr as *const _ as *const u8, dib.as_mut_ptr(), hdr_sz);
        dib[hdr_sz..].copy_from_slice(bgra);
        if OpenClipboard(None).is_ok() {
            let _ = EmptyClipboard();
            if let Ok(hg) = GlobalAlloc(GMEM_MOVEABLE, dib.len()) {
                let p = GlobalLock(hg) as *mut u8;
                if !p.is_null() {
                    std::ptr::copy_nonoverlapping(dib.as_ptr(), p, dib.len());
                    let _ = GlobalUnlock(hg);
                    let _ = SetClipboardData(CF_DIB, Some(HANDLE(hg.0)));
                }
            }
            let _ = CloseClipboard();
        }
    }
}

pub fn pick_color_at_cursor() -> Option<[u8; 3]> {
    unsafe {
        let mut pt = POINT::default();
        GetCursorPos(&mut pt).ok()?;
        let dc = GetDC(None);
        if dc.is_invalid() {
            return None;
        }
        let c = GetPixel(dc, pt.x, pt.y).0;
        ReleaseDC(None, dc);
        if c == 0xFFFF_FFFF {
            return None;
        }
        Some([(c & 0xFF) as u8, ((c >> 8) & 0xFF) as u8, ((c >> 16) & 0xFF) as u8])
    }
}

fn dirs_pictures() -> PathBuf {
    std::env::var_os("USERPROFILE")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("C:\\Users\\Public"))
        .join("Pictures")
}

fn dirs_videos() -> PathBuf {
    std::env::var_os("USERPROFILE")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("C:\\Users\\Public"))
        .join("Videos")
}

pub fn trigger_snip() -> bool {
    unsafe {
        let h = ShellExecuteW(
            None,
            windows::core::w!("open"),
            windows::core::w!("ms-screenclip:"),
            None,
            None,
            SW_SHOWNORMAL,
        );
        (h.0 as isize) > 32
    }
}

pub fn set_capture_excluded(hwnd: HWND, exclude: bool) {
    unsafe {
        let _ = SetWindowDisplayAffinity(
            hwnd,
            if exclude { WDA_EXCLUDEFROMCAPTURE } else { WDA_NONE },
        );
    }
}

pub struct Recorder {
    stop: Arc<AtomicBool>,
    elapsed_secs: Arc<AtomicU64>,
    path: PathBuf,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Recorder {
    pub fn start(notify: impl Fn() + Send + 'static) -> Option<Self> {
        let t = crate::time::local_now();
        let dir = dirs_videos().join("Lumina");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join(format!(
            "Recording_{:04}{:02}{:02}_{:02}{:02}{:02}.avi",
            t.year, t.month, t.day, t.hour, t.minute, t.second
        ));
        let stop = Arc::new(AtomicBool::new(false));
        let elapsed_secs = Arc::new(AtomicU64::new(0));
        let stop_t = stop.clone();
        let el_t = elapsed_secs.clone();
        let path_t = path.clone();

        let thread = std::thread::Builder::new()
            .name("np-recorder".into())
            .spawn(move || unsafe {
                let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
                let _ = record_loop(&path_t, &stop_t, &el_t, notify);
                CoUninitialize();
            })
            .ok()?;

        Some(Self {
            stop,
            elapsed_secs,
            path,
            thread: Some(thread),
        })
    }

    pub fn elapsed_secs(&self) -> u64 {
        self.elapsed_secs.load(Ordering::Relaxed)
    }

    pub fn stop(mut self) -> PathBuf {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(h) = self.thread.take() {
            let _ = h.join();
        }
        self.path.clone()
    }
}

impl Drop for Recorder {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(h) = self.thread.take() {
            let _ = h.join();
        }
    }
}

unsafe fn record_loop(
    path: &PathBuf,
    stop: &AtomicBool,
    elapsed_secs: &AtomicU64,
    notify: impl Fn(),
) -> Option<()> {
    let w = GetSystemMetrics(SM_CXSCREEN).max(2);
    let h = GetSystemMetrics(SM_CYSCREEN).max(2);
    let mut file = std::fs::File::create(path).ok()?;
    let hdr_pos = write_avi_header(&mut file, w as u32, h as u32, 10).ok()?;
    let movi_start = file.stream_position().ok()? - 4;
    let factory: IWICImagingFactory =
        CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER).ok()?;
    let mut index: Vec<(u32, u32)> = Vec::new();
    let t0 = std::time::Instant::now();
    let mut last_notify = 0u64;

    while !stop.load(Ordering::Relaxed) {
        let frame_t = std::time::Instant::now();
        if let Some(jpg) = grab_primary_jpeg(&factory, w, h) {
            let off = (file.stream_position().ok()? - movi_start) as u32;
            let sz = jpg.len() as u32;
            let _ = file.write_all(b"00dc");
            let _ = file.write_all(&sz.to_le_bytes());
            let _ = file.write_all(&jpg);
            if sz % 2 == 1 {
                let _ = file.write_all(&[0]);
            }
            index.push((off, sz));
        }
        let sec = t0.elapsed().as_secs();
        elapsed_secs.store(sec, Ordering::Relaxed);
        if sec != last_notify {
            last_notify = sec;
            notify();
        }
        let spent = frame_t.elapsed();
        if spent < std::time::Duration::from_millis(100) {
            std::thread::sleep(std::time::Duration::from_millis(100) - spent);
        }
    }

    finalize_avi(&mut file, hdr_pos, movi_start, &index).ok()?;
    notify();
    Some(())
}

unsafe fn grab_primary_jpeg(factory: &IWICImagingFactory, w: i32, h: i32) -> Option<Vec<u8>> {
    let screen_dc = GetDC(None);
    let mem_dc = CreateCompatibleDC(Some(screen_dc));
    let bmp = CreateCompatibleBitmap(screen_dc, w, h);
    let old = SelectObject(mem_dc, HGDIOBJ(bmp.0));
    let ok = BitBlt(mem_dc, 0, 0, w, h, Some(screen_dc), 0, 0, SRCCOPY).is_ok();
    let mut buf = vec![0u8; (w as usize) * (h as usize) * 4];
    let mut info = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: w,
            biHeight: -h,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };
    if ok {
        GetDIBits(
            mem_dc,
            bmp,
            0,
            h as u32,
            Some(buf.as_mut_ptr() as *mut _),
            &mut info,
            DIB_RGB_COLORS,
        );
        for px in buf.chunks_exact_mut(4) {
            px[3] = 255;
        }
    }
    SelectObject(mem_dc, old);
    let _ = DeleteObject(HGDIOBJ(bmp.0));
    let _ = DeleteDC(mem_dc);
    ReleaseDC(None, screen_dc);
    if !ok {
        return None;
    }

    let wic_bmp = factory
        .CreateBitmapFromMemory(
            w as u32,
            h as u32,
            &GUID_WICPixelFormat32bppBGRA,
            (w * 4) as u32,
            &buf,
        )
        .ok()?;
    let stream: IStream = CreateStreamOnHGlobal(HGLOBAL::default(), true).ok()?;
    let enc = factory
        .CreateEncoder(&GUID_ContainerFormatJpeg, std::ptr::null())
        .ok()?;
    enc.Initialize(&stream, WICBitmapEncoderNoCache).ok()?;
    let mut frame = None;
    enc.CreateNewFrame(&mut frame, std::ptr::null_mut()).ok()?;
    let frame = frame?;
    frame.Initialize(None).ok()?;
    frame.WriteSource(&wic_bmp, std::ptr::null()).ok()?;
    frame.Commit().ok()?;
    enc.Commit().ok()?;

    let _ = stream.Seek(0, STREAM_SEEK_SET, None);
    let mut out = Vec::new();
    let mut chunk = [0u8; 32768];
    loop {
        let mut read = 0u32;
        let hr = stream.Read(chunk.as_mut_ptr() as *mut _, chunk.len() as u32, Some(&mut read));
        if hr.is_err() || read == 0 {
            break;
        }
        out.extend_from_slice(&chunk[..read as usize]);
    }
    Some(out)
}

fn write_avi_header(f: &mut std::fs::File, w: u32, h: u32, fps: u32) -> std::io::Result<u64> {
    f.write_all(b"RIFF")?;
    f.write_all(&0u32.to_le_bytes())?;
    f.write_all(b"AVI ")?;
    f.write_all(b"LIST")?;
    f.write_all(&(192u32).to_le_bytes())?;
    f.write_all(b"hdrl")?;
    f.write_all(b"avih")?;
    f.write_all(&56u32.to_le_bytes())?;
    let avih_pos = f.stream_position()?;
    f.write_all(&(1_000_000u32 / fps.max(1)).to_le_bytes())?;
    f.write_all(&0u32.to_le_bytes())?;
    f.write_all(&0u32.to_le_bytes())?;
    f.write_all(&0x10u32.to_le_bytes())?; // AVIF_HASINDEX
    f.write_all(&0u32.to_le_bytes())?; // total frames patched later
    f.write_all(&0u32.to_le_bytes())?;
    f.write_all(&1u32.to_le_bytes())?; // streams
    f.write_all(&0u32.to_le_bytes())?;
    f.write_all(&w.to_le_bytes())?;
    f.write_all(&h.to_le_bytes())?;
    f.write_all(&[0u8; 16])?;

    f.write_all(b"LIST")?;
    f.write_all(&116u32.to_le_bytes())?;
    f.write_all(b"strl")?;
    f.write_all(b"strh")?;
    f.write_all(&56u32.to_le_bytes())?;
    f.write_all(b"vids")?;
    f.write_all(b"MJPG")?;
    f.write_all(&[0u8; 12])?;
    f.write_all(&1u32.to_le_bytes())?; // scale
    f.write_all(&fps.max(1).to_le_bytes())?; // rate
    f.write_all(&0u32.to_le_bytes())?;
    f.write_all(&0u32.to_le_bytes())?; // length patched later
    f.write_all(&[0u8; 12])?;
    f.write_all(&(w as u16).to_le_bytes())?;
    f.write_all(&(h as u16).to_le_bytes())?;
    f.write_all(&[0u8; 4])?;

    f.write_all(b"strf")?;
    f.write_all(&40u32.to_le_bytes())?;
    f.write_all(&40u32.to_le_bytes())?;
    f.write_all(&w.to_le_bytes())?;
    f.write_all(&h.to_le_bytes())?;
    f.write_all(&1u16.to_le_bytes())?;
    f.write_all(&24u16.to_le_bytes())?;
    f.write_all(b"MJPG")?;
    f.write_all(&(w * h * 3).to_le_bytes())?;
    f.write_all(&[0u8; 16])?;

    f.write_all(b"LIST")?;
    f.write_all(&0u32.to_le_bytes())?;
    f.write_all(b"movi")?;
    Ok(avih_pos)
}

fn finalize_avi(
    f: &mut std::fs::File,
    avih_pos: u64,
    movi_start: u64,
    index: &[(u32, u32)],
) -> std::io::Result<()> {
    let movi_end = f.stream_position()?;
    let movi_len = (movi_end - movi_start) as u32;
    f.write_all(b"idx1")?;
    f.write_all(&((index.len() * 16) as u32).to_le_bytes())?;
    for &(off, sz) in index {
        f.write_all(b"00dc")?;
        f.write_all(&0x10u32.to_le_bytes())?;
        f.write_all(&off.to_le_bytes())?;
        f.write_all(&sz.to_le_bytes())?;
    }
    let total_end = f.stream_position()?;
    f.seek(SeekFrom::Start(4))?;
    f.write_all(&((total_end - 8) as u32).to_le_bytes())?;
    f.seek(SeekFrom::Start(avih_pos + 16))?;
    f.write_all(&(index.len() as u32).to_le_bytes())?;
    f.seek(SeekFrom::Start(avih_pos + 56 + 12 + 32))?;
    f.write_all(&(index.len() as u32).to_le_bytes())?;
    f.seek(SeekFrom::Start(movi_start - 4))?;
    f.write_all(&movi_len.to_le_bytes())?;
    Ok(())
}
