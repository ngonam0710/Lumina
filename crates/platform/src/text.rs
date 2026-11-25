use crate::icons::Bitmap;
use windows::core::{HSTRING, PCWSTR};
use windows::Win32::Foundation::{COLORREF, RECT, SIZE};
use windows::Win32::Graphics::Gdi::{
    AddFontResourceExW, CreateCompatibleDC, CreateDIBSection, CreateFontW, DeleteDC, DeleteObject,
    DrawTextW, EnumFontFamiliesExW, GetGlyphIndicesW, GetTextExtentPoint32W, SelectObject,
    SetBkMode, SetTextColor, ANTIALIASED_QUALITY, BITMAPINFO, BITMAPINFOHEADER, BI_RGB,
    CLIP_DEFAULT_PRECIS, DEFAULT_CHARSET, DIB_RGB_COLORS, DT_CENTER, DT_END_ELLIPSIS, DT_LEFT,
    DT_NOPREFIX, DT_RIGHT, DT_SINGLELINE, DT_VCENTER, FF_DONTCARE, FONT_RESOURCE_CHARACTERISTICS,
    GGI_MARK_NONEXISTING_GLYPHS, HGDIOBJ, LOGFONTW, OUT_DEFAULT_PRECIS, TEXTMETRICW, TRANSPARENT,
    VARIABLE_PITCH,
};

const SS: i32 = 3;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextAlign {
    Left,
    Center,
    Right,
}

#[derive(Clone, Copy)]
pub struct Style {
    pub px: i32,
    pub weight: i32,
}

fn load_bundled_fonts() -> bool {
    let Ok(exe) = std::env::current_exe() else {
        return false;
    };
    let mut dirs = Vec::new();
    if let Some(p) = exe.parent() {
        dirs.push(p.join("fonts"));
        if let Some(p2) = p.parent().and_then(|d| d.parent()) {
            dirs.push(p2.join("assets").join("fonts"));
        }
    }
    let mut inter = false;
    for dir in dirs {
        let Ok(rd) = std::fs::read_dir(&dir) else {
            continue;
        };
        for e in rd.flatten() {
            let p = e.path();
            let ok = p
                .extension()
                .and_then(|x| x.to_str())
                .is_some_and(|x| x.eq_ignore_ascii_case("ttf") || x.eq_ignore_ascii_case("otf"));
            if ok {
                let w: Vec<u16> = p.to_string_lossy().encode_utf16().chain(std::iter::once(0)).collect();
                let added = unsafe {
                    AddFontResourceExW(PCWSTR(w.as_ptr()), FONT_RESOURCE_CHARACTERISTICS(0x10), None)
                };
                if added > 0
                    && p.file_name()
                        .and_then(|n| n.to_str())
                        .is_some_and(|n| n.to_ascii_lowercase().starts_with("inter-regular"))
                {
                    inter = true;
                }
            }
        }
    }
    inter
}

fn font_exists(face: &str) -> bool {
    unsafe extern "system" fn found(_: *const LOGFONTW, _: *const TEXTMETRICW, _: u32, lp: windows::Win32::Foundation::LPARAM) -> i32 {
        *(lp.0 as *mut bool) = true;
        0
    }
    unsafe {
        let dc = CreateCompatibleDC(None);
        let mut lf = LOGFONTW {
            lfCharSet: DEFAULT_CHARSET,
            ..Default::default()
        };
        for (i, c) in face.encode_utf16().take(31).enumerate() {
            lf.lfFaceName[i] = c;
        }
        let mut hit = false;
        EnumFontFamiliesExW(dc, &lf, Some(found), windows::Win32::Foundation::LPARAM(&mut hit as *mut bool as isize), 0);
        let _ = DeleteDC(dc);
        hit
    }
}

fn renders_latin(face: &str) -> bool {
    const SAMPLE: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
    unsafe {
        let dc = CreateCompatibleDC(None);
        let f = font(&Style { px: 16, weight: 400 }, face);
        let old = SelectObject(dc, HGDIOBJ(f.0));
        let wide: Vec<u16> = SAMPLE.encode_utf16().collect();
        let mut out = vec![0u16; wide.len()];
        let n = GetGlyphIndicesW(
            dc,
            &HSTRING::from(SAMPLE),
            wide.len() as i32,
            out.as_mut_ptr(),
            GGI_MARK_NONEXISTING_GLYPHS,
        );
        SelectObject(dc, old);
        let _ = DeleteObject(HGDIOBJ(f.0));
        let _ = DeleteDC(dc);
        n as usize == wide.len() && out.iter().all(|g| *g != 0xFFFF)
    }
}

pub fn text_face() -> &'static str {
    static FACE: std::sync::OnceLock<&'static str> = std::sync::OnceLock::new();
    FACE.get_or_init(|| {
        let bundled = load_bundled_fonts();
        if bundled && renders_latin("Inter") {
            return "Inter";
        }
        for f in ["Segoe UI Variable Text", "Segoe UI", "Arial"] {
            if font_exists(f) && renders_latin(f) {
                return f;
            }
        }
        "Segoe UI"
    })
}

fn font(style: &Style, face: &str) -> windows::Win32::Graphics::Gdi::HFONT {
    unsafe {
        CreateFontW(
            -style.px,
            0,
            0,
            0,
            style.weight,
            0,
            0,
            0,
            DEFAULT_CHARSET,
            OUT_DEFAULT_PRECIS,
            CLIP_DEFAULT_PRECIS,
            ANTIALIASED_QUALITY,
            (VARIABLE_PITCH.0 | FF_DONTCARE.0) as u32,
            &HSTRING::from(face),
        )
    }
}

pub fn measure(text: &str, style: &Style) -> (i32, i32) {
    if text.is_empty() {
        return (0, (style.px as f32 * 1.3).round() as i32);
    }
    unsafe {
        let dc = CreateCompatibleDC(None);
        let big = Style {
            px: style.px.max(1) * SS,
            weight: style.weight,
        };
        let f = font(&big, text_face());
        let old = SelectObject(dc, HGDIOBJ(f.0));
        let wide: Vec<u16> = text.encode_utf16().collect();
        let mut sz = SIZE::default();
        let _ = GetTextExtentPoint32W(dc, &wide, &mut sz);
        SelectObject(dc, old);
        let _ = DeleteObject(HGDIOBJ(f.0));
        let _ = DeleteDC(dc);
        (((sz.cx + SS - 1) / SS).max(1), ((sz.cy + SS - 1) / SS).max(1))
    }
}

pub fn render(text: &str, style: &Style, w: i32, h: i32) -> Option<Bitmap> {
    render_aligned(text, style, w, h, TextAlign::Left)
}

pub fn render_aligned(
    text: &str,
    style: &Style,
    w: i32,
    h: i32,
    align: TextAlign,
) -> Option<Bitmap> {
    render_face(text, style, text_face(), w, h, align)
}

pub fn ink_y_bounds(bmp: &Bitmap) -> Option<(f32, f32)> {
    let w = bmp.w as usize;
    let h = bmp.h as usize;
    let mut min_y = h;
    let mut max_y = 0usize;
    for y in 0..h {
        let row = &bmp.bgra[y * w * 4..(y + 1) * w * 4];
        if row.chunks_exact(4).any(|px| px[3] > 20) {
            min_y = min_y.min(y);
            max_y = max_y.max(y);
        }
    }
    (min_y <= max_y).then_some((min_y as f32, max_y as f32 + 1.0))
}

pub fn render_face(
    text: &str,
    style: &Style,
    face: &str,
    w: i32,
    h: i32,
    align: TextAlign,
) -> Option<Bitmap> {
    if w <= 0 || h <= 0 || text.is_empty() {
        return None;
    }
    let big = render_raw(
        text,
        &Style {
            px: style.px.max(1) * SS,
            weight: style.weight,
        },
        face,
        w * SS,
        h * SS,
        align,
    )?;
    let (wu, hu, s) = (w as usize, h as usize, SS as usize);
    let mut out = vec![0u8; wu * hu * 4];
    for y in 0..hu {
        for x in 0..wu {
            let mut sum = 0u32;
            for dy in 0..s {
                let row = ((y * s + dy) * wu * s + x * s) * 4;
                for dx in 0..s {
                    sum += big.bgra[row + dx * 4 + 3] as u32;
                }
            }
            let a = (sum / (s * s) as u32) as u8;
            out[(y * wu + x) * 4..(y * wu + x) * 4 + 4].copy_from_slice(&[a, a, a, a]);
        }
    }
    Some(Bitmap {
        w: wu as u32,
        h: hu as u32,
        bgra: out,
    })
}

fn render_raw(
    text: &str,
    style: &Style,
    face: &str,
    w: i32,
    h: i32,
    align: TextAlign,
) -> Option<Bitmap> {
    if w <= 0 || h <= 0 || text.is_empty() {
        return None;
    }
    unsafe {
        let dc = CreateCompatibleDC(None);
        let info = BITMAPINFO {
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
        let mut bits: *mut std::ffi::c_void = std::ptr::null_mut();
        let dib = CreateDIBSection(Some(dc), &info, DIB_RGB_COLORS, &mut bits, None, 0).ok()?;
        if bits.is_null() {
            let _ = DeleteDC(dc);
            return None;
        }
        let old_bmp = SelectObject(dc, HGDIOBJ(dib.0));
        let f = font(style, face);
        let old_font = SelectObject(dc, HGDIOBJ(f.0));
        SetBkMode(dc, TRANSPARENT);
        SetTextColor(dc, COLORREF(0x00FF_FFFF));
        let mut wide: Vec<u16> = text.encode_utf16().collect();
        let mut rc = RECT {
            left: 0,
            top: 0,
            right: w,
            bottom: h,
        };
        let halign = match align {
            TextAlign::Left => DT_LEFT,
            TextAlign::Center => DT_CENTER,
            TextAlign::Right => DT_RIGHT,
        };
        DrawTextW(
            dc,
            &mut wide,
            &mut rc,
            DT_SINGLELINE | DT_VCENTER | halign | DT_END_ELLIPSIS | DT_NOPREFIX,
        );
        let src = std::slice::from_raw_parts(bits as *const u8, (w * h * 4) as usize);
        let mut out = vec![0u8; src.len()];
        for (s, d) in src.chunks_exact(4).zip(out.chunks_exact_mut(4)) {
            let a = s[0].max(s[1]).max(s[2]);
            d.copy_from_slice(&[a, a, a, a]);
        }
        SelectObject(dc, old_font);
        SelectObject(dc, old_bmp);
        let _ = DeleteObject(HGDIOBJ(f.0));
        let _ = DeleteObject(HGDIOBJ(dib.0));
        let _ = DeleteDC(dc);
        Some(Bitmap {
            w: w as u32,
            h: h as u32,
            bgra: out,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn centered_text_is_horizontally_symmetric() {
        let st = Style { px: 12, weight: 700 };
        let (tw, th) = measure("25", &st);
        let w = tw + 8;
        let bmp = render_aligned("25", &st, w, th, TextAlign::Center).expect("bitmap");
        let mut min_x = bmp.w as f32;
        let mut max_x = 0.0f32;
        for y in 0..bmp.h {
            for x in 0..bmp.w {
                let a = bmp.bgra[((y * bmp.w + x) * 4 + 3) as usize];
                if a > 32 {
                    min_x = min_x.min(x as f32);
                    max_x = max_x.max(x as f32 + 1.0);
                }
            }
        }
        let ink_mid = (min_x + max_x) * 0.5;
        let box_mid = bmp.w as f32 * 0.5;
        assert!((ink_mid - box_mid).abs() <= 1.0, "ink_mid={ink_mid}, box_mid={box_mid}");
    }
}
