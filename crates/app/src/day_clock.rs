use crate::prefs;
use np_core::island::{calendar, greeting_text};
use np_core::settings::Lang;
use np_platform::{monitors, startup, time};
use np_ui::gfx::Gfx;
use np_ui::host::{attach, Surface};
use np_ui::painter::{Anchor, Painter};
use std::cell::RefCell;
use windows::core::{w, PCWSTR};
use windows::UI::Composition::{Compositor, ContainerVisual};
use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::{ReleaseCapture, SetCapture};
use windows::Win32::UI::WindowsAndMessaging::*;
use windows_numerics::Vector2;

const T_TICK: usize = 1;
const CMD_RESET: usize = 1;
const CMD_NEXT_COLOR: usize = 2;
const CMD_SETTINGS: usize = 3;
const CMD_HIDE: usize = 4;

const COLORS: [[u8; 3]; 7] = [
    [255, 255, 255],
    [220, 234, 255],
    [255, 224, 196],
    [255, 198, 218],
    [192, 248, 220],
    [214, 198, 255],
    [245, 92, 92],
];

struct DayClock {
    hwnd: HWND,
    surface: Surface,
    content: ContainerVisual,
    painter: Painter,
    dpi: f32,
    w: i32,
    h: i32,
    drag: Option<(i32, i32)>,
    last_sig: String,
    open_settings_cb: fn(),
}

thread_local! {
    static STATE: RefCell<Option<DayClock>> = const { RefCell::new(None) };
}

fn with<R>(f: impl FnOnce(&mut DayClock) -> R) -> Option<R> {
    STATE.with(|c| c.try_borrow_mut().ok().and_then(|mut b| b.as_mut().map(f)))
}

pub fn init(comp: &Compositor, open_settings_cb: fn()) -> windows_core::Result<()> {
    unsafe {
        let hi: HINSTANCE = GetModuleHandleW(None)?.into();
        let class = w!("LuminaDayClock");
        RegisterClassW(&WNDCLASSW {
            lpfnWndProc: Some(proc),
            hInstance: hi,
            lpszClassName: class,
            hCursor: LoadCursorW(None, IDC_SIZEALL)?,
            ..Default::default()
        });
        let hwnd = CreateWindowExW(
            WS_EX_NOREDIRECTIONBITMAP | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
            class,
            w!("np-day-clock"),
            WS_POPUP,
            0,
            0,
            460,
            130,
            None,
            None,
            Some(hi),
            None,
        )?;
        let mon = monitors::primary();
        let dpi = mon.scale();
        let surface = attach(comp, hwnd, (460.0, 130.0))?;
        let content = comp.CreateContainerVisual()?;
        surface.root.Children()?.InsertAtTop(&content)?;
        let gfx = Gfx::shared(comp)?;
        let painter = Painter::new(comp.clone(), gfx, dpi);
        STATE.with(|c| {
            *c.borrow_mut() = Some(DayClock {
                hwnd,
                surface,
                content,
                painter,
                dpi,
                w: 460,
                h: 130,
                drag: None,
                last_sig: String::new(),
                open_settings_cb,
            });
        });
        prefs::listen(hwnd);
        SetTimer(Some(hwnd), T_TICK, 1000, None);
    }
    with(|d| d.apply_layout_and_paint(true));
    Ok(())
}

pub fn shutdown() {
    if let Some(d) = STATE.with(|c| c.borrow_mut().take()) {
        let hwnd = d.hwnd;
        prefs::unlisten(hwnd);
        drop(d);
        unsafe {
            let _ = DestroyWindow(hwnd);
        }
    }
}

impl DayClock {
    fn apply_layout_and_paint(&mut self, force: bool) {
        let sp = prefs::get();
        let cfg = sp.day_clock;
        if !cfg.enabled {
            unsafe {
                let _ = ShowWindow(self.hwnd, SW_HIDE);
            }
            return;
        }
        let n = time::local_now();
        let sig = format!(
            "{}-{}-{}-{}:{}:{}-{:?}-{}-{}-{:?}-{}-{}",
            n.year,
            n.month,
            n.day,
            n.hour,
            n.minute,
            cfg.size_pct,
            cfg.color,
            sp.clock_24h,
            sp.day_clock_greeting,
            sp.lang,
            sp.greet_name,
            sp.greet_text,
        );
        if !force && sig == self.last_sig {
            return;
        }
        self.last_sig = sig;

        let scale = cfg.size_pct.clamp(50, 220) as f32 / 100.0;
        let mon = monitors::primary();
        self.dpi = mon.scale();
        let k = self.dpi * scale;

        let wd = calendar::weekday(n.year, n.month, n.day);
        let day_word = calendar::weekday_upper(wd);
        let month_name = calendar::month_upper(n.month);
        let date_str = format!("{} {}, {}.", n.day, month_name, n.year);
        let time_str = if sp.clock_24h {
            format!("\u{2022} {:02}:{:02} \u{2022}", n.hour, n.minute)
        } else {
            let h12 = if n.hour % 12 == 0 { 12 } else { n.hour % 12 };
            let ampm = if n.hour < 12 { "AM" } else { "PM" };
            format!("\u{2022} {h12:02}:{:02} {ampm} \u{2022}", n.minute)
        };

        let greet_line = if sp.day_clock_greeting {
            let vi = match sp.lang {
                Lang::Vi => true,
                Lang::En => false,
                Lang::System => startup::system_is_vietnamese(),
            };
            let name = if sp.greet_name.trim().is_empty() {
                std::env::var("USERNAME").unwrap_or_default()
            } else {
                sp.greet_name.clone()
            };
            Some(greeting_text(&sp.greet_text, &name, n.hour, vi))
        } else {
            None
        };

        self.w = (540.0 * k).ceil().max(260.0) as i32;
        self.h = ((if greet_line.is_some() { 164.0 } else { 138.0 }) * k).ceil().max(76.0) as i32;
        let (x, y) = if cfg.custom_pos {
            (cfg.x, cfg.y)
        } else {
            (
                mon.area.x + (mon.area.w as f32 * 0.14) as i32,
                mon.area.y + (mon.area.h as f32 * 0.24) as i32,
            )
        };

        unsafe {
            let _ = SetWindowPos(
                self.hwnd,
                Some(HWND_BOTTOM),
                x,
                y,
                self.w,
                self.h,
                SWP_NOACTIVATE | SWP_SHOWWINDOW,
            );
        }
        let _ = self.surface.root.SetSize(Vector2 {
            X: self.w as f32,
            Y: self.h as f32,
        });

        if let Ok(kids) = self.content.Children() {
            let _ = kids.RemoveAll();
        }
        self.painter.reset(k);
        let cx = self.w as f32 / 2.0;
        let mut y_top = 8.0 * k;
        if let Some(greet) = greet_line {
            self.painter.text_shadow(
                &self.content,
                &greet,
                cx,
                y_top,
                11.8,
                600,
                cfg.color,
                0.86,
                Anchor::Center,
                self.w as f32 - 20.0 * k,
            );
            y_top += 22.0 * k;
        }
        let glyph_h = 34.0 * k;
        let spacing = 22.0 * k;
        let (_, ah) = self
            .painter
            .anurati_centered(&self.content, day_word, cx, y_top, glyph_h, spacing, cfg.color);
        let y_date = y_top + ah + 14.0 * k;
        self.painter.text_shadow(
            &self.content,
            &date_str,
            cx,
            y_date,
            14.0,
            600,
            cfg.color,
            0.94,
            Anchor::Center,
            self.w as f32,
        );
        self.painter.text_shadow(
            &self.content,
            &time_str,
            cx,
            y_date + 25.0 * k,
            12.2,
            600,
            cfg.color,
            0.88,
            Anchor::Center,
            self.w as f32,
        );
    }

    fn show_menu(&mut self) {
        unsafe {
            let Ok(menu) = CreatePopupMenu() else { return };
            let to_w = |s: &str| -> Vec<u16> { s.encode_utf16().chain(std::iter::once(0)).collect() };
            let w_col = to_w(prefs::tr("Cycle clock colour", "Đổi màu đồng hồ"));
            let w_rst = to_w(prefs::tr("Reset position & size", "Đặt lại vị trí & kích thước"));
            let w_set = to_w(prefs::tr("Widget Settings…", "Cài đặt Tiện ích…"));
            let w_hide = to_w(prefs::tr("Hide Day Clock", "Ẩn Day Clock"));
            let _ = AppendMenuW(menu, MF_STRING, CMD_NEXT_COLOR, PCWSTR(w_col.as_ptr()));
            let _ = AppendMenuW(menu, MF_STRING, CMD_RESET, PCWSTR(w_rst.as_ptr()));
            let _ = AppendMenuW(menu, MF_STRING, CMD_SETTINGS, PCWSTR(w_set.as_ptr()));
            let _ = AppendMenuW(menu, MF_SEPARATOR, 0, windows::core::PCWSTR::null());
            let _ = AppendMenuW(menu, MF_STRING, CMD_HIDE, PCWSTR(w_hide.as_ptr()));
            let mut pt = POINT::default();
            let _ = GetCursorPos(&mut pt);
            let _ = SetForegroundWindow(self.hwnd);
            let cmd = TrackPopupMenu(
                menu,
                TPM_RETURNCMD | TPM_RIGHTBUTTON,
                pt.x,
                pt.y,
                Some(0),
                self.hwnd,
                None,
            );
            let _ = DestroyMenu(menu);
            match cmd.0 as usize {
                CMD_NEXT_COLOR => {
                    prefs::update(|s| {
                        let i = COLORS
                            .iter()
                            .position(|c| *c == s.day_clock.color)
                            .unwrap_or(0);
                        s.day_clock.color = COLORS[(i + 1) % COLORS.len()];
                    });
                }
                CMD_RESET => {
                    prefs::update(|s| {
                        s.day_clock.size_pct = 100;
                        s.day_clock.custom_pos = false;
                        s.day_clock.color = [255, 255, 255];
                    });
                }
                CMD_SETTINGS => (self.open_settings_cb)(),
                CMD_HIDE => {
                    prefs::update(|s| s.day_clock.enabled = false);
                }
                _ => {}
            }
        }
    }
}

unsafe extern "system" fn proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        prefs::WM_PREFS => {
            with(|d| d.apply_layout_and_paint(true));
            LRESULT(0)
        }
        WM_TIMER if wp.0 == T_TICK => {
            with(|d| d.apply_layout_and_paint(false));
            LRESULT(0)
        }
        WM_LBUTTONDOWN => {
            let mut pt = POINT::default();
            let _ = GetCursorPos(&mut pt);
            let mut r = windows::Win32::Foundation::RECT::default();
            let _ = GetWindowRect(hwnd, &mut r);
            with(|d| {
                d.drag = Some((pt.x - r.left, pt.y - r.top));
                SetCapture(hwnd);
            });
            LRESULT(0)
        }
        WM_MOUSEMOVE => {
            let mut pt = POINT::default();
            let _ = GetCursorPos(&mut pt);
            with(|d| {
                if let Some((gx, gy)) = d.drag {
                    let nx = pt.x - gx;
                    let ny = pt.y - gy;
                    let _ = SetWindowPos(
                        hwnd,
                        Some(HWND_BOTTOM),
                        nx,
                        ny,
                        0,
                        0,
                        SWP_NOSIZE | SWP_NOACTIVATE,
                    );
                }
            });
            LRESULT(0)
        }
        WM_LBUTTONUP => {
            with(|d| {
                if d.drag.take().is_some() {
                    let _ = ReleaseCapture();
                    let mut r = windows::Win32::Foundation::RECT::default();
                    let _ = GetWindowRect(hwnd, &mut r);
                    prefs::update(|s| {
                        s.day_clock.x = r.left;
                        s.day_clock.y = r.top;
                        s.day_clock.custom_pos = true;
                    });
                }
            });
            LRESULT(0)
        }
        WM_MOUSEWHEEL => {
            let delta = ((wp.0 >> 16) as i16) as i32;
            let step = if delta > 0 { 8 } else { -8 };
            prefs::update(|s| {
                s.day_clock.size_pct = (s.day_clock.size_pct as i32 + step).clamp(55, 200) as u8;
            });
            LRESULT(0)
        }
        WM_RBUTTONUP => {
            with(|d| d.show_menu());
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}
