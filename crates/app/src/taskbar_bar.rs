use crate::{paths, prefs};
use np_core::dock_layout::{compute_layout, hit_cell, merge_items, norm_key, DockItem, Layout, SlotKind};
use np_core::settings::{surface_spec, IslandEdge};
use np_core::symbols::Sym;
use np_platform::{audio, icons, monitors, sys, taskbar, time, wifi_bt};
use np_ui::gfx::Gfx;
use np_ui::glass::GlassPanel;
use np_ui::host::{attach, Surface};
use np_ui::painter::{rgba, Anchor, Painter, WHITE};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use windows::core::{w, PCWSTR};
use windows::UI::Composition::{
    CompositionDrawingSurface, Compositor, ContainerVisual,
};
use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows::Win32::Graphics::Gdi::{CreateRectRgn, SetWindowRgn};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::{TrackMouseEvent, TME_LEAVE, TRACKMOUSEEVENT};
use windows::Win32::UI::WindowsAndMessaging::*;
use windows_numerics::{Vector2, Vector3};

const WM_MOUSELEAVE: u32 = 0x02A3;
const WM_ICONS: u32 = WM_APP + 61;
const T_POLL: usize = 1;
const T_EDGE: usize = 2;
const CMD_PIN: usize = 1;
const CMD_CLOSE: usize = 2;
const CMD_SETTINGS: usize = 3;

#[derive(Clone, Copy)]
enum SlotEntry {
    Start,
    App(usize),
    Sep,
}

struct TaskbarBar {
    hwnd: HWND,
    gfx: Gfx,
    surface: Surface,
    panel: GlassPanel,
    content: ContainerVisual,
    painter: Painter,
    dpi: f32,
    win_pos: (i32, i32),
    win_size: (i32, i32),
    card_rect: (f32, f32, f32, f32),
    items: Vec<DockItem>,
    entries: Vec<SlotEntry>,
    slots: Vec<SlotKind>,
    layout: Layout,
    icon_surfaces: HashMap<String, CompositionDrawingSurface>,
    requested: HashSet<String>,
    worker: icons::Worker,
    cursor_x: Option<f32>,
    tracking: bool,
    allow_takeover: bool,
    visible: bool,
    last_sig: String,
    now_playing_text: String,
    np_rect: Option<(f32, f32, f32, f32)>,
    open_settings_cb: fn(),
}

thread_local! {
    static BAR: RefCell<Option<TaskbarBar>> = const { RefCell::new(None) };
}

fn with<R>(f: impl FnOnce(&mut TaskbarBar) -> R) -> Option<R> {
    BAR.with(|c| c.try_borrow_mut().ok().and_then(|mut b| b.as_mut().map(f)))
}

pub fn set_now_playing(text: String) {
    with(|b| {
        if b.now_playing_text != text {
            b.now_playing_text = text;
            b.render();
        }
    });
}

pub fn shutdown() {
    if let Some(b) = BAR.with(|c| c.borrow_mut().take()) {
        let hwnd = b.hwnd;
        prefs::unlisten(hwnd);
        drop(b);
        unsafe {
            let _ = DestroyWindow(hwnd);
        }
    }
}

pub fn init(comp: &Compositor, allow_takeover: bool, open_settings_cb: fn()) -> windows_core::Result<()> {
    unsafe {
        let hi: HINSTANCE = GetModuleHandleW(None)?.into();
        let class = w!("LuminaTaskbar");
        RegisterClassW(&WNDCLASSW {
            lpfnWndProc: Some(proc),
            hInstance: hi,
            lpszClassName: class,
            hCursor: LoadCursorW(None, IDC_ARROW)?,
            ..Default::default()
        });
        let hwnd = CreateWindowExW(
            WS_EX_NOREDIRECTIONBITMAP | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
            class,
            w!("np-taskbar"),
            WS_POPUP,
            0,
            0,
            600,
            72,
            None,
            None,
            Some(hi),
            None,
        )?;
        let mon = monitors::primary();
        let dpi = mon.scale();
        let surface = attach(comp, hwnd, (600.0, 72.0))?;
        let panel = GlassPanel::new(comp, &surface.root, prefs::glass_params((400.0, 52.0), 22.0))?;
        let content = comp.CreateContainerVisual()?;
        surface.root.Children()?.InsertAtTop(&content)?;
        let gfx = Gfx::shared(comp)?;
        let hw = hwnd.0 as isize;
        let worker = icons::spawn_worker((48.0 * dpi).round() as i32, move || {
            let _ = PostMessageW(Some(HWND(hw as *mut _)), WM_ICONS, WPARAM(0), LPARAM(0));
        });
        let painter = Painter::new(comp.clone(), gfx.clone(), dpi);
        BAR.with(|c| {
            *c.borrow_mut() = Some(TaskbarBar {
                hwnd,
                gfx,
                surface,
                panel,
                content,
                painter,
                dpi,
                win_pos: (0, 0),
                win_size: (600, 72),
                card_rect: (0.0, 0.0, 400.0, 52.0),
                items: Vec::new(),
                entries: Vec::new(),
                slots: Vec::new(),
                layout: Layout {
                    cells: Vec::new(),
                    total: 0.0,
                },
                icon_surfaces: HashMap::new(),
                requested: HashSet::new(),
                worker,
                cursor_x: None,
                tracking: false,
                allow_takeover,
                visible: false,
                last_sig: String::new(),
                now_playing_text: String::new(),
                np_rect: None,
                open_settings_cb,
            });
        });
        prefs::listen(hwnd);
        SetTimer(Some(hwnd), T_POLL, 900, None);
    }
    with(|b| b.sync_state(true));
    Ok(())
}

impl TaskbarBar {
    fn sync_state(&mut self, settings_changed: bool) {
        let s = prefs::get();
        if settings_changed {
            unsafe {
                if s.taskbar_enabled && s.taskbar_auto_reveal {
                    SetTimer(Some(self.hwnd), T_EDGE, 100, None);
                } else {
                    let _ = KillTimer(Some(self.hwnd), T_EDGE);
                }
            }
        }
        if !s.taskbar_enabled {
            self.visible = false;
            if taskbar::is_hidden() {
                taskbar::restore();
            }
            unsafe {
                let _ = ShowWindow(self.hwnd, SW_HIDE);
            }
            return;
        }
        if self.allow_takeover && settings_changed && !taskbar::is_hidden() {
            let _ = taskbar::hide(&paths::marker_file(), &paths::guard_exe());
        }
        let mon = monitors::primary();
        self.dpi = mon.scale();
        if (s.hide_under_fullscreen && taskbar::foreground_is_fullscreen(std::process::id(), mon.area))
            || (s.taskbar_auto_reveal && !self.at_edge())
            || (!s.taskbar_auto_reveal && s.taskbar_autohide_side && s.edge != IslandEdge::Top)
        {
            self.visible = false;
            unsafe {
                let _ = ShowWindow(self.hwnd, SW_HIDE);
            }
            return;
        }
        let appearing = !self.visible;
        self.visible = true;
        self.refresh_windows();
        self.render();
        if appearing {
            let scale = s.motion.period_scale();
            if scale > 0.0 {
                let root = &self.surface.root;
                let comp = &self.painter.comp;
                let _ = np_ui::motion::scalar(comp, root, "Offset.Y", 18.0 * self.dpi, 0.0, (220.0 * scale) as u32);
                let _ = np_ui::motion::scalar(comp, root, "Opacity", 0.0, 1.0, (170.0 * scale) as u32);
            } else {
                let _ = self.surface.root.SetOffset(Vector3::default());
                let _ = self.surface.root.SetOpacity(1.0);
            }
            unsafe { let _ = ShowWindow(self.hwnd, SW_SHOWNOACTIVATE); }
        }
    }

    fn at_edge(&self) -> bool {
        let area = monitors::primary().area;
        let mut pt = POINT::default();
        if unsafe { GetCursorPos(&mut pt) }.is_err() {
            return false;
        }
        let dock = self.visible.then(|| monitors::Area::new(
            self.win_pos.0, self.win_pos.1, self.win_size.0, area.bottom() - self.win_pos.1,
        ));
        reveal_at(area, dock, self.dpi, pt)
    }

    fn refresh_windows(&mut self) {
        let wins = taskbar::enumerate_windows(std::process::id());
        let pins = prefs::get().pinned_apps;
        let items = merge_items(&pins, &wins);
        let sig = format!(
            "{:?}|{}",
            items.iter().map(|i| (&i.key, i.pinned, i.hwnds.len())).collect::<Vec<_>>(),
            prefs::get().surface.label()
        );
        self.items = items;
        if sig != self.last_sig {
            self.last_sig = sig;
            self.entries.clear();
            self.slots.clear();
            self.entries.push(SlotEntry::Start);
            self.slots.push(SlotKind::Item);
            self.entries.push(SlotEntry::Sep);
            self.slots.push(SlotKind::Separator);
            for i in 0..self.items.len() {
                self.entries.push(SlotEntry::App(i));
                self.slots.push(SlotKind::Item);
                if self.items[i].pinned {
                    let path = &self.items[i].path;
                    if !self.icon_surfaces.contains_key(path) && self.requested.insert(path.clone()) {
                        let _ = self.worker.requests.send(path.clone());
                    }
                } else {
                    let path = &self.items[i].path;
                    if !self.icon_surfaces.contains_key(path) && self.requested.insert(path.clone()) {
                        let _ = self.worker.requests.send(path.clone());
                    }
                }
            }
        }
    }

    fn render(&mut self) {
        let s = prefs::get();
        if !self.visible || !s.taskbar_enabled {
            return;
        }
        let mon = monitors::primary();
        let d = self.dpi;
        self.painter.reset(d);

        let now = time::local_now();
        let time_str = if s.clock_24h {
            format!("{:02}:{:02}", now.hour, now.minute)
        } else {
            let h12 = if now.hour % 12 == 0 { 12 } else { now.hour % 12 };
            let ap = if now.hour < 12 { "AM" } else { "PM" };
            format!("{h12}:{:02} {ap}", now.minute)
        };
        let date_str = format!("{:02}/{:02}/{}", now.month, now.day, now.year);
        let clock_w = self
            .painter
            .measure(&time_str, 10.5, 600)
            .max(self.painter.measure(&date_str, 9.2, 500))
            .max(66.0 * d);

        let bat = sys::battery();
        let bat_txt = bat.map(|(pct, _)| format!("{pct}%"));
        let bat_txt_w = bat_txt
            .as_deref()
            .map(|t| self.painter.measure(t, 9.5, 600))
            .unwrap_or(0.0);
        // Spacious left-to-right pill geometry so battery icon, %, separator, Wi-Fi and speaker never crowd
        let bat_block_w = if bat.is_some() {
            18.0 * d + 6.0 * d + bat_txt_w + 10.0 * d + 1.0 * d + 10.0 * d
        } else {
            0.0
        };
        let cap_w = 13.0 * d + bat_block_w + 14.0 * d + 13.0 * d + 14.0 * d + 13.0 * d;
        let tray_w = cap_w + 12.0 * d + clock_w;

        let mut icon_sz = 30.0 * d;
        let mut gap = 7.0 * d;
        let max_zoom = (s.taskbar_dock_zoom_pct.clamp(100, 180) as f32 / 100.0).max(1.0);
        let raw_dock = compute_layout(&self.slots, self.cursor_x, icon_sz, gap, max_zoom, 0.0);

        let mut np_w = if s.taskbar_show_now_playing && !self.now_playing_text.is_empty() {
            (self.painter.measure(&self.now_playing_text, 10.0, 600) + 34.0 * d).clamp(96.0 * d, 180.0 * d)
        } else {
            0.0
        };
        let pad_x = 14.0 * d;
        let gap_dock_right = 14.0 * d;
        let max_bar_w = (mon.area.w as f32 - 24.0 * d).max(340.0 * d);
        let avail_left = (max_bar_w - pad_x * 2.0 - gap_dock_right - tray_w).max(120.0 * d);
        if raw_dock.total + (if np_w > 0.0 { 12.0 * d + np_w } else { 0.0 }) > avail_left {
            np_w = 0.0;
        }
        let dock_w = if raw_dock.total > avail_left {
            let scale = (avail_left / raw_dock.total).clamp(0.55, 1.0);
            icon_sz *= scale;
            gap *= scale;
            compute_layout(&self.slots, self.cursor_x, icon_sz, gap, max_zoom, 0.0).total
        } else {
            raw_dock.total
        };

        let np_gap = if np_w > 0.0 { 12.0 * d } else { 0.0 };
        let card_h = 46.0 * d;
        let card_w = (pad_x * 2.0 + dock_w + np_gap + np_w + gap_dock_right + tray_w).min(max_bar_w);
        let win_h = (card_h + 24.0 * d).ceil() as i32;
        let win_w = (card_w + 32.0 * d).ceil() as i32;
        let win_x = mon.area.x + (mon.area.w - win_w) / 2;
        let win_y = mon.area.bottom() - win_h - (6.0 * d).round() as i32;
        self.win_pos = (win_x, win_y);
        self.win_size = (win_w, win_h);

        let cx = (win_w as f32 - card_w) / 2.0;
        let cy = win_h as f32 - card_h;
        self.card_rect = (cx, cy, card_w, card_h);

        unsafe {
            let _ = SetWindowPos(
                self.hwnd,
                Some(HWND_TOPMOST),
                win_x,
                win_y,
                win_w,
                win_h,
                SWP_NOACTIVATE,
            );
            let rgn = CreateRectRgn(
                cx.floor() as i32,
                0,
                (cx + card_w).ceil() as i32,
                win_h,
            );
            SetWindowRgn(self.hwnd, Some(rgn), true);
        }
        let _ = self.surface.root.SetSize(Vector2 {
            X: win_w as f32,
            Y: win_h as f32,
        });

        let radius = (23.0 * d * s.corner_scale()).max(4.0);
        let gp = prefs::glass_params((card_w, card_h), radius);
        let _ = self.panel.set_params(gp, prefs::auto_tier());
        let _ = self.panel.set_rect(cx, cy, card_w, card_h);
        let _ = self.panel.set_radius(radius);

        if let Ok(kids) = self.content.Children() {
            let _ = kids.RemoveAll();
        }

        let dock_area_left = cx + pad_x;
        let dock_layout = compute_layout(
            &self.slots,
            self.cursor_x,
            icon_sz,
            gap,
            max_zoom,
            dock_w,
        );
        self.layout = Layout {
            cells: dock_layout
                .cells
                .into_iter()
                .map(|c| np_core::dock_layout::Cell {
                    x: dock_area_left + c.x,
                    ..c
                })
                .collect(),
            total: dock_w,
        };

        let fg = surface_spec(s.surface).fg;
        let fg_hwnd = taskbar::foreground_hwnd();
        let base_y = cy + (card_h - icon_sz) / 2.0 - 1.5 * d;
        self.np_rect = None;
        for (i, entry) in self.entries.iter().enumerate() {
            let cell = self.layout.cells[i];
            let sz = icon_sz * cell.scale;
            let iy = base_y - (sz - icon_sz);
            match *entry {
                SlotEntry::Start => {
                    let r = (sz * 0.28).max(4.0);
                    self.painter.card(&self.content, cell.x, iy, sz, sz, r, [64, 144, 255], 165, 110);
                    let q = sz * 0.21;
                    let qx = cell.x + sz * 0.245;
                    let qy = iy + sz * 0.245;
                    let qg = sz * 0.295;
                    for (dx, dy) in [(0.0, 0.0), (qg, 0.0), (0.0, qg), (qg, qg)] {
                        self.painter.rect(&self.content, qx + dx, qy + dy, q, q, 2.2 * d, rgba(WHITE, 245));
                    }
                }
                SlotEntry::Sep => {
                    let lx = cell.x + cell.w / 2.0 - 0.5 * d;
                    self.painter.rect(
                        &self.content,
                        lx,
                        cy + 11.0 * d,
                        1.0 * d,
                        card_h - 22.0 * d,
                        0.5 * d,
                        rgba(fg, 48),
                    );
                }
                SlotEntry::App(app_i) => {
                    let it = &self.items[app_i];
                    if cell.scale > 1.05 {
                        self.painter.card(&self.content, cell.x - 2.0 * d, iy - 2.0 * d, sz + 4.0 * d, sz + 4.0 * d, (sz * 0.26).max(4.0), fg, 22, 42);
                    }
                    if let Some(surf) = self.icon_surfaces.get(&it.path) {
                        if let Some(sv) = self.painter.image(&self.content, surf, cell.x, iy, sz, sz, sz * 0.22) {
                            let _ = sv.SetOffset(Vector3 {
                                X: cell.x,
                                Y: iy,
                                Z: 0.0,
                            });
                        }
                    } else {
                        self.painter.card(
                            &self.content,
                            cell.x,
                            iy,
                            sz,
                            sz,
                            sz * 0.22,
                            fg,
                            36,
                            40,
                        );
                    }
                    if it.running() {
                        let active = it.hwnds.contains(&fg_hwnd);
                        let pw = if active { 11.5 * d } else { 4.5 * d };
                        let ph = 3.0 * d;
                        let col = if active { s.accent } else { fg };
                        self.painter.rect(
                            &self.content,
                            cell.x + sz / 2.0 - pw / 2.0,
                            cy + card_h - 6.0 * d,
                            pw,
                            ph,
                            ph / 2.0,
                            rgba(col, if active { 250 } else { 205 }),
                        );
                    }
                }
            }
        }

        let right_edge = cx + card_w - pad_x;
        self.painter.text(&self.content, &time_str, right_edge, cy + 7.5 * d, 10.5, 600, fg, 0.96, Anchor::Right, clock_w + 4.0 * d);
        self.painter.text(&self.content, &date_str, right_edge, cy + 22.5 * d, 9.2, 500, fg, 0.65, Anchor::Right, clock_w + 4.0 * d);

        let cap_right = right_edge - clock_w - 12.0 * d;
        let cap_x = cap_right - cap_w;
        let cap_h = card_h - 14.0 * d;
        let cap_y = cy + (card_h - cap_h) / 2.0;
        let mid_y = cy + card_h / 2.0;
        self.painter.card(&self.content, cap_x, cap_y, cap_w, cap_h, cap_h / 2.0, fg, 18, 32);

        let mut sx = cap_x + 13.0 * d;
        if let (Some((pct, ch)), Some(txt)) = (bat, bat_txt.as_deref()) {
            let bcol = if ch { [72, 224, 134] } else if pct <= 20 { [255, 94, 94] } else { fg };
            let bat_cx = sx + 9.0 * d;
            self.painter.sym(&self.content, Sym::Battery, bat_cx, mid_y, 14.0, fg, 0.82);
            let fw = (6.4 * d * (pct as f32 / 100.0).clamp(0.15, 1.0)).max(2.0 * d);
            self.painter.rect(&self.content, bat_cx - 4.4 * d, mid_y - 2.0 * d, fw, 4.0 * d, 1.0 * d, rgba(bcol, 240));
            if ch {
                self.painter.sym(&self.content, Sym::Bolt, bat_cx - 0.5 * d, mid_y, 8.5, WHITE, 0.98);
            }
            sx += 18.0 * d + 6.0 * d;
            self.painter.pill_text(&self.content, txt, sx + bat_txt_w / 2.0, mid_y, 9.5, 600, bcol, 0.94, bat_txt_w + 4.0 * d);
            sx += bat_txt_w + 10.0 * d;
            self.painter.rect(&self.content, sx, mid_y - 6.5 * d, 1.0 * d, 13.0 * d, 0.5 * d, rgba(fg, 40));
            sx += 1.0 * d + 10.0 * d;
        }
        self.painter.sym(
            &self.content,
            Sym::Wifi,
            sx + 7.0 * d,
            mid_y,
            12.5,
            if wifi_bt::wifi_on() { fg } else { [160, 160, 165] },
            0.90,
        );
        sx += 14.0 * d + 13.0 * d;
        let aud = audio::latest();
        self.painter.sym(
            &self.content,
            if aud.muted { Sym::SpeakerMute } else { Sym::Speaker },
            sx + 7.0 * d,
            mid_y,
            12.5,
            fg,
            0.90,
        );

        if np_w > 0.0 {
            let nx = dock_area_left + dock_w + 12.0 * d;
            if nx + np_w <= cap_x - 10.0 * d {
                let py = cy + 7.0 * d;
                let ph = card_h - 14.0 * d;
                self.painter.card(&self.content, nx, py, np_w, ph, ph / 2.0, s.accent, 34, 44);
                self.painter.sym(&self.content, Sym::Music, nx + 14.0 * d, mid_y, 11.5, s.accent, 0.96);
                let txt = self.painter.fit(&self.now_playing_text, 10.0, 600, np_w - 30.0 * d);
                self.painter.text(&self.content, &txt, nx + 24.0 * d, mid_y - 6.0 * d, 10.0, 600, fg, 0.92, Anchor::Left, np_w - 30.0 * d);
                self.np_rect = Some((nx, py, np_w, ph));
            }
        }
    }

    fn on_click(&mut self, lx: f32) {
        if let Some((nx, _, nw, _)) = self.np_rect {
            if lx >= nx && lx <= nx + nw {
                crate::island::toggle_open();
                return;
            }
        }
        let Some(idx) = hit_cell(&self.layout, lx) else {
            return;
        };
        match self.entries.get(idx).copied() {
            Some(SlotEntry::Start) => taskbar::trigger_start_menu(),
            Some(SlotEntry::App(app_i)) => {
                if let Some(item) = self.items.get(app_i) {
                    if let Some(&hwnd) = item.hwnds.first() {
                        taskbar::activate_or_toggle_window(hwnd);
                    } else {
                        taskbar::launch_path(&item.path);
                    }
                }
            }
            _ => {}
        }
    }

    fn on_right_click(&mut self, lx: f32) {
        let clicked_app = hit_cell(&self.layout, lx).and_then(|idx| match self.entries.get(idx) {
            Some(SlotEntry::App(app_i)) => self.items.get(*app_i).cloned(),
            _ => None,
        });
        unsafe {
            let Ok(menu) = CreatePopupMenu() else { return };
            let to_w = |s: &str| -> Vec<u16> { s.encode_utf16().chain(std::iter::once(0)).collect() };
            if let Some(ref item) = clicked_app {
                let lbl = if item.pinned {
                    prefs::tr("Unpin from Taskbar", "Bỏ ghim khỏi Thanh tác vụ")
                } else {
                    prefs::tr("Pin to Taskbar", "Ghim vào Thanh tác vụ")
                };
                let w_pin = to_w(lbl);
                let _ = AppendMenuW(menu, MF_STRING, CMD_PIN, PCWSTR(w_pin.as_ptr()));
                if item.running() {
                    let w_close = to_w(prefs::tr("Close window", "Đóng cửa sổ"));
                    let _ = AppendMenuW(menu, MF_STRING, CMD_CLOSE, PCWSTR(w_close.as_ptr()));
                }
                let _ = AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null());
            }
            let w_ts = to_w(prefs::tr("Taskbar Settings…", "Cài đặt Thanh tác vụ…"));
            let _ = AppendMenuW(menu, MF_STRING, CMD_SETTINGS, PCWSTR(w_ts.as_ptr()));
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
                CMD_PIN => {
                    if let Some(item) = clicked_app {
                        let k = norm_key(&item.path);
                        prefs::update(|s| {
                            if let Some(pos) = s.pinned_apps.iter().position(|p| norm_key(p) == k) {
                                s.pinned_apps.remove(pos);
                            } else {
                                s.pinned_apps.push(item.path.clone());
                            }
                        });
                    }
                }
                CMD_CLOSE => {
                    if let Some(item) = clicked_app {
                        for h in item.hwnds {
                            let _ = PostMessageW(Some(HWND(h as *mut _)), WM_CLOSE, WPARAM(0), LPARAM(0));
                        }
                    }
                }
                CMD_SETTINGS => (self.open_settings_cb)(),
                _ => {}
            }
        }
    }
}

unsafe extern "system" fn proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        prefs::WM_PREFS => {
            with(|b| b.sync_state(true));
            LRESULT(0)
        }
        WM_TIMER if wp.0 == T_POLL => {
            wifi_bt::poll_status();
            with(|b| b.sync_state(false));
            LRESULT(0)
        }
        WM_TIMER if wp.0 == T_EDGE => {
            with(|b| {
                if b.at_edge() != b.visible {
                    b.sync_state(false);
                }
            });
            LRESULT(0)
        }
        WM_ICONS => {
            with(|b| {
                let mut got = false;
                while let Ok((path, bmp)) = b.worker.results.try_recv() {
                    if let Some(bmp) = bmp {
                        if let Ok(surf) = b.gfx.surface_from_bgra(bmp.w, bmp.h, &bmp.bgra) {
                            b.icon_surfaces.insert(path, surf);
                            got = true;
                        }
                    }
                }
                if got {
                    b.render();
                }
            });
            LRESULT(0)
        }
        WM_MOUSEMOVE => {
            let x = (lp.0 & 0xFFFF) as i16 as f32;
            with(|b| {
                if !b.tracking {
                    b.tracking = true;
                    let mut tme = TRACKMOUSEEVENT {
                        cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                        dwFlags: TME_LEAVE,
                        hwndTrack: hwnd,
                        dwHoverTime: 0,
                    };
                    let _ = TrackMouseEvent(&mut tme);
                }
                b.cursor_x = Some(x);
                b.render();
            });
            LRESULT(0)
        }
        WM_MOUSELEAVE => {
            with(|b| {
                b.tracking = false;
                b.cursor_x = None;
                b.render();
            });
            LRESULT(0)
        }
        WM_LBUTTONUP => {
            let x = (lp.0 & 0xFFFF) as i16 as f32;
            with(|b| b.on_click(x));
            LRESULT(0)
        }
        WM_RBUTTONUP => {
            let x = (lp.0 & 0xFFFF) as i16 as f32;
            with(|b| b.on_right_click(x));
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

fn reveal_at(area: monitors::Area, dock: Option<monitors::Area>, dpi: f32, pt: POINT) -> bool {
    area.contains(pt.x, pt.y) && (pt.y >= area.bottom() - (3.0 * dpi).ceil() as i32
        || dock.is_some_and(|bounds| bounds.contains(pt.x, pt.y)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bottom_edge_opens_and_dock_and_gap_keep_bar_visible() {
        let area = monitors::Area::new(0, 0, 1920, 1080);
        let dock = Some(monitors::Area::new(400, 1000, 1120, 80));
        let hit = |bounds, x, y| reveal_at(area, bounds, 1.5, POINT { x, y });
        assert!(hit(None, 500, 1079));
        assert!(!hit(None, 500, 1070));
        assert!(hit(dock, 500, 1070));
        assert!(hit(dock, 500, 1020));
        assert!(!hit(dock, 300, 1020));
        assert!(!hit(dock, 500, 999));
        assert!(!hit(dock, 1920, 1079));
        assert!(!hit(dock, 500, 1080));
    }
}
