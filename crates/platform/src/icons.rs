use std::sync::mpsc::{channel, Receiver, Sender};
use windows::core::HSTRING;
use windows::Win32::Foundation::SIZE;
use windows::Win32::Graphics::Gdi::{
    DeleteObject, GetDC, GetDIBits, GetObjectW, ReleaseDC, BITMAP, BITMAPINFO, BITMAPINFOHEADER,
    BI_RGB, DIB_RGB_COLORS, HGDIOBJ,
};
use windows::Win32::Graphics::Imaging::{
    CLSID_WICImagingFactory, GUID_WICPixelFormat32bppPBGRA, IWICImagingFactory,
    WICBitmapDitherTypeNone, WICBitmapInterpolationModeFant, WICBitmapPaletteTypeCustom,
    WICDecodeMetadataCacheOnDemand, WICRect,
};
use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED};
use windows::Win32::UI::Shell::{
    IShellItemImageFactory, SHCreateItemFromParsingName, SHCreateMemStream, SIIGBF_BIGGERSIZEOK,
    SIIGBF_ICONONLY,
};

#[derive(Clone)]
pub struct Bitmap {
    pub w: u32,
    pub h: u32,
    pub bgra: Vec<u8>,
}

pub fn fit(b: Bitmap, px: u32) -> Bitmap {
    if b.w <= px && b.h <= px {
        return b;
    }
    let k = (px as f32 / b.w.max(b.h) as f32).min(1.0);
    let (ow, oh) = (
        ((b.w as f32 * k).round() as u32).max(1),
        ((b.h as f32 * k).round() as u32).max(1),
    );
    let (sx, sy) = (b.w as f32 / ow as f32, b.h as f32 / oh as f32);
    let mut out = vec![0u8; (ow * oh * 4) as usize];
    for y in 0..oh {
        let (y0, y1) = (y as f32 * sy, (y + 1) as f32 * sy);
        for x in 0..ow {
            let (x0, x1) = (x as f32 * sx, (x + 1) as f32 * sx);
            let mut acc = [0f32; 4];
            let mut wsum = 0f32;
            let mut yy = y0.floor() as u32;
            while (yy as f32) < y1 && yy < b.h {
                let wy = ((yy + 1) as f32).min(y1) - (yy as f32).max(y0);
                let mut xx = x0.floor() as u32;
                while (xx as f32) < x1 && xx < b.w {
                    let wx = ((xx + 1) as f32).min(x1) - (xx as f32).max(x0);
                    let w = wx * wy;
                    let i = ((yy * b.w + xx) * 4) as usize;
                    for c in 0..4 {
                        acc[c] += b.bgra[i + c] as f32 * w;
                    }
                    wsum += w;
                    xx += 1;
                }
                yy += 1;
            }
            let o = ((y * ow + x) * 4) as usize;
            for c in 0..4 {
                out[o + c] = (acc[c] / wsum.max(1e-6)).round().clamp(0.0, 255.0) as u8;
            }
        }
    }
    Bitmap {
        w: ow,
        h: oh,
        bgra: out,
    }
}

pub fn decode_square(bytes: &[u8], px: u32) -> Option<Bitmap> {
    unsafe {
        let factory: IWICImagingFactory =
            CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER).ok()?;
        let stream = SHCreateMemStream(Some(bytes))?;
        let decoder = factory
            .CreateDecoderFromStream(&stream, std::ptr::null(), WICDecodeMetadataCacheOnDemand)
            .ok()?;
        let frame = decoder.GetFrame(0).ok()?;
        let (mut w, mut h) = (0u32, 0u32);
        frame.GetSize(&mut w, &mut h).ok()?;
        if w == 0 || h == 0 {
            return None;
        }
        let side = w.min(h);
        let (x, y) = ((w - side) / 2, (h - side) / 2);
        let clipper = factory.CreateBitmapClipper().ok()?;
        clipper
            .Initialize(
                &frame,
                &WICRect {
                    X: x as i32,
                    Y: y as i32,
                    Width: side as i32,
                    Height: side as i32,
                },
            )
            .ok()?;
        let scaler = factory.CreateBitmapScaler().ok()?;
        scaler
            .Initialize(&clipper, px, px, WICBitmapInterpolationModeFant)
            .ok()?;
        let conv = factory.CreateFormatConverter().ok()?;
        conv.Initialize(
            &scaler,
            &GUID_WICPixelFormat32bppPBGRA,
            WICBitmapDitherTypeNone,
            None,
            0.0,
            WICBitmapPaletteTypeCustom,
        )
        .ok()?;
        let mut buf = vec![0u8; (px * px * 4) as usize];
        conv.CopyPixels(std::ptr::null(), px * 4, &mut buf).ok()?;
        Some(Bitmap {
            w: px,
            h: px,
            bgra: buf,
        })
    }
}

pub fn load_shell_icon(path: &str, px: i32) -> Option<Bitmap> {
    let path = path.split('?').next().unwrap_or(path);
    unsafe {
        let factory: IShellItemImageFactory =
            SHCreateItemFromParsingName(&HSTRING::from(path), None).ok()?;
        let req = (px * 2).clamp(64, 256);
        let hbmp = factory
            .GetImage(SIZE { cx: req, cy: req }, SIIGBF_ICONONLY | SIIGBF_BIGGERSIZEOK)
            .ok()?;
        let mut bm = BITMAP::default();
        let got = GetObjectW(
            HGDIOBJ(hbmp.0),
            std::mem::size_of::<BITMAP>() as i32,
            Some(&mut bm as *mut _ as *mut _),
        );
        if got == 0 || bm.bmWidth <= 0 || bm.bmHeight <= 0 {
            let _ = DeleteObject(HGDIOBJ(hbmp.0));
            return None;
        }
        let (w, h) = (bm.bmWidth as u32, bm.bmHeight as u32);
        let mut info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: w as i32,
                biHeight: -(h as i32),
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut buf = vec![0u8; (w * h * 4) as usize];
        let dc = GetDC(None);
        let lines = GetDIBits(dc, hbmp, 0, h, Some(buf.as_mut_ptr() as *mut _), &mut info, DIB_RGB_COLORS);
        ReleaseDC(None, dc);
        let _ = DeleteObject(HGDIOBJ(hbmp.0));
        if lines == 0 {
            return None;
        }
        Some(fit(Bitmap { w, h, bgra: buf }, px.max(1) as u32))
    }
}

pub struct Worker {
    pub requests: Sender<String>,
    pub results: Receiver<(String, Option<Bitmap>)>,
}

pub fn spawn_worker(px: i32, notify: impl Fn() + Send + 'static) -> Worker {
    let (req_tx, req_rx) = channel::<String>();
    let (res_tx, res_rx) = channel();
    std::thread::spawn(move || {
        let com = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED).is_ok() };
        while let Ok(path) = req_rx.recv() {
            let bmp = load_shell_icon(&path, px);
            if res_tx.send((path, bmp)).is_err() {
                break;
            }
            notify();
        }
        if com {
            unsafe { CoUninitialize() };
        }
    });
    Worker {
        requests: req_tx,
        results: res_rx,
    }
}
