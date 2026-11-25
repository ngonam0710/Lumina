use crate::prefs;
use np_core::settings::{Lang, SurfaceTheme};
use np_core::symbols::Sym;
use np_platform::monitors;
use np_ui::gfx::Gfx;
use np_ui::glass::{GlassPanel, GlassParams};
use np_ui::host::{attach, Surface};
use np_ui::painter::{rgba, Anchor, Painter, WHITE};
use std::cell::RefCell;
use windows::core::w;
use windows::UI::Composition::{Compositor, ContainerVisual};
use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::*;

const WIN_W: f32 = 470.0;
const WIN_H: f32 = 408.0;

#[derive(Clone, Copy)]
enum Act {
    Close,
    Next,
    Back,
    Lang(Lang),
    ToggleNotif,
    ToggleGameStrip,
    ToggleFps,
    ToggleVolHud,
    ToggleClip,
}

struct Region {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    act: Act,
}

impl Region {
    fn hit(&self, x: f32, y: f32) -> bool {
        x >= self.x && x < self.x + self.w && y >= self.y && y < self.y + self.h
    }
}

struct TourWin {
    hwnd: HWND,
    _surface: Surface,
    panel: GlassPanel,
    content: ContainerVisual,
    painter: Painter,
    dpi: f32,
    size: (f32, f32),
    step: u8,
    regions: Vec<Region>,
}

thread_local! {
    static TOUR: RefCell<Option<TourWin>> = const { RefCell::new(None) };
}

fn with<R>(f: impl FnOnce(&mut TourWin) -> R) -> Option<R> {
    TOUR.with(|c| c.try_borrow_mut().ok().and_then(|mut b| b.as_mut().map(f)))
}

pub fn open(comp: &Compositor) {
    let exists = with(|w| {
        w.step = 0;
        unsafe {
            let _ = ShowWindow(w.hwnd, SW_SHOW);
            let _ = SetForegroundWindow(w.hwnd);
        }
        w.render();
    });
    if exists.is_some() {
        return;
    }
    unsafe {
        let Ok(hi) = GetModuleHandleW(None) else { return };
        let hi: HINSTANCE = hi.into();
        let class = w!("LuminaTour");
        RegisterClassW(&WNDCLASSW {
            hIcon: crate::app_icon(),
            lpfnWndProc: Some(proc),
            hInstance: hi,
            lpszClassName: class,
            hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
            ..Default::default()
        });
        let mon = monitors::under_cursor();
        let dpi = mon.scale();
        let (pw, ph) = ((WIN_W * dpi).round() as i32, (WIN_H * dpi).round() as i32);
        let x = mon.work.x + (mon.work.w - pw) / 2;
        let y = mon.work.y + (mon.work.h - ph) / 2;
        let Ok(hwnd) = CreateWindowExW(
            WS_EX_NOREDIRECTIONBITMAP | WS_EX_APPWINDOW,
            class,
            w!("Welcome to Lumina"),
            WS_POPUP,
            x,
            y,
            pw,
            ph,
            None,
            None,
            Some(hi),
            None,
        ) else {
            return;
        };
        let Ok(surface) = attach(comp, hwnd, (pw as f32, ph as f32)) else { return };
        let Ok(panel) = GlassPanel::new(
            comp,
            &surface.root,
            GlassParams::from_theme(SurfaceTheme::Acrylic, (pw as f32, ph as f32), 20.0 * dpi),
        ) else {
            return;
        };
        let Ok(content) = comp.CreateContainerVisual() else { return };
        let _ = surface.root.Children().and_then(|k| k.InsertAtTop(&content));
        let Ok(gfx) = Gfx::shared(comp) else { return };
        let painter = Painter::new(comp.clone(), gfx, dpi);
        TOUR.with(|c| {
            *c.borrow_mut() = Some(TourWin {
                hwnd,
                _surface: surface,
                panel,
                content,
                painter,
                dpi,
                size: (pw as f32, ph as f32),
                step: 0,
                regions: Vec::new(),
            });
        });
        prefs::listen(hwnd);
        with(|w| w.render());
        let _ = ShowWindow(hwnd, SW_SHOW);
        let _ = SetForegroundWindow(hwnd);
    }
}

pub fn shutdown() {
    if let Some(w) = TOUR.with(|c| c.borrow_mut().take()) {
        let hwnd = w.hwnd;
        prefs::unlisten(hwnd);
        drop(w);
        unsafe {
            let _ = DestroyWindow(hwnd);
        }
    }
}

impl TourWin {
    fn render(&mut self) {
        let s = prefs::get();
        let vi = prefs::is_vi();
        let tr = |en, v| if vi { v } else { en };
        let d = self.dpi;
        self.painter.reset(d);
        self.regions.clear();
        if let Ok(k) = self.content.Children() {
            let _ = k.RemoveAll();
        }
        let (w, h) = self.size;
        let _ = self.panel.set_params(
            GlassParams::from_theme(SurfaceTheme::Acrylic, (w, h), 20.0 * d),
            prefs::auto_tier(),
        );
        let _ = self.panel.set_rect(0.0, 0.0, w, h);

        let pad = 26.0 * d;
        let cw = w - 2.0 * pad;
        let cx = w - 24.0 * d;
        let cy = 22.0 * d;
        self.painter.card(&self.content, cx - 12.0 * d, cy - 12.0 * d, 24.0 * d, 24.0 * d, 12.0 * d, WHITE, 24, 42);
        self.painter.sym(&self.content, Sym::Close, cx, cy, 9.5, WHITE, 0.90);
        self.regions.push(Region {
            x: cx - 14.0 * d,
            y: cy - 14.0 * d,
            w: 28.0 * d,
            h: 28.0 * d,
            act: Act::Close,
        });

        // Header badge
        self.painter.card(&self.content, pad, 24.0 * d, 40.0 * d, 40.0 * d, 12.0 * d, [72, 148, 255], 165, 95);
        self.painter.sym(&self.content, Sym::Sparkles, pad + 20.0 * d, 44.0 * d, 20.0, WHITE, 0.98);
        self.painter.text(&self.content, "Lumina", pad + 52.0 * d, 25.0 * d, 12.0, 600, [138, 196, 255], 0.95, Anchor::Left, cw - 80.0 * d);
        self.painter.text(
            &self.content,
            if self.step == 0 {
                tr("Welcome to Lumina", "Chào mừng đến với Lumina")
            } else {
                tr("Optional extras", "Tính năng tuỳ chọn")
            },
            pad + 52.0 * d,
            42.0 * d,
            18.5,
            700,
            WHITE,
            1.0,
            Anchor::Left,
            cw - 80.0 * d,
        );

        if self.step == 0 {
            self.painter.text(
                &self.content,
                tr(
                    "Starts at the top center of your screen. Rest your pointer on it to peek and open,",
                    "Nằm ở chính giữa mép trên màn hình. Rê chuột vào để hé xem và mở rộng,",
                ),
                pad,
                84.0 * d,
                11.0,
                500,
                WHITE,
                0.78,
                Anchor::Left,
                cw,
            );
            self.painter.text(
                &self.content,
                tr("move away and it closes smoothly.", "di chuyển ra ngoài để thu gọn mượt mà."),
                pad,
                102.0 * d,
                11.0,
                500,
                WHITE,
                0.78,
                Anchor::Left,
                cw,
            );

            // Language picker card
            let ly = 138.0 * d;
            self.painter.card(&self.content, pad, ly, cw, 68.0 * d, 14.0 * d, WHITE, 16, 28);
            self.painter.text(&self.content, tr("Language", "Ngôn ngữ"), pad + 14.0 * d, ly + 10.0 * d, 11.5, 700, WHITE, 0.95, Anchor::Left, cw);
            let seg_y = ly + 33.0 * d;
            let seg_w = cw - 28.0 * d;
            let item_w = seg_w / Lang::ALL.len() as f32;
            for (i, l) in Lang::ALL.into_iter().enumerate() {
                let ix = pad + 14.0 * d + i as f32 * item_w;
                let on = s.lang == l;
                if on {
                    self.painter.card(&self.content, ix + 2.0 * d, seg_y, item_w - 4.0 * d, 24.0 * d, 7.0 * d, [72, 148, 255], 175, 95);
                } else {
                    self.painter.card(&self.content, ix + 2.0 * d, seg_y, item_w - 4.0 * d, 24.0 * d, 7.0 * d, WHITE, 12, 20);
                }
                self.painter.pill_text(&self.content, l.label_i18n(vi), ix + item_w / 2.0, seg_y + 12.0 * d, 9.8, if on { 700 } else { 500 }, WHITE, if on { 1.0 } else { 0.78 }, item_w - 6.0 * d);
                self.regions.push(Region {
                    x: ix,
                    y: seg_y,
                    w: item_w,
                    h: 26.0 * d,
                    act: Act::Lang(l),
                });
            }

            // Try it now hint card
            let hy = 224.0 * d;
            self.painter.card(&self.content, pad, hy, cw, 98.0 * d, 14.0 * d, [72, 148, 255], 30, 44);
            self.painter.text(&self.content, tr("Try it now", "Dùng thử ngay"), pad + 14.0 * d, hy + 14.0 * d, 12.0, 700, [142, 204, 255], 0.98, Anchor::Left, cw - 28.0 * d);
            self.painter.text(&self.content, tr("Move your pointer to the top of the screen and rest on the Island.", "Đưa con trỏ chuột lên mép trên màn hình và giữ nhẹ trên Island."), pad + 14.0 * d, hy + 38.0 * d, 11.0, 500, WHITE, 0.86, Anchor::Left, cw - 28.0 * d);
            self.painter.text(&self.content, tr("Right-click the Lumina icon near the clock anytime for Settings.", "Chuột phải biểu tượng Lumina gần đồng hồ để mở Cài đặt bất cứ lúc nào."), pad + 14.0 * d, hy + 58.0 * d, 10.5, 500, WHITE, 0.65, Anchor::Left, cw - 28.0 * d);

            let bx = w - pad - 104.0 * d;
            let by = h - 54.0 * d;
            self.painter.card(&self.content, bx, by, 104.0 * d, 32.0 * d, 16.0 * d, [72, 148, 255], 210, 110);
            self.painter.pill_text(&self.content, tr("Next →", "Tiếp →"), bx + 52.0 * d, by + 16.0 * d, 11.5, 700, WHITE, 0.98, 96.0 * d);
            self.regions.push(Region {
                x: bx,
                y: by,
                w: 104.0 * d,
                h: 32.0 * d,
                act: Act::Next,
            });
        } else {
            self.painter.text(
                &self.content,
                tr(
                    "Turn on what you want. You can change any of these later in Settings.",
                    "Bật những tính năng bạn thích. Bạn luôn có thể đổi lại sau trong Cài đặt.",
                ),
                pad,
                76.0 * d,
                10.5,
                500,
                WHITE,
                0.72,
                Anchor::Left,
                cw,
            );
            let mut y = 104.0 * d;
            for (lbl, on, act) in [
                (tr("Notifications in the island", "Hiển thị thông báo trên Island"), s.notifications_in_island, Act::ToggleNotif),
                (tr("Game strip (fullscreen CPU / GPU / RAM)", "Thanh thông số Game (CPU / GPU / RAM)"), s.game_strip, Act::ToggleGameStrip),
                (tr("FPS counter in Game strip", "Bộ đếm FPS trong thanh Game"), s.fps_counter, Act::ToggleFps),
                (tr("Volume & brightness HUD pill in Island", "Thanh trượt âm lượng & độ sáng trên Island"), s.volume_in_island, Act::ToggleVolHud),
                (tr("Clipboard history", "Lịch sử Clipboard"), s.clipboard_history, Act::ToggleClip),
            ] {
                let rh = 40.0 * d;
                self.painter.card(&self.content, pad, y, cw, rh, 11.0 * d, WHITE, 15, 26);
                self.painter.text(&self.content, lbl, pad + 14.0 * d, y + 12.0 * d, 11.0, 600, WHITE, 0.94, Anchor::Left, cw - 72.0 * d);
                let sw = 40.0 * d;
                let sh = 20.0 * d;
                let sx = pad + cw - sw - 12.0 * d;
                let sy = y + (rh - sh) / 2.0;
                self.painter.card(&self.content, sx, sy, sw, sh, sh / 2.0, if on { [64, 156, 255] } else { WHITE }, if on { 220 } else { 28 }, 60);
                let kd = 14.0 * d;
                let kx = if on { sx + sw - kd - 3.0 * d } else { sx + 3.0 * d };
                self.painter.rect(&self.content, kx, sy + 3.0 * d, kd, kd, kd / 2.0, rgba(WHITE, 250));
                self.regions.push(Region { x: pad, y, w: cw, h: rh, act });
                y += rh + 8.0 * d;
            }

            let by = h - 54.0 * d;
            self.painter.card(&self.content, pad, by, 102.0 * d, 32.0 * d, 16.0 * d, WHITE, 24, 38);
            self.painter.pill_text(&self.content, tr("← Back", "← Quay lại"), pad + 51.0 * d, by + 16.0 * d, 11.0, 600, WHITE, 0.92, 96.0 * d);
            self.regions.push(Region { x: pad, y: by, w: 102.0 * d, h: 32.0 * d, act: Act::Back });

            let dx = w - pad - 114.0 * d;
            self.painter.card(&self.content, dx, by, 114.0 * d, 32.0 * d, 16.0 * d, [64, 168, 255], 220, 110);
            self.painter.pill_text(&self.content, tr("Done ✓", "Hoàn tất ✓"), dx + 57.0 * d, by + 16.0 * d, 11.5, 700, WHITE, 0.98, 106.0 * d);
            self.regions.push(Region { x: dx, y: by, w: 114.0 * d, h: 32.0 * d, act: Act::Close });
        }
    }

    fn apply_act(&mut self, act: Act) {
        match act {
            Act::Close => {
                let s = prefs::get();
                let _ = std::fs::create_dir_all(crate::paths::config_dir());
                let _ = std::fs::write(crate::paths::settings_file(), s.to_json());
                unsafe {
                    let _ = ShowWindow(self.hwnd, SW_HIDE);
                }
            }
            Act::Next => {
                self.step = 1;
                self.render();
            }
            Act::Back => {
                self.step = 0;
                self.render();
            }
            Act::Lang(l) => prefs::update(|s| s.lang = l),
            Act::ToggleNotif => prefs::update(|s| s.notifications_in_island = !s.notifications_in_island),
            Act::ToggleGameStrip => prefs::update(|s| s.game_strip = !s.game_strip),
            Act::ToggleFps => prefs::update(|s| s.fps_counter = !s.fps_counter),
            Act::ToggleVolHud => prefs::update(|s| s.volume_in_island = !s.volume_in_island),
            Act::ToggleClip => prefs::update(|s| {
                s.clipboard_history = !s.clipboard_history;
                np_platform::clipboard::set_enabled(s.clipboard_history);
            }),
        }
    }
}

unsafe extern "system" fn proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        prefs::WM_PREFS => {
            with(|w| w.render());
            LRESULT(0)
        }
        WM_NCHITTEST => {
            let mut pt = windows::Win32::Foundation::POINT {
                x: (lp.0 & 0xFFFF) as i16 as i32,
                y: ((lp.0 >> 16) & 0xFFFF) as i16 as i32,
            };
            let _ = windows::Win32::Graphics::Gdi::ScreenToClient(hwnd, &mut pt);
            let drag = with(|w| {
                pt.y < (64.0 * w.dpi) as i32 && !w.regions.iter().any(|r| r.hit(pt.x as f32, pt.y as f32))
            })
            .unwrap_or(false);
            if drag {
                LRESULT(HTCAPTION as isize)
            } else {
                DefWindowProcW(hwnd, msg, wp, lp)
            }
        }
        WM_LBUTTONDOWN => {
            let x = (lp.0 & 0xFFFF) as i16 as f32;
            let y = ((lp.0 >> 16) & 0xFFFF) as i16 as f32;
            with(|w| {
                if let Some(r) = w.regions.iter().find(|r| r.hit(x, y)) {
                    let act = r.act;
                    w.apply_act(act);
                }
            });
            LRESULT(0)
        }
        WM_CLOSE => {
            let _ = ShowWindow(hwnd, SW_HIDE);
            LRESULT(0)
        }
        WM_DESTROY => {
            prefs::unlisten(hwnd);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

