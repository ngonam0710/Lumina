use crate::{paths, prefs};
use crate::popup::Popup;
use np_core::island::{calendar, FocusTimer, Phase, Stage};
use np_core::settings::surface_spec;
use np_core::symbols::Sym;
use np_core::widgets::{format_speed, DesktopWidget, SystemMetrics, WidgetKind};
use np_platform::{monitors, sys, time, weather};
use np_ui::gfx::Gfx;
use np_ui::painter::{rgba, Anchor, Painter, WHITE};
use std::cell::RefCell;
use std::time::Instant;
use windows::core::{w, PCWSTR};
use windows::UI::Composition::Compositor;
use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::{ReleaseCapture, SetCapture};
use windows::Win32::UI::WindowsAndMessaging::*;

const T_TICK: usize = 1;
const CMD_REMOVE: usize = 1;
const CMD_SETTINGS: usize = 2;

struct LiveWidget {
    kind: WidgetKind,
    popup: Popup,
    painter: Painter,
    x: i32,
    y: i32,
    w: f32,
    h: f32,
}

struct Drag {
    idx: usize,
    grab_x: i32,
    grab_y: i32,
}

struct Mgr {
    ctl: HWND,
    comp: Compositor,
    gfx: Gfx,
    dpi: f32,
    list: Vec<LiveWidget>,
    drag: Option<Drag>,
    metrics: SystemMetrics,
    timer: FocusTimer,
    t0: Instant,
    open_settings_cb: fn(),
}

thread_local! {
    static MGR: RefCell<Option<Mgr>> = const { RefCell::new(None) };
}

fn with<R>(f: impl FnOnce(&mut Mgr) -> R) -> Option<R> {
    MGR.with(|c| c.try_borrow_mut().ok().and_then(|mut b| b.as_mut().map(f)))
}

pub fn init(comp: &Compositor, open_settings_cb: fn()) -> windows_core::Result<()> {
    unsafe {
        let hi: HINSTANCE = GetModuleHandleW(None)?.into();
        let class = w!("LuminaWidgetsCtl");
        RegisterClassW(&WNDCLASSW {
            lpfnWndProc: Some(ctl_proc),
            hInstance: hi,
            lpszClassName: class,
            ..Default::default()
        });
        let ctl = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            class,
            w!("np-widgets-ctl"),
            WS_POPUP,
            0,
            0,
            0,
            0,
            Some(HWND_MESSAGE),
            None,
            Some(hi),
            None,
        )?;
        let mon = monitors::primary();
        let gfx = Gfx::shared(comp)?;
        let cfg = prefs::get();
        MGR.with(|c| {
            *c.borrow_mut() = Some(Mgr {
                ctl,
                comp: comp.clone(),
                gfx,
                dpi: mon.scale(),
                list: Vec::new(),
                drag: None,
                metrics: SystemMetrics::default(),
                timer: FocusTimer::new(cfg.focus_mins, cfg.break_mins),
                t0: Instant::now(),
                open_settings_cb,
            });
        });
        prefs::listen(ctl);
        SetTimer(Some(ctl), T_TICK, 1000, None);
    }
    with(|m| m.load());
    Ok(())
}

pub fn has_widget(kind: WidgetKind) -> bool {
    with(|m| m.list.iter().any(|w| w.kind == kind)).unwrap_or(false)
}

pub fn toggle_widget(kind: WidgetKind) {
    with(|m| {
        if let Some(i) = m.list.iter().position(|w| w.kind == kind) {
            m.remove_at(i);
        } else {
            let mon = monitors::primary();
            let idx = m.list.len() as i32;
            let x = mon.work.x + 110 + (idx % 2) * 232;
            let y = mon.work.y + 390 + (idx / 2) * 110;
            m.spawn(kind, x, y);
        }
        m.save();
    });
}

pub fn spawn_at(kind: WidgetKind, screen_x: i32, screen_y: i32) {
    with(|m| {
        if let Some(i) = m.list.iter().position(|w| w.kind == kind) {
            m.move_to(i, screen_x, screen_y);
        } else {
            m.spawn(kind, screen_x, screen_y);
        }
        m.save();
    });
}

pub fn preview_at(kind: WidgetKind, screen_x: i32, screen_y: i32) {
    with(|m| {
        if let Some(i) = m.list.iter().position(|w| w.kind == kind) {
            m.move_to(i, screen_x, screen_y);
        } else {
            m.spawn(kind, screen_x, screen_y);
        }
    });
}

pub fn present_all() -> usize {
    with(|m| m.list.len()).unwrap_or(0)
}

pub fn shutdown() {
    if let Some(mut m) = MGR.with(|c| c.borrow_mut().take()) {
        let ctl = m.ctl;
        prefs::unlisten(ctl);
        let hwnds: Vec<HWND> = m.list.iter().map(|w| w.popup.hwnd).collect();
        m.list.clear();
        drop(m);
        unsafe {
            for h in hwnds {
                let _ = DestroyWindow(h);
            }
            let _ = DestroyWindow(ctl);
        }
    }
}

impl Mgr {
    fn load(&mut self) {
        let Ok(txt) = std::fs::read_to_string(paths::widgets_file()) else {
            return;
        };
        let saved: Vec<DesktopWidget> = serde_json::from_str(&txt).unwrap_or_default();
        for w in saved {
            self.spawn(w.kind, w.x, w.y);
        }
    }

    fn save(&self) {
        let out: Vec<DesktopWidget> = self
            .list
            .iter()
            .map(|w| DesktopWidget {
                kind: w.kind,
                x: w.x,
                y: w.y,
            })
            .collect();
        let _ = std::fs::create_dir_all(paths::config_dir());
        if let Ok(json) = serde_json::to_string_pretty(&out) {
            let _ = std::fs::write(paths::widgets_file(), json);
        }
    }

    fn radius(&self) -> f32 {
        (22.0 * self.dpi * prefs::get().corner_scale()).max(4.0)
    }

    fn spawn(&mut self, kind: WidgetKind, x: i32, y: i32) {
        let (w, h) = kind.desktop_size(self.dpi);
        let mon = monitors::primary();
        let x = x.clamp(mon.area.x + 8, (mon.area.right() - w as i32 - 8).max(mon.area.x + 8));
        let y = y.clamp(mon.area.y + 8, (mon.area.bottom() - h as i32 - 8).max(mon.area.y + 8));
        let glass = prefs::glass_params((w, h), self.radius());
        let Ok(mut popup) = Popup::new(
            &self.comp,
            w!("LuminaDesktopWidget"),
            Some(widget_proc),
            WINDOW_EX_STYLE(0),
            glass,
            false,
        ) else {
            return;
        };
        let _ = popup.set_rect(x, y, w.ceil() as i32, h.ceil() as i32);
        let _ = popup.panel.set_params(glass, prefs::auto_tier());
        unsafe {
            let _ = SetWindowPos(
                popup.hwnd,
                Some(HWND_BOTTOM),
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
            );
        }
        popup.show();
        let painter = Painter::new(self.comp.clone(), self.gfx.clone(), self.dpi);
        self.list.push(LiveWidget {
            kind,
            popup,
            painter,
            x,
            y,
            w,
            h,
        });
        let idx = self.list.len() - 1;
        self.paint(idx);
    }

    fn remove_at(&mut self, i: usize) {
        if i < self.list.len() {
            let w = self.list.remove(i);
            unsafe {
                let _ = DestroyWindow(w.popup.hwnd);
            }
        }
    }

    fn move_to(&mut self, i: usize, x: i32, y: i32) {
        let Some(wd) = self.list.get_mut(i) else { return };
        wd.x = x;
        wd.y = y;
        unsafe {
            let _ = SetWindowPos(
                wd.popup.hwnd,
                Some(HWND_BOTTOM),
                x,
                y,
                0,
                0,
                SWP_NOSIZE | SWP_NOACTIVATE,
            );
        }
    }

    fn refresh_theme(&mut self) {
        let s = prefs::get();
        self.timer.sync_config(s.focus_mins, s.break_mins);
        let tier = prefs::auto_tier();
        let r = self.radius();
        for i in 0..self.list.len() {
            let size = (self.list[i].w, self.list[i].h);
            let _ = self.list[i].popup.panel.set_params(prefs::glass_params(size, r), tier);
            let _ = self.list[i].popup.panel.set_radius(r);
            self.paint(i);
        }
    }

    fn tick(&mut self) {
        let now_ms = self.t0.elapsed().as_millis() as i64;
        self.timer.tick(now_ms);
        let (cpu, gpu, temp, ram, down, up) = sys::telemetry_snapshot();
        self.metrics.push_sample(cpu, gpu, ram);
        self.metrics.gpu_temp_c = temp;
        self.metrics.net_down_bps = down;
        self.metrics.net_up_bps = up;
        for i in 0..self.list.len() {
            self.paint(i);
        }
    }

    fn paint(&mut self, idx: usize) {
        let Some(wd) = self.list.get_mut(idx) else { return };
        let content = wd.popup.content.clone();
        if let Ok(k) = content.Children() {
            let _ = k.RemoveAll();
        }
        let d = self.dpi;
        let (w, h) = (wd.w, wd.h);
        let fg = surface_spec(prefs::get().surface).fg;
        let pad = 14.0 * d;
        let p = &mut wd.painter;

        match wd.kind {
            WidgetKind::Weather => {
                p.grad_rect(&content, 0.0, 0.0, w, h, 0.0, rgba([82, 152, 255], 34), rgba([255, 198, 92], 8), true);
                let wx = weather::latest();
                let vi = prefs::is_vi();
                let cond = if vi {
                    match wx.condition.as_str() {
                        "Partly cloudy" => "Ít mây",
                        "Sunny" | "Clear" => "Trời quang",
                        "Cloudy" | "Overcast" => "Nhiều mây",
                        "Rain" | "Light rain" => "Mưa nhẹ",
                        "Showers" => "Mưa rào",
                        "Thunderstorm" => "Dông",
                        other => other,
                    }
                } else {
                    wx.condition.as_str()
                };
                p.text(
                    &content,
                    &format!("{}°", wx.temp_c),
                    pad,
                    11.0 * d,
                    24.0,
                    700,
                    fg,
                    1.0,
                    Anchor::Left,
                    w * 0.55,
                );
                p.layered_weather(&content, w - pad - 16.0 * d, 25.0 * d, 26.0);
                p.text(
                    &content,
                    cond,
                    pad,
                    h - 30.0 * d,
                    11.0,
                    600,
                    fg,
                    0.92,
                    Anchor::Left,
                    w * 0.55,
                );
                p.card(&content, w - pad - 72.0 * d, h - 32.0 * d, 72.0 * d, 18.0 * d, 9.0 * d, fg, 18, 26);
                let hl = if vi {
                    format!("C:{}° T:{}°", wx.high_c, wx.low_c)
                } else {
                    format!("H:{}° L:{}°", wx.high_c, wx.low_c)
                };
                p.text(
                    &content,
                    &hl,
                    w - pad - 36.0 * d,
                    h - 29.5 * d,
                    9.5,
                    600,
                    fg,
                    0.82,
                    Anchor::Center,
                    68.0 * d,
                );
            }
            WidgetKind::SystemMonitor => {
                let m = &self.metrics;
                p.sparkline(&content, pad, h - 35.0 * d, w - 2.0 * pad, 16.0 * d, &m.cpu_hist, [96, 175, 255]);
                let rows: [(&str, String, f32, [u8; 3]); 3] = [
                    ("CPU", format!("{}%", m.cpu_pct), m.cpu_pct as f32 / 100.0, [96, 175, 255]),
                    (
                        "GPU",
                        match m.gpu_temp_c {
                            Some(t) => format!("{}%  {}°", m.gpu_pct, t),
                            None => format!("{}%", m.gpu_pct),
                        },
                        m.gpu_pct as f32 / 100.0,
                        [255, 118, 92],
                    ),
                    ("RAM", format!("{}%", m.ram_pct), m.ram_pct as f32 / 100.0, [64, 218, 128]),
                ];
                let mut y = 10.0 * d;
                let bar_x = pad + 38.0 * d;
                let bar_w = w - bar_x - pad - 56.0 * d;
                for (name, val, frac, col) in rows {
                    p.text(&content, name, pad, y, 10.5, 700, fg, 0.74, Anchor::Left, 36.0 * d);
                    p.rect(&content, bar_x, y + 6.0 * d, bar_w, 5.0 * d, 2.5 * d, rgba(WHITE, 32));
                    p.grad_rect(
                        &content,
                        bar_x,
                        y + 6.0 * d,
                        (bar_w * frac.clamp(0.0, 1.0)).max(5.0 * d),
                        5.0 * d,
                        2.5 * d,
                        rgba(col, 245),
                        rgba(WHITE, 215),
                        true,
                    );
                    p.text(&content, &val, w - pad, y, 10.5, 600, fg, 0.95, Anchor::Right, 58.0 * d);
                    y += 17.5 * d;
                }
                let net = format!(
                    "↓ {}   ↑ {}",
                    format_speed(m.net_down_bps),
                    format_speed(m.net_up_bps)
                );
                p.text(&content, &net, w / 2.0, h - 22.0 * d, 9.8, 600, fg, 0.80, Anchor::Center, w - 2.0 * pad);
            }
            WidgetKind::Calendar => {
                let n = time::local_now();
                let vi = prefs::is_vi();
                let wd_i = calendar::weekday(n.year, n.month, n.day) as usize;
                let wd_s = if vi {
                    ["CN", "T2", "T3", "T4", "T5", "T6", "T7"][wd_i % 7]
                } else {
                    calendar::weekday_short(wd_i as u32)
                };
                let mo_s = if vi {
                    format!("Th{}", n.month)
                } else {
                    calendar::month_short(n.month).to_string()
                };
                let wk = calendar::iso_week(n.year, n.month, n.day);
                p.text(&content, &format!("{:02}", n.day), pad, 9.0 * d, 22.0, 700, fg, 1.0, Anchor::Left, 44.0 * d);
                p.text_shadow(&content, wd_s, pad + 44.0 * d, 10.5 * d, 10.0, 700, [255, 114, 108], 1.0, Anchor::Left, 44.0 * d);
                p.text(&content, &mo_s, pad + 44.0 * d, 23.0 * d, 10.0, 600, fg, 0.78, Anchor::Left, 44.0 * d);
                let wk_x = w - pad - 58.0 * d;
                let wk_y = 11.0 * d;
                let wk_w = 58.0 * d;
                let wk_h = 18.0 * d;
                p.card(&content, wk_x, wk_y, wk_w, wk_h, 9.0 * d, fg, 22, 28);
                let wk_lbl = if vi { format!("Tuần {wk}") } else { format!("Wk {wk}") };
                p.pill_text(&content, &wk_lbl, wk_x + wk_w / 2.0, wk_y + wk_h / 2.0, 9.0, 600, fg, 0.84, wk_w - 4.0 * d);

                let gx = pad;
                let gy = 43.0 * d;
                let cw = (w - 2.0 * pad) / 7.0;
                let first = calendar::weekday(n.year, n.month, 1);
                let dim = calendar::days_in_month(n.year, n.month);
                let rows_needed = ((first + dim - 1) / 7 + 1).max(5);
                let ch = ((h - gy - 14.0 * d) / (rows_needed as f32 + 1.0)).clamp(12.2 * d, 15.0 * d);
                let hdrs = if vi {
                    ["CN", "T2", "T3", "T4", "T5", "T6", "T7"]
                } else {
                    ["S", "M", "T", "W", "T", "F", "S"]
                };
                for (k, ch_lbl) in hdrs.iter().enumerate() {
                    p.pill_text(&content, ch_lbl, gx + cw * (k as f32 + 0.5), gy + ch * 0.45, 8.2, 600, fg, 0.48, cw);
                }
                for day in 1..=dim {
                    let idx = first + day - 1;
                    let row = idx / 7;
                    if row >= 6 {
                        break;
                    }
                    let cx = gx + cw * ((idx % 7) as f32 + 0.5);
                    let cy_mid = gy + ch * (row as f32 + 1.45);
                    let today = day == n.day;
                    if today {
                        p.grad_rect(&content, cx - 8.5 * d, cy_mid - 7.5 * d, 17.0 * d, 15.0 * d, 7.5 * d, rgba([255, 102, 96], 248), rgba([224, 52, 74], 248), false);
                    }
                    p.pill_text(&content, &day.to_string(), cx, cy_mid, 8.8, if today { 700 } else { 500 }, fg, if today { 1.0 } else { 0.86 }, cw);
                }
            }
            WidgetKind::FocusTimer => {
                let vi = prefs::is_vi();
                let ring_col = if self.timer.stage == Stage::Break { [72, 224, 146] } else { [255, 168, 62] };
                p.ring_progress(&content, pad + 18.0 * d, h / 2.0, 38.0 * d, 3.6 * d, self.timer.elapsed_frac(), fg, ring_col);
                p.sym(&content, Sym::Clock, pad + 18.0 * d, h / 2.0, 14.0, ring_col, 0.95);
                p.text(&content, &self.timer.mmss(), pad + 46.0 * d, 22.0 * d, 23.0, 700, fg, 1.0, Anchor::Left, w * 0.52);
                let sub = match (self.timer.phase, self.timer.stage, vi) {
                    (Phase::Running, Stage::Break, true) => "Đang nghỉ ngơi",
                    (Phase::Running, Stage::Break, false) => "Break active",
                    (Phase::Running, Stage::Focus, true) => "Đang tập trung…",
                    (Phase::Running, Stage::Focus, false) => "Focusing…",
                    (Phase::Paused, _, true) => "Đã tạm dừng",
                    (Phase::Paused, _, false) => "Paused",
                    (_, _, true) => "Đồng hồ tập trung",
                    (_, _, false) => "Focus timer",
                };
                p.text(&content, sub, pad + 46.0 * d, 56.0 * d, 10.5, 600, ring_col, 0.90, Anchor::Left, w * 0.52);
                let min_txt = if vi {
                    format!("{} + {} phút", self.timer.focus_secs / 60, self.timer.break_secs / 60)
                } else {
                    format!("{} + {} min", self.timer.focus_secs / 60, self.timer.break_secs / 60)
                };
                p.text(&content, &min_txt, pad + 46.0 * d, 74.0 * d, 9.5, 500, fg, 0.55, Anchor::Left, w * 0.52);

                let bx = w - pad - 15.0 * d;
                let by = h / 2.0;
                p.card(&content, bx - 15.0 * d, by - 15.0 * d, 30.0 * d, 30.0 * d, 15.0 * d, ring_col, 130, 88);
                p.sym(&content, if self.timer.phase == Phase::Running { Sym::Pause } else { Sym::Play }, bx, by, 12.5, WHITE, 0.98);
            }
        }
    }

    fn idx_of(&self, hwnd: HWND) -> Option<usize> {
        self.list.iter().position(|w| w.popup.hwnd == hwnd)
    }

    fn show_context_menu(&mut self, i: usize) {
        let Some(wd) = self.list.get(i) else { return };
        unsafe {
            let Ok(menu) = CreatePopupMenu() else { return };
            let to_w = |s: &str| -> Vec<u16> { s.encode_utf16().chain(std::iter::once(0)).collect() };
            let w_rem = to_w(prefs::tr("Remove widget", "Xoá tiện ích"));
            let w_set = to_w(prefs::tr("Lumina Settings…", "Cài đặt Lumina…"));
            let _ = AppendMenuW(menu, MF_STRING, CMD_REMOVE, PCWSTR(w_rem.as_ptr()));
            let _ = AppendMenuW(menu, MF_STRING, CMD_SETTINGS, PCWSTR(w_set.as_ptr()));
            let mut pt = POINT::default();
            let _ = GetCursorPos(&mut pt);
            let _ = SetForegroundWindow(self.ctl);
            let cmd = TrackPopupMenu(
                menu,
                TPM_RETURNCMD | TPM_RIGHTBUTTON,
                pt.x,
                pt.y,
                Some(0),
                wd.popup.hwnd,
                None,
            );
            let _ = DestroyMenu(menu);
            match cmd.0 as usize {
                CMD_REMOVE => {
                    self.remove_at(i);
                    self.save();
                }
                CMD_SETTINGS => (self.open_settings_cb)(),
                _ => {}
            }
        }
    }
}

unsafe extern "system" fn ctl_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        prefs::WM_PREFS => {
            with(|m| m.refresh_theme());
            LRESULT(0)
        }
        WM_TIMER if wp.0 == T_TICK => {
            with(|m| m.tick());
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

unsafe extern "system" fn widget_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_LBUTTONDOWN => {
            let lx = (lp.0 & 0xFFFF) as i16 as f32;
            let mut pt = POINT::default();
            let _ = GetCursorPos(&mut pt);
            with(|m| {
                if let Some(i) = m.idx_of(hwnd) {
                    if m.list[i].kind == WidgetKind::FocusTimer && lx >= m.list[i].w - 50.0 * m.dpi {
                        let now_ms = m.t0.elapsed().as_millis() as i64;
                        m.timer.toggle(now_ms);
                        m.paint(i);
                        return;
                    }
                    m.drag = Some(Drag {
                        idx: i,
                        grab_x: pt.x - m.list[i].x,
                        grab_y: pt.y - m.list[i].y,
                    });
                    SetCapture(hwnd);
                }
            });
            LRESULT(0)
        }
        WM_MOUSEMOVE => {
            let mut pt = POINT::default();
            let _ = GetCursorPos(&mut pt);
            with(|m| {
                if let Some(d) = &m.drag {
                    if d.idx < m.list.len() && m.list[d.idx].popup.hwnd == hwnd {
                        m.move_to(d.idx, pt.x - d.grab_x, pt.y - d.grab_y);
                    }
                }
            });
            LRESULT(0)
        }
        WM_LBUTTONUP => {
            with(|m| {
                if m.drag.take().is_some() {
                    let _ = ReleaseCapture();
                    m.save();
                }
            });
            LRESULT(0)
        }
        WM_RBUTTONUP => {
            with(|m| {
                if let Some(i) = m.idx_of(hwnd) {
                    m.show_context_menu(i);
                }
            });
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}
