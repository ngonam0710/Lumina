use crate::{desktop_widgets, island, paths, prefs};
use np_core::island::greeting_text;
use np_core::settings::{
    hex_color, hsv_to_rgb, rgb_to_hsv, surface_spec, AiProvider, IslandEdge, IslandShape, Lang,
    MotionMode, OpenTarget, OpenTrigger, Settings, SurfaceTheme, VisStyle,
};
use np_core::symbols::Sym;
use np_core::widgets::WidgetKind;
use np_platform::{capture, clipboard, monitors, startup, taskbar, time};
use np_ui::gfx::Gfx;
use np_ui::glass::{GlassPanel, GlassParams};
use np_ui::host::{attach, Surface};
use np_ui::painter::{rgba, Anchor, Painter, WHITE};
use std::cell::RefCell;
use windows::core::w;
use windows::UI::Color;
use windows::UI::Composition::{
    CompositionDrawingSurface, CompositionStretch, Compositor, ContainerVisual,
};
use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::{ReleaseCapture, SetCapture};
use windows::Win32::UI::WindowsAndMessaging::*;

const WIN_W: f32 = 812.0;
const WIN_H: f32 = 576.0;
const SIDE_W: f32 = 206.0;
const HEADER_H: f32 = 56.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Nav {
    General,
    Accessibility,
    Appearance,
    Island,
    Taskbar,
    Panels,
    Widgets,
    Calls,
    Streaming,
    Gaming,
    AiAgents,
    Tools,
    Keys,
    About,
}

impl Nav {
    const ALL: [Nav; 14] = [
        Nav::General,
        Nav::Accessibility,
        Nav::Appearance,
        Nav::Island,
        Nav::Taskbar,
        Nav::Panels,
        Nav::Widgets,
        Nav::Calls,
        Nav::Streaming,
        Nav::Gaming,
        Nav::AiAgents,
        Nav::Tools,
        Nav::Keys,
        Nav::About,
    ];

    fn title(self, vi: bool) -> &'static str {
        if vi {
            match self {
                Nav::General => "Chung",
                Nav::Accessibility => "Trợ năng",
                Nav::Appearance => "Giao diện",
                Nav::Island => "Island",
                Nav::Taskbar => "Thanh tác vụ",
                Nav::Panels => "Ghi chú",
                Nav::Widgets => "Tiện ích",
                Nav::Calls => "Cuộc gọi",
                Nav::Streaming => "Phát trực tiếp",
                Nav::Gaming => "Chơi game",
                Nav::AiAgents => "Trợ lý AI",
                Nav::Tools => "Công cụ",
                Nav::Keys => "Phím tắt",
                Nav::About => "Giới thiệu",
            }
        } else {
            match self {
                Nav::General => "General",
                Nav::Accessibility => "Accessibility",
                Nav::Appearance => "Appearance",
                Nav::Island => "Island",
                Nav::Taskbar => "Taskbar",
                Nav::Panels => "Panels",
                Nav::Widgets => "Widgets",
                Nav::Calls => "Calls",
                Nav::Streaming => "Streaming",
                Nav::Gaming => "Gaming",
                Nav::AiAgents => "AI agents",
                Nav::Tools => "Tools",
                Nav::Keys => "Keys",
                Nav::About => "About",
            }
        }
    }

    fn sym(self) -> Sym {
        match self {
            Nav::General => Sym::Gear,
            Nav::Accessibility => Sym::Accessibility,
            Nav::Appearance => Sym::Sparkles,
            Nav::Island => Sym::Music,
            Nav::Taskbar => Sym::Sliders,
            Nav::Panels => Sym::Folder,
            Nav::Widgets => Sym::Widgets,
            Nav::Calls => Sym::Mic,
            Nav::Streaming => Sym::Shield,
            Nav::Gaming => Sym::Gamepad,
            Nav::AiAgents => Sym::Sparkles,
            Nav::Tools => Sym::Tools,
            Nav::Keys => Sym::Keyboard,
            Nav::About => Sym::Info,
        }
    }
}

#[derive(Clone, Copy)]
enum Act {
    Close,
    Tab(Nav),
    SearchFocus,
    SearchClear,
    PreviewToggle,
    Surface(SurfaceTheme),
    Motion(MotionMode),
    Vis(VisStyle),
    Shape(IslandShape),
    Edge(IslandEdge),
    OpenTo(OpenTarget),
    Trigger(OpenTrigger),
    Lang(Lang),
    Ai(AiProvider),
    LaunchAi(AiProvider),
    ToggleStartup,
    ToggleIsland,
    ToggleGlow,
    ToggleGlareStrip,
    ToggleGreeting,
    ToggleGreetingRotate,
    ToggleDayClockGreeting,
    FocusGreetName,
    ClearGreetName,
    ToggleStickyOpen,
    ToggleDwellLong,
    ToggleHighContrast,
    ToggleShowAiTab,
    ToggleShowCallsTab,
    ToggleVolumeHud,
    ToggleNotifications,
    ToggleGameStrip,
    ToggleFpsCounter,
    ToggleFullscreenHide,
    ToggleTaskbar,
    ToggleTaskbarAutoReveal,
    ToggleTaskbarAutohideSide,
    ToggleTaskbarNowPlaying,
    ToggleDayClock,
    ToggleClock24,
    ToggleHotkeys,
    ToggleClipboard,
    ToggleCallsBanner,
    ToggleMicMute,
    ToggleStreamShield,
    ToggleStreamBadge,
    ToggleAiClipboard,
    ToggleWidget(WidgetKind),
    DayClockColor([u8; 3]),
    WheelPick(f32, f32, f32),
    WheelValue(f32, f32),
    SliderOpenSize(f32, f32),
    SliderSeeThrough(f32, f32),
    SliderCorners(f32, f32),
    SliderTextScale(f32, f32),
    SliderDayClockSize(f32, f32),
    SliderDockZoom(f32, f32),
    PickToolShortcut(usize),
    ClearToolShortcut(usize),
    OpenSystemTaskbarSettings,
    OpenConfigFolder,
    OpenRecordingsFolder,
    ReplayTour,
    RestoreWindowsTaskbar,
    QuitApp,
}

impl Act {
    fn is_draggable(self) -> bool {
        matches!(
            self,
            Act::SliderOpenSize(..)
                | Act::SliderSeeThrough(..)
                | Act::SliderCorners(..)
                | Act::SliderTextScale(..)
                | Act::SliderDayClockSize(..)
                | Act::SliderDockZoom(..)
                | Act::WheelPick(..)
                | Act::WheelValue(..)
        )
    }
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

struct SettingsWin {
    hwnd: HWND,
    _surface: Surface,
    panel: GlassPanel,
    content: ContainerVisual,
    header_layer: ContainerVisual,
    gfx: Gfx,
    logo: Option<CompositionDrawingSurface>,
    painter: Painter,
    dpi: f32,
    size: (f32, f32),
    nav: Nav,
    preview_open: bool,
    search: String,
    search_focused: bool,
    greet_focused: bool,
    scroll_y: f32,
    max_scroll: f32,
    wheel_surf: Option<(i32, CompositionDrawingSurface)>,
    val_bar_surf: Option<(i32, i32, u8, u8, u8, CompositionDrawingSurface)>,
    regions: Vec<Region>,
    drag_act: Option<Act>,
    quit_cb: fn(),
}

thread_local! {
    static WIN: RefCell<Option<SettingsWin>> = const { RefCell::new(None) };
}

fn with<R>(f: impl FnOnce(&mut SettingsWin) -> R) -> Option<R> {
    WIN.with(|c| c.try_borrow_mut().ok().and_then(|mut b| b.as_mut().map(f)))
}

pub fn open(comp: &Compositor, quit_cb: fn()) {
    let exists = with(|w| {
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
        let class = w!("LuminaSettings");
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
            w!("Lumina Settings"),
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
        capture::set_capture_excluded(hwnd, prefs::get().stream_privacy_shield);
        let Ok(surface) = attach(comp, hwnd, (pw as f32, ph as f32)) else {
            return;
        };
        let Ok(panel) = GlassPanel::new(
            comp,
            &surface.root,
            GlassParams::from_theme(SurfaceTheme::Acrylic, (pw as f32, ph as f32), 20.0 * dpi),
        ) else {
            return;
        };
        let Ok(content) = comp.CreateContainerVisual() else { return };
        let Ok(header_layer) = comp.CreateContainerVisual() else { return };
        let _ = surface.root.Children().and_then(|k| {
            let _ = k.InsertAtTop(&content);
            k.InsertAtTop(&header_layer)
        });
        let Ok(gfx) = Gfx::shared(comp) else { return };
        let pixels = np_platform::icons::decode_square(include_bytes!("../../../assets/lumina-logo.png"), 128);
        let logo = pixels.and_then(|b| gfx.surface_from_bgra(b.w, b.h, &b.bgra).ok());
        let painter = Painter::new(comp.clone(), gfx.clone(), dpi);
        WIN.with(|c| {
            *c.borrow_mut() = Some(SettingsWin {
                hwnd,
                _surface: surface,
                panel,
                content,
                header_layer,
                gfx,
                logo,
                painter,
                dpi,
                size: (pw as f32, ph as f32),
                nav: Nav::General,
                preview_open: true,
                search: String::new(),
                search_focused: false,
                greet_focused: false,
                scroll_y: 0.0,
                max_scroll: 0.0,
                wheel_surf: None,
                val_bar_surf: None,
                regions: Vec::new(),
                drag_act: None,
                quit_cb,
            });
        });
        with(|w| w.render());
        prefs::listen(hwnd);
        let _ = ShowWindow(hwnd, SW_SHOW);
    }
}

pub fn hide_if_open() {
    with(|w| unsafe {
        let _ = ShowWindow(w.hwnd, SW_HIDE);
    });
}

pub fn shutdown() {
    let win = WIN.with(|c| c.borrow_mut().take());
    if let Some(w) = win {
        let hwnd = w.hwnd;
        prefs::unlisten(hwnd);
        drop(w);
        unsafe {
            let _ = DestroyWindow(hwnd);
        }
    }
}

impl SettingsWin {
    fn render(&mut self) {
        let s = prefs::get();
        capture::set_capture_excluded(self.hwnd, s.stream_privacy_shield);
        let d = self.dpi;
        self.painter.reset(d);
        self.regions.clear();
        if let Ok(kids) = self.content.Children() {
            let _ = kids.RemoveAll();
        }
        if let Ok(kids) = self.header_layer.Children() {
            let _ = kids.RemoveAll();
        }

        let (w, h) = self.size;
        let _ = self.panel.set_params(
            GlassParams::from_theme(SurfaceTheme::Acrylic, (w, h), 20.0 * d),
            prefs::auto_tier(),
        );
        let _ = self.panel.set_rect(0.0, 0.0, w, h);

        let side_w = SIDE_W * d;
        let x0 = side_w + 24.0 * d;
        let cw = w - x0 - 24.0 * d;
        let header_h = HEADER_H * d;

        // Scrollable right-pane body (drawn first so fixed header & sidebar cleanly sit over scrolled edges)
        let mut y = header_h + 4.0 * d - self.scroll_y;
        let y_start = y;

        let q = self.search.trim().to_ascii_lowercase();
        if !q.is_empty() {
            y = self.render_search_results(x0, y, cw, &q, &s);
        } else {
            y = match self.nav {
                Nav::General => self.render_general(x0, y, cw, &s),
                Nav::Accessibility => self.render_accessibility(x0, y, cw, &s),
                Nav::Appearance => self.render_appearance(x0, y, cw, &s),
                Nav::Island => self.render_island(x0, y, cw, &s),
                Nav::Taskbar => self.render_taskbar(x0, y, cw, &s),
                Nav::Panels => self.render_panels(x0, y, cw, &s),
                Nav::Widgets => self.render_widgets(x0, y, cw, &s),
                Nav::Calls => self.render_calls(x0, y, cw, &s),
                Nav::Streaming => self.render_streaming(x0, y, cw, &s),
                Nav::Gaming => self.render_gaming(x0, y, cw, &s),
                Nav::AiAgents => self.render_ai(x0, y, cw, &s),
                Nav::Tools => self.render_tools(x0, y, cw, &s),
                Nav::Keys => self.render_keys(x0, y, cw, &s),
                Nav::About => self.render_about(x0, y, cw),
            };
        }

        let total_content_h = (y - y_start).max(0.0);
        let visible_h = (h - header_h - 14.0 * d).max(1.0);
        self.max_scroll = (total_content_h - visible_h).max(0.0);
        if self.scroll_y > self.max_scroll {
            self.scroll_y = self.max_scroll;
        }
        // Keep clickable regions confined inside the scrollable viewport below the fixed header
        self.regions.retain(|r| r.y + r.h > header_h && r.y < h - 4.0 * d);

        // Fixed right-pane opaque glass header backdrop covering scrolled rows passing underneath
        self.painter.grad_rect(
            &self.header_layer,
            side_w,
            0.0,
            w - side_w,
            header_h,
            0.0,
            rgba([18, 22, 32], 238),
            rgba([14, 18, 28], 228),
            false,
        );
        self.painter.rect(
            &self.header_layer,
            side_w,
            header_h - 1.0,
            w - side_w,
            1.0,
            0.0,
            rgba(WHITE, 22),
        );
        let vi = prefs::is_vi();
        let tr = |en: &'static str, v: &'static str| if vi { v } else { en };
        let title = if q.is_empty() {
            self.nav.title(vi)
        } else {
            tr("Search results", "Kết quả tìm kiếm")
        };
        self.painter.text(&self.header_layer, title, x0, 16.0 * d, 19.0, 700, WHITE, 1.0, Anchor::Left, cw - 60.0 * d);

        // Subtle scrollbar indicator when scrollable
        if self.max_scroll > 2.0 {
            let track_h = visible_h - 8.0 * d;
            let thumb_h = ((visible_h / (total_content_h + 1.0)) * track_h).clamp(26.0 * d, track_h);
            let thumb_y = header_h + 6.0 * d + (self.scroll_y / self.max_scroll) * (track_h - thumb_h);
            self.painter.rect(&self.header_layer, w - 8.0 * d, thumb_y, 4.0 * d, thumb_h, 2.0 * d, rgba(WHITE, 82));
        }

        // Close button in fixed header
        let cx = w - 26.0 * d;
        let cy = 24.0 * d;
        self.painter.card(&self.header_layer, cx - 13.0 * d, cy - 13.0 * d, 26.0 * d, 26.0 * d, 13.0 * d, WHITE, 24, 42);
        self.painter.sym(&self.header_layer, Sym::Close, cx, cy, 10.0, WHITE, 0.90);
        self.regions.push(Region {
            x: cx - 15.0 * d,
            y: cy - 15.0 * d,
            w: 30.0 * d,
            h: 30.0 * d,
            act: Act::Close,
        });

        // Fixed left sidebar (brand + search + 14 video sections)
        self.painter.grad_rect(&self.header_layer, 0.0, 0.0, side_w, h, 0.0, rgba([16, 18, 26], 235), rgba([10, 12, 18], 242), false);
        self.painter.rect(&self.header_layer, side_w - 1.0, 0.0, 1.0, h, 0.0, rgba(WHITE, 28));
        if let Some(logo) = &self.logo {
            self.painter.image(&self.header_layer, logo, 14.0 * d, 14.0 * d, 28.0 * d, 28.0 * d, 0.0);
        }
        self.painter.text(&self.header_layer, "Lumina", 50.0 * d, 13.0 * d, 14.5, 700, WHITE, 1.0, Anchor::Left, side_w - 58.0 * d);
        self.painter.text(&self.header_layer, "0.3.2", 50.0 * d, 31.0 * d, 9.0, 500, WHITE, 0.56, Anchor::Left, side_w - 58.0 * d);

        // Search box
        let sbx = 12.0 * d;
        let sby = 52.0 * d;
        let sbw = side_w - 24.0 * d;
        let sbh = 28.0 * d;
        self.painter.card(
            &self.header_layer,
            sbx,
            sby,
            sbw,
            sbh,
            9.0 * d,
            if self.search_focused { [72, 146, 255] } else { WHITE },
            if self.search_focused { 36 } else { 18 },
            if self.search_focused { 110 } else { 32 },
        );
        self.painter.sym(&self.header_layer, Sym::Search, sbx + 13.0 * d, sby + sbh / 2.0, 10.5, WHITE, 0.68);
        let search_txt = if self.search.is_empty() {
            if self.search_focused {
                tr("Type to filter...", "Nhập để tìm...")
            } else {
                tr("Search settings", "Tìm kiếm cài đặt")
            }
        } else {
            self.search.as_str()
        };
        let search_alpha = if self.search.is_empty() { 0.48 } else { 0.96 };
        self.painter.text(&self.header_layer, search_txt, sbx + 26.0 * d, sby + 6.5 * d, 10.5, 500, WHITE, search_alpha, Anchor::Left, sbw - 48.0 * d);
        self.regions.push(Region {
            x: sbx,
            y: sby,
            w: sbw - if self.search.is_empty() { 0.0 } else { 22.0 * d },
            h: sbh,
            act: Act::SearchFocus,
        });
        if !self.search.is_empty() {
            self.painter.sym(&self.header_layer, Sym::Close, sbx + sbw - 11.0 * d, sby + sbh / 2.0, 8.5, WHITE, 0.72);
            self.regions.push(Region {
                x: sbx + sbw - 22.0 * d,
                y: sby,
                w: 22.0 * d,
                h: sbh,
                act: Act::SearchClear,
            });
        }

        let mut ny = 90.0 * d;
        for item in Nav::ALL {
            let active = q.is_empty() && self.nav == item;
            let row_h = 29.0 * d;
            if active {
                self.painter.card(&self.header_layer, 10.0 * d, ny, side_w - 20.0 * d, row_h, 9.0 * d, [72, 138, 255], 96, 85);
                self.painter.rect(&self.header_layer, 14.0 * d, ny + 6.5 * d, 3.0 * d, 16.0 * d, 1.5 * d, rgba([120, 192, 255], 250));
            }
            self.painter.sym(&self.header_layer, item.sym(), 28.0 * d, ny + row_h / 2.0, 11.5, WHITE, if active { 1.0 } else { 0.62 });
            self.painter.text(&self.header_layer, item.title(vi), 44.0 * d, ny + 6.5 * d, 11.5, if active { 700 } else { 500 }, WHITE, if active { 1.0 } else { 0.78 }, Anchor::Left, side_w - 52.0 * d);
            self.regions.push(Region {
                x: 10.0 * d,
                y: ny,
                w: side_w - 20.0 * d,
                h: row_h,
                act: Act::Tab(item),
            });
            ny += 32.5 * d;
        }
    }

    fn render_general(&mut self, x0: f32, mut y: f32, cw: f32, s: &Settings) -> f32 {
        let d = self.dpi;
        let vi = prefs::is_vi();
        let tr = |en: &'static str, v: &'static str| if vi { v } else { en };
        y = self.seg_row(
            x0,
            y,
            cw,
            tr("Language", "Ngôn ngữ"),
            &Lang::ALL.iter().map(|l| (l.label_i18n(vi), Act::Lang(*l), s.lang == *l)).collect::<Vec<_>>(),
        );
        y = self.toggle_row(x0, y, cw, tr("Start Lumina on Windows sign-in", "Tự khởi động cùng Windows"), startup::is_enabled(), Act::ToggleStartup);
        y = self.toggle_row(x0, y, cw, tr("24-hour clock", "Định dạng đồng hồ 24 giờ"), s.clock_24h, Act::ToggleClock24);
        y = self.seg_row(
            x0,
            y,
            cw,
            tr("Open to", "Mở vào tab"),
            &OpenTarget::ALL.iter().map(|t| (t.label_i18n(vi), Act::OpenTo(*t), s.open_to == *t)).collect::<Vec<_>>(),
        );
        y = self.toggle_row(x0, y, cw, tr("Glare strip (5dp slim bar on maximized apps)", "Dải sáng mảnh 5dp khi cửa sổ phóng to"), s.glare_strip, Act::ToggleGlareStrip);
        y = self.toggle_row(x0, y, cw, tr("Hide resting pill under fullscreen apps", "Ẩn pill chờ khi mở ứng dụng toàn màn hình"), s.hide_under_fullscreen, Act::ToggleFullscreenHide);

        y += 4.0 * d;
        self.painter.text(&self.content, tr("Optional extras", "Tính năng tuỳ chọn"), x0, y, 11.5, 700, WHITE, 0.88, Anchor::Left, cw);
        y += 24.0 * d;
        y = self.toggle_row(x0, y, cw, tr("Notifications in the island", "Thông báo trên Island"), s.notifications_in_island, Act::ToggleNotifications);
        y = self.toggle_row(x0, y, cw, tr("Game strip (CPU / GPU / RAM)", "Thanh thông số Game (CPU / GPU / RAM)"), s.game_strip, Act::ToggleGameStrip);
        y = self.toggle_row(x0, y, cw, tr("FPS counter overlay", "Hiển thị bộ đếm FPS"), s.fps_counter, Act::ToggleFpsCounter);
        y = self.toggle_row(x0, y, cw, tr("Volume & brightness HUD in the island", "Thanh trượt âm lượng & độ sáng trên Island"), s.volume_in_island, Act::ToggleVolumeHud);
        y = self.toggle_row(x0, y, cw, tr("Clipboard history in Island", "Lịch sử Clipboard trên Island"), s.clipboard_history, Act::ToggleClipboard);
        self.toggle_row(x0, y, cw, tr("Global keyboard shortcuts", "Phím tắt toàn hệ thống"), s.hotkeys_enabled, Act::ToggleHotkeys)
    }

    fn render_accessibility(&mut self, x0: f32, mut y: f32, cw: f32, s: &Settings) -> f32 {
        let vi = prefs::is_vi();
        let tr = |en: &'static str, v: &'static str| if vi { v } else { en };
        y = self.toggle_row(x0, y, cw, tr("High-contrast borders & sharper text", "Viền tương phản cao & chữ sắc nét"), s.high_contrast, Act::ToggleHighContrast);
        y = self.toggle_row(x0, y, cw, tr("Longer hover dwell delay before opening", "Giữ chuột lâu hơn trước khi mở rộng"), s.dwell_long, Act::ToggleDwellLong);
        y = self.toggle_row(x0, y, cw, tr("Keep Island open until clicked outside", "Giữ Island luôn mở đến khi bấm ra ngoài"), s.sticky_open, Act::ToggleStickyOpen);
        y = self.seg_row(
            x0,
            y,
            cw,
            tr("Motion speed", "Hiệu ứng chuyển động"),
            &MotionMode::ALL.iter().map(|m| (m.label_i18n(vi), Act::Motion(*m), s.motion == *m)).collect::<Vec<_>>(),
        );
        self.slider_row(
            x0,
            y,
            cw,
            tr("Text scale", "Cỡ chữ"),
            (s.text_scale_pct as f32 - 90.0) / 50.0,
            &format!("{}%", s.text_scale_pct),
            |sx, sw| Act::SliderTextScale(sx, sw),
        )
    }

    fn render_appearance(&mut self, x0: f32, mut y: f32, cw: f32, s: &Settings) -> f32 {
        let d = self.dpi;
        let ph = 114.0 * d;
        self.painter.grad_rect(&self.content, x0, y, cw, ph, 18.0 * d, rgba([36, 56, 128], 225), rgba([126, 62, 134], 215), true);
        self.painter.grad_rect(&self.content, x0, y, cw, ph, 18.0 * d, rgba([72, 178, 255], 70), rgba([255, 134, 102], 55), false);
        self.painter.stroke_rect(&self.content, x0, y, cw, ph, 18.0 * d, 1.1, rgba(WHITE, 64), rgba(WHITE, 18));

        let spec = surface_spec(s.surface);
        let see_alpha = ((s.see_through_pct as f32 / 100.0) * 190.0 + 60.0).round().clamp(60.0, 252.0) as u8;
        let pw = if self.preview_open { 280.0 * d } else { 164.0 * d };
        let pih = if self.preview_open { 86.0 * d } else { 26.0 * d };
        let pr = ((if self.preview_open { 18.0 } else { 13.0 }) * d * s.corner_scale()).max(4.0 * d);
        let px = x0 + (cw - pw) / 2.0;
        let py = if s.shape == IslandShape::Notch { y } else { y + 10.0 * d };
        self.painter.soft_shadow(&self.content, px, py, pw, pih, pr, 8.0 * d, [0, 0, 0], 110);
        self.painter.grad_rect(
            &self.content,
            px,
            py,
            pw,
            pih,
            pr,
            Color { A: spec.tint[0].min(see_alpha).max(120), R: spec.tint[1], G: spec.tint[2], B: spec.tint[3] },
            Color { A: spec.tint_bottom[0].min(see_alpha).max(135), R: spec.tint_bottom[1], G: spec.tint_bottom[2], B: spec.tint_bottom[3] },
            false,
        );
        let rim_a = if s.high_contrast { 210 } else { spec.rim[0].max(90) };
        self.painter.stroke_rect(
            &self.content,
            px,
            py,
            pw,
            pih,
            pr,
            if s.high_contrast { 1.6 } else { 1.15 },
            Color { A: rim_a, R: spec.rim[1], G: spec.rim[2], B: spec.rim[3] },
            rgba(WHITE, 32),
        );
        if self.preview_open {
            let art_x = px + 12.0 * d;
            let art_y = py + 13.0 * d;
            self.painter.soft_shadow(&self.content, art_x, art_y, 44.0 * d, 44.0 * d, 10.0 * d, 5.0 * d, [245, 118, 72], 120);
            self.painter.grad_rect(&self.content, art_x, art_y, 44.0 * d, 44.0 * d, 10.0 * d, rgba([255, 134, 86], 245), rgba([192, 56, 112], 245), false);
            self.painter.sym(&self.content, Sym::Music, art_x + 22.0 * d, art_y + 22.0 * d, 16.0, WHITE, 0.96);
            self.painter.text(&self.content, "Neon Harbor", px + 66.0 * d, py + 16.0 * d, 11.5, 700, spec.fg, 0.98, Anchor::Left, 130.0 * d);
            self.painter.text(&self.content, "Kael Rao", px + 66.0 * d, py + 33.0 * d, 10.0, 500, spec.fg, 0.72, Anchor::Left, 130.0 * d);
            self.painter.card(&self.content, px + pw - 36.0 * d, py + 19.0 * d, 24.0 * d, 24.0 * d, 12.0 * d, [245, 118, 72], 140, 85);
            self.painter.sym(&self.content, Sym::Pause, px + pw - 24.0 * d, py + 31.0 * d, 10.0, WHITE, 0.96);
            self.painter.wavy_progress(&self.content, px + 14.0 * d, py + pih - 13.0 * d, pw - 28.0 * d, 0.62, 1.4, spec.glow, true);
        } else {
            self.painter.grad_rect(&self.content, px + 8.0 * d, py + 5.0 * d, 16.0 * d, 16.0 * d, 4.0 * d, rgba([255, 134, 86], 240), rgba([192, 56, 112], 240), false);
            self.painter.text(&self.content, "Static Bloom", px + pw / 2.0, py + 5.5 * d, 9.5, 600, spec.fg, 0.94, Anchor::Center, pw - 50.0 * d);
        }
        let vi = prefs::is_vi();
        let tr = |en: &'static str, v: &'static str| if vi { v } else { en };
        let btn_w = 114.0 * d;
        let btn_x = x0 + cw - btn_w - 10.0 * d;
        let btn_y = y + ph - 28.0 * d;
        self.painter.card(&self.content, btn_x, btn_y, btn_w, 22.0 * d, 11.0 * d, [12, 14, 22], 125, 64);
        let mode_lbl = if self.preview_open {
            tr("Mode: Open", "Chế độ: Mở")
        } else {
            tr("Mode: Resting", "Chế độ: Nghỉ")
        };
        self.painter.text(&self.content, mode_lbl, btn_x + btn_w / 2.0, btn_y + 4.0 * d, 9.5, 600, WHITE, 0.96, Anchor::Center, btn_w);
        self.regions.push(Region {
            x: btn_x,
            y: btn_y,
            w: btn_w,
            h: 22.0 * d,
            act: Act::PreviewToggle,
        });
        y += ph + 10.0 * d;

        y = self.slider_row(x0, y, cw, tr("Open size", "Cỡ khi mở rộng"), (s.open_size_pct as f32 - 80.0) / 45.0, &format!("{}%", s.open_size_pct), |sx, sw| Act::SliderOpenSize(sx, sw));
        y = self.slider_row(x0, y, cw, tr("See-through surface", "Độ trong suốt"), (s.see_through_pct as f32 - 10.0) / 90.0, &format!("{}%", s.see_through_pct), |sx, sw| Act::SliderSeeThrough(sx, sw));
        y = self.seg_row(
            x0,
            y,
            cw,
            tr("Motion", "Chuyển động"),
            &MotionMode::ALL.iter().map(|m| (m.label_i18n(vi), Act::Motion(*m), s.motion == *m)).collect::<Vec<_>>(),
        );
        y = self.seg_row(
            x0,
            y,
            cw,
            tr("Visualizer", "Sóng nhạc"),
            &VisStyle::ALL.iter().map(|v| (v.label_i18n(vi), Act::Vis(*v), s.visualizer == *v)).collect::<Vec<_>>(),
        );
        y = self.slider_row(x0, y, cw, tr("Corners", "Bo góc"), s.corner_pct as f32 / 100.0, &format!("{}%", s.corner_pct), |sx, sw| Act::SliderCorners(sx, sw));

        self.painter.text(&self.content, tr("Surface (10 themes)", "Chất liệu nền (10 chủ đề)"), x0, y + 3.0 * d, 11.5, 700, WHITE, 0.92, Anchor::Left, cw);
        y += 24.0 * d;
        let gap = 7.0 * d;
        let tw = (cw - 4.0 * gap) / 5.0;
        let th = 38.0 * d;
        for (i, theme) in SurfaceTheme::ALL.into_iter().enumerate() {
            let (col, row) = (i % 5, i / 5);
            let tx = x0 + col as f32 * (tw + gap);
            let ty = y + row as f32 * (th + gap);
            let on = s.surface == theme;
            let sp = surface_spec(theme);
            self.painter.grad_rect(
                &self.content,
                tx,
                ty,
                tw,
                th,
                11.0 * d,
                Color { A: 242, R: sp.solid[1], G: sp.solid[2], B: sp.solid[3] },
                Color { A: 248, R: sp.tint_bottom[1], G: sp.tint_bottom[2], B: sp.tint_bottom[3] },
                false,
            );
            self.painter.stroke_rect(
                &self.content,
                tx,
                ty,
                tw,
                th,
                11.0 * d,
                if on { 1.8 } else { 1.0 },
                if on { rgba([92, 168, 255], 255) } else { Color { A: sp.rim[0], R: sp.rim[1], G: sp.rim[2], B: sp.rim[3] } },
                rgba(WHITE, if on { 180 } else { 28 }),
            );
            if on {
                self.painter.rect(&self.content, tx + tw - 15.0 * d, ty + 4.0 * d, 11.0 * d, 11.0 * d, 5.5 * d, rgba([76, 152, 255], 250));
                self.painter.sym(&self.content, Sym::Check, tx + tw - 9.5 * d, ty + 9.5 * d, 7.5, WHITE, 0.98);
            }
            self.painter.text(&self.content, theme.label_i18n(vi), tx + tw / 2.0, ty + 12.0 * d, 9.5, if on { 700 } else { 500 }, WHITE, 0.96, Anchor::Center, tw - 8.0 * d);
            self.regions.push(Region {
                x: tx,
                y: ty,
                w: tw,
                h: th,
                act: Act::Surface(theme),
            });
        }
        y + 2.0 * th + gap + 8.0 * d
    }

    fn render_island(&mut self, x0: f32, mut y: f32, cw: f32, s: &Settings) -> f32 {
        let d = self.dpi;
        let vi = prefs::is_vi();
        let tr = |en: &'static str, v: &'static str| if vi { v } else { en };
        let sys_user = std::env::var("USERNAME").unwrap_or_default();
        let name = if s.greet_name.trim().is_empty() {
            sys_user.clone()
        } else {
            s.greet_name.clone()
        };
        let preview_g = greeting_text(&s.greet_text, &name, time::local_now().hour, vi);

        // Live time-based Greeting banner + toggles (Abra-inspired)
        self.painter.card(&self.content, x0, y, cw, 56.0 * d, 13.0 * d, [72, 146, 255], 26, 46);
        self.painter.rect(&self.content, x0 + 14.0 * d, y + 23.0 * d, 9.0 * d, 9.0 * d, 4.5 * d, rgba([120, 198, 255], 245));
        self.painter.text(&self.content, &preview_g, x0 + 32.0 * d, y + 11.0 * d, 12.5, 700, WHITE, 0.98, Anchor::Left, cw - 48.0 * d);
        self.painter.text(
            &self.content,
            tr(
                "Updates automatically by hour (Morning / Noon / Afternoon / Evening)",
                "Tự cập nhật theo giờ trong ngày (Sáng / Trưa / Chiều / Tối)",
            ),
            x0 + 32.0 * d,
            y + 32.0 * d,
            9.8,
            500,
            WHITE,
            0.66,
            Anchor::Left,
            cw - 48.0 * d,
        );
        y += 64.0 * d;

        y = self.toggle_row(x0, y, cw, tr("Time-based greeting when Island is idle", "Hiển thị lời chào theo giờ khi Island nghỉ"), s.show_greeting, Act::ToggleGreeting);
        y = self.toggle_row(x0, y, cw, tr("Rotate idle greeting with clock, date & battery", "Luân chuyển lời chào với giờ, ngày & pin"), s.greeting_rotate, Act::ToggleGreetingRotate);
        y = self.toggle_row(x0, y, cw, tr("Show greeting above Desktop Day Clock", "Hiện lời chào phía trên Desktop Day Clock"), s.day_clock_greeting, Act::ToggleDayClockGreeting);

        // Editable nickname row
        self.painter.card(
            &self.content,
            x0,
            y,
            cw,
            38.0 * d,
            11.0 * d,
            if self.greet_focused { [72, 146, 255] } else { WHITE },
            if self.greet_focused { 30 } else { 14 },
            if self.greet_focused { 110 } else { 24 },
        );
        self.painter.text(&self.content, tr("Greeting nickname", "Biệt danh lời chào"), x0 + 14.0 * d, y + 11.0 * d, 11.0, 600, WHITE, 0.94, Anchor::Left, 150.0 * d);
        let disp = if s.greet_name.is_empty() {
            if self.greet_focused { tr("Type name...", "Nhập tên...") } else { sys_user.as_str() }
        } else {
            s.greet_name.as_str()
        };
        let has_custom = !s.greet_name.is_empty();
        let right_end = if has_custom { cw - 62.0 * d } else { cw - 14.0 * d };
        self.painter.text(
            &self.content,
            disp,
            x0 + right_end,
            y + 11.0 * d,
            11.0,
            600,
            if s.greet_name.is_empty() { [156, 206, 255] } else { WHITE },
            if s.greet_name.is_empty() { 0.72 } else { 0.96 },
            Anchor::Right,
            (cw - 190.0 * d).max(60.0 * d),
        );
        self.regions.push(Region {
            x: x0,
            y,
            w: if has_custom { cw - 60.0 * d } else { cw },
            h: 38.0 * d,
            act: Act::FocusGreetName,
        });
        if has_custom {
            let cbx = x0 + cw - 54.0 * d;
            self.painter.card(&self.content, cbx, y + 7.0 * d, 44.0 * d, 24.0 * d, 10.0 * d, WHITE, 24, 38);
            self.painter.text(&self.content, tr("Reset", "Đặt lại"), cbx + 22.0 * d, y + 12.0 * d, 9.5, 600, WHITE, 0.92, Anchor::Center, 42.0 * d);
            self.regions.push(Region { x: cbx, y: y + 7.0 * d, w: 44.0 * d, h: 24.0 * d, act: Act::ClearGreetName });
        }
        y += 46.0 * d;

        y = self.toggle_row(x0, y, cw, tr("Enable Dynamic Island", "Bật Dynamic Island"), s.island_enabled, Act::ToggleIsland);
        y = self.seg_row(
            x0,
            y,
            cw,
            tr("Edge", "Vị trí cạnh"),
            &IslandEdge::ALL.iter().map(|e| (e.label_i18n(vi), Act::Edge(*e), s.edge == *e)).collect::<Vec<_>>(),
        );
        y = self.seg_row(
            x0,
            y,
            cw,
            tr("Shape", "Kiểu dáng"),
            &IslandShape::ALL.iter().map(|sh| (sh.label_i18n(vi), Act::Shape(*sh), s.shape == *sh)).collect::<Vec<_>>(),
        );
        y = self.seg_row(
            x0,
            y,
            cw,
            tr("Open on", "Cách mở"),
            &[
                (tr("Hover", "Rê chuột"), Act::Trigger(OpenTrigger::Hover), s.open_trigger == OpenTrigger::Hover),
                (tr("Click", "Nhấp chuột"), Act::Trigger(OpenTrigger::Click), s.open_trigger == OpenTrigger::Click),
            ],
        );
        y = self.seg_row(
            x0,
            y,
            cw,
            tr("Open to", "Mở vào tab"),
            &OpenTarget::ALL.iter().map(|t| (t.label_i18n(vi), Act::OpenTo(*t), s.open_to == *t)).collect::<Vec<_>>(),
        );
        y = self.toggle_row(x0, y, cw, tr("Glare strip when maximized app sits under Top edge", "Thu thành dải mảnh khi cửa sổ phóng to ở cạnh trên"), s.glare_strip, Act::ToggleGlareStrip);
        y = self.toggle_row(x0, y, cw, tr("Keep open until clicked outside", "Giữ mở cho đến khi bấm ra ngoài"), s.sticky_open, Act::ToggleStickyOpen);
        y = self.toggle_row(x0, y, cw, tr("Album color glow on pill", "Ánh sáng toả theo màu bìa nhạc"), s.island_glow, Act::ToggleGlow);
        y = self.toggle_row(x0, y, cw, tr("Volume & brightness HUD flash", "Hiện nhanh thanh âm lượng & độ sáng"), s.volume_in_island, Act::ToggleVolumeHud);
        y = self.toggle_row(x0, y, cw, tr("Notifications in the island", "Thông báo trên Island"), s.notifications_in_island, Act::ToggleNotifications);
        y = self.toggle_row(x0, y, cw, tr("Show AI agents tab in Island", "Hiện tab Trợ lý AI trên Island"), s.show_ai_tab, Act::ToggleShowAiTab);
        self.toggle_row(x0, y, cw, tr("Show Calls & Meet tab in Island", "Hiện tab Cuộc gọi trên Island"), s.show_calls_tab, Act::ToggleShowCallsTab)
    }

    fn render_taskbar(&mut self, x0: f32, mut y: f32, cw: f32, s: &Settings) -> f32 {
        let d = self.dpi;
        let vi = prefs::is_vi();
        let tr = |en: &'static str, v: &'static str| if vi { v } else { en };
        y = self.toggle_row(x0, y, cw, tr("Lumina Taskbar (Glass replacement)", "Thanh tác vụ Glass Lumina"), s.taskbar_enabled, Act::ToggleTaskbar);
        y = self.toggle_row(x0, y, cw, tr("Auto-show taskbar at bottom edge", "Tự hiện thanh tác vụ ở mép dưới"), s.taskbar_auto_reveal, Act::ToggleTaskbarAutoReveal);
        y = self.toggle_row(x0, y, cw, tr("Auto-hide taskbar when Island is docked Left/Right", "Tự ẩn taskbar khi Island gắn cạnh Trái/Phải"), s.taskbar_autohide_side, Act::ToggleTaskbarAutohideSide);
        y = self.toggle_row(x0, y, cw, tr("Show Now Playing in taskbar", "Hiện bài đang phát trên thanh tác vụ"), s.taskbar_show_now_playing, Act::ToggleTaskbarNowPlaying);
        y = self.slider_row(x0, y, cw, tr("Dock icon zoom", "Phóng to biểu tượng"), (s.taskbar_dock_zoom_pct as f32 - 100.0) / 80.0, &format!("{}%", s.taskbar_dock_zoom_pct), |sx, sw| Act::SliderDockZoom(sx, sw));
        y = self.button_row(x0, y + 6.0 * d, cw, tr("Open Windows taskbar settings", "Mở cài đặt Taskbar của Windows"), Act::OpenSystemTaskbarSettings);
        self.button_row(x0, y, cw, tr("Restore native Windows taskbar now", "Khôi phục Taskbar gốc của Windows ngay"), Act::RestoreWindowsTaskbar)
    }

    fn render_panels(&mut self, x0: f32, mut y: f32, cw: f32, s: &Settings) -> f32 {
        let d = self.dpi;
        let vi = prefs::is_vi();
        let tr = |en: &'static str, v: &'static str| if vi { v } else { en };
        self.painter.card(&self.content, x0, y, cw, 92.0 * d, 14.0 * d, [72, 138, 255], 22, 34);
        self.painter.text(&self.content, tr("Quick Notes & File Shelf Panels", "Ghi chú nhanh & Ngăn chứa tệp"), x0 + 14.0 * d, y + 12.0 * d, 12.5, 700, WHITE, 0.96, Anchor::Left, cw - 28.0 * d);
        self.painter.text(
            &self.content,
            tr(
                "Type live quick notes directly inside the Island 'Panels' tab, pin dragged files in the File Shelf, and recall copied text from 'Clipboard'.",
                "Lưu nhanh văn bản trong tab 'Ghi chú' của Island, giữ tệp kéo thả trong Ngăn tệp và lấy lại chữ đã chép ở mục 'Clipboard'.",
            ),
            x0 + 14.0 * d,
            y + 34.0 * d,
            10.5,
            500,
            WHITE,
            0.76,
            Anchor::Left,
            cw - 28.0 * d,
        );
        let note_preview = if s.notes.is_empty() {
            tr("(empty)", "(trống)").to_string()
        } else {
            s.notes.join(" · ")
        };
        let notes_hdr = if vi {
            format!("Ghi chú đã lưu ({}): {}", s.notes.len(), note_preview)
        } else {
            format!("Saved notes ({}): {}", s.notes.len(), note_preview)
        };
        self.painter.text(
            &self.content,
            &notes_hdr,
            x0 + 14.0 * d,
            y + 66.0 * d,
            10.0,
            600,
            [138, 198, 255],
            0.92,
            Anchor::Left,
            cw - 28.0 * d,
        );
        y += 102.0 * d;
        y = self.toggle_row(x0, y, cw, tr("Clipboard history panel in Island", "Bảng lịch sử Clipboard trên Island"), s.clipboard_history, Act::ToggleClipboard);
        y = self.toggle_row(x0, y, cw, tr("Notifications feed in Island", "Bảng thông báo trên Island"), s.notifications_in_island, Act::ToggleNotifications);
        self.button_row(x0, y + 4.0 * d, cw, tr("Open saved notes & shelf folder", "Mở thư mục lưu ghi chú & cấu hình"), Act::OpenConfigFolder)
    }

    fn render_widgets(&mut self, x0: f32, mut y: f32, cw: f32, s: &Settings) -> f32 {
        let d = self.dpi;
        let vi = prefs::is_vi();
        let tr = |en: &'static str, v: &'static str| if vi { v } else { en };
        y = self.toggle_row(x0, y, cw, tr("Day clock on the desktop (Anurati)", "Đồng hồ thứ/ngày trên màn hình (Anurati)"), s.day_clock.enabled, Act::ToggleDayClock);
        y = self.slider_row(x0, y, cw, tr("Day clock size", "Kích thước Day Clock"), (s.day_clock.size_pct as f32 - 55.0) / 145.0, &format!("{}%", s.day_clock.size_pct), |sx, sw| Act::SliderDayClockSize(sx, sw));

        // Interactive HSV Color Wheel + Value bar + Hex badge + Preset Swatches
        let card_h = 160.0 * d;
        self.painter.card(&self.content, x0, y, cw, card_h, 14.0 * d, WHITE, 14, 24);
        self.painter.text(&self.content, tr("Day clock colour (Color wheel + presets)", "Màu chữ Day Clock (Bánh xe màu + mẫu sẵn)"), x0 + 14.0 * d, y + 10.0 * d, 11.5, 600, WHITE, 0.94, Anchor::Left, cw - 120.0 * d);
        let hex = hex_color(s.day_clock.color);
        let hx = x0 + cw - 86.0 * d;
        let hy = y + 9.0 * d;
        self.painter.card(&self.content, hx, hy, 72.0 * d, 22.0 * d, 11.0 * d, s.day_clock.color, 55, 115);
        self.painter.text(&self.content, &hex, hx + 36.0 * d, hy + 4.5 * d, 10.0, 700, WHITE, 0.98, Anchor::Center, 68.0 * d);

        let wheel_d = (104.0 * d).round() as i32;
        let wheel_f = wheel_d as f32;
        let wx = x0 + 18.0 * d;
        let wy = y + 38.0 * d;
        self.draw_hsv_wheel(wx, wy, wheel_d);
        let (h_deg, sat, val) = rgb_to_hsv(s.day_clock.color);
        let rad = h_deg.to_radians();
        let rr = sat * (wheel_f * 0.48);
        let dot_x = wx + wheel_f / 2.0 + rr * rad.cos();
        let dot_y = wy + wheel_f / 2.0 + rr * rad.sin();
        self.painter.rect(&self.content, dot_x - 6.0 * d, dot_y - 6.0 * d, 12.0 * d, 12.0 * d, 6.0 * d, rgba([12, 14, 20], 220));
        self.painter.rect(&self.content, dot_x - 4.0 * d, dot_y - 4.0 * d, 8.0 * d, 8.0 * d, 4.0 * d, rgba(s.day_clock.color, 255));
        self.regions.push(Region {
            x: wx,
            y: wy,
            w: wheel_f,
            h: wheel_f,
            act: Act::WheelPick(wx + wheel_f / 2.0, wy + wheel_f / 2.0, wheel_f * 0.48),
        });

        // Brightness / Value bar to the right of the wheel
        let vx = wx + wheel_f + 16.0 * d;
        let vy = wy + 8.0 * d;
        let vw = (cw - wheel_f - 54.0 * d).max(110.0 * d);
        let vh = 16.0 * d;
        let pure_rgb = hsv_to_rgb(h_deg, sat, 1.0);
        self.draw_value_bar(vx, vy, vw.round() as i32, vh.round() as i32, pure_rgb);
        let vkx = vx + val.clamp(0.0, 1.0) * vw - 7.0 * d;
        self.painter.soft_shadow(&self.content, vkx, vy + vh / 2.0 - 7.0 * d, 14.0 * d, 14.0 * d, 7.0 * d, 2.5 * d, [0, 0, 0], 90);
        self.painter.rect(&self.content, vkx, vy + vh / 2.0 - 7.0 * d, 14.0 * d, 14.0 * d, 7.0 * d, rgba(WHITE, 252));
        self.regions.push(Region {
            x: vx - 4.0 * d,
            y: vy - 6.0 * d,
            w: vw + 8.0 * d,
            h: vh + 12.0 * d,
            act: Act::WheelValue(vx, vw),
        });

        // Quick preset swatches below the brightness bar
        let sw_y = vy + 42.0 * d;
        self.painter.text(&self.content, tr("Quick presets", "Màu chọn nhanh"), vx, sw_y - 18.0 * d, 9.5, 500, WHITE, 0.65, Anchor::Left, vw);
        let palette: [[u8; 3]; 7] = [
            [255, 255, 255],
            [200, 224, 255],
            [255, 212, 168],
            [255, 174, 206],
            [168, 244, 212],
            [206, 186, 255],
            [245, 92, 92],
        ];
        for (i, rgb) in palette.into_iter().enumerate() {
            let sx = vx + i as f32 * 28.0 * d;
            let on = s.day_clock.color == rgb;
            if on {
                self.painter.rect(&self.content, sx - 3.0 * d, sw_y - 3.0 * d, 26.0 * d, 26.0 * d, 13.0 * d, rgba([76, 152, 255], 250));
                self.painter.rect(&self.content, sx - 1.0 * d, sw_y - 1.0 * d, 22.0 * d, 22.0 * d, 11.0 * d, rgba([18, 22, 32], 255));
            }
            let inset = if on { 0.5 * d } else { 0.0 };
            let sz = 20.0 * d - 2.0 * inset;
            self.painter.rect(&self.content, sx + inset, sw_y + inset, sz, sz, sz / 2.0, rgba(rgb, 255));
            self.painter.stroke_rect(&self.content, sx + inset, sw_y + inset, sz, sz, sz / 2.0, 1.0, rgba([0, 0, 0], 65), rgba([0, 0, 0], 95));
            self.regions.push(Region {
                x: sx,
                y: sw_y,
                w: 20.0 * d,
                h: 20.0 * d,
                act: Act::DayClockColor(rgb),
            });
        }
        y += card_h + 12.0 * d;

        self.painter.text(&self.content, tr("Desktop Glass Widgets (or drag tiles out of the Island!)", "Tiện ích kính ngoài màn hình (hoặc kéo trực tiếp từ Island!)"), x0, y, 11.5, 700, WHITE, 0.85, Anchor::Left, cw);
        y += 26.0 * d;
        for kind in WidgetKind::ALL {
            let on = desktop_widgets::has_widget(kind);
            y = self.toggle_row(x0, y, cw, kind.label_i18n(vi), on, Act::ToggleWidget(kind));
        }
        y
    }

    fn render_calls(&mut self, x0: f32, mut y: f32, cw: f32, s: &Settings) -> f32 {
        let d = self.dpi;
        let vi = prefs::is_vi();
        let tr = |en: &'static str, v: &'static str| if vi { v } else { en };
        self.painter.card(&self.content, x0, y, cw, 64.0 * d, 14.0 * d, [72, 146, 255], 28, 48);
        self.painter.sym(&self.content, Sym::Mic, x0 + 26.0 * d, y + 32.0 * d, 18.0, [138, 198, 255], 0.98);
        self.painter.text(&self.content, tr("Universal Microphone Mute & Meet Hub", "Tắt/Mở Micro Toàn Hệ Thống & Cuộc Gọi"), x0 + 52.0 * d, y + 14.0 * d, 12.5, 700, WHITE, 0.98, Anchor::Left, cw - 68.0 * d);
        self.painter.text(&self.content, tr("Press Ctrl + Alt + M or click below to toggle mute across Zoom, Meet, Teams, and Discord.", "Nhấn Ctrl + Alt + M hoặc bấm nút dưới để tắt/mở mic cho Zoom, Meet, Teams, Discord."), x0 + 52.0 * d, y + 34.0 * d, 10.0, 500, WHITE, 0.72, Anchor::Left, cw - 68.0 * d);
        y += 74.0 * d;

        y = self.button_row(x0, y, cw, tr("Toggle Universal Microphone Mute (Ctrl + Alt + M)", "Bật / Tắt Micro Toàn Cục (Ctrl + Alt + M)"), Act::ToggleMicMute);
        y = self.toggle_row(x0, y, cw, tr("Show Calls & Meet quick controls tab in Island", "Hiện tab Cuộc gọi trên Island"), s.show_calls_tab, Act::ToggleShowCallsTab);
        self.toggle_row(x0, y, cw, tr("Show mic & camera indicator banner", "Hiện chấm báo trạng thái Micro & Camera"), s.calls_mic_banner, Act::ToggleCallsBanner)
    }

    fn render_streaming(&mut self, x0: f32, mut y: f32, cw: f32, s: &Settings) -> f32 {
        let d = self.dpi;
        let vi = prefs::is_vi();
        let tr = |en: &'static str, v: &'static str| if vi { v } else { en };
        self.painter.card(&self.content, x0, y, cw, 66.0 * d, 14.0 * d, [72, 146, 255], 24, 38);
        self.painter.sym(&self.content, Sym::Shield, x0 + 26.0 * d, y + 33.0 * d, 18.0, [138, 198, 255], 0.98);
        self.painter.text(&self.content, tr("Stream Capture Shield (WDA_EXCLUDEFROMCAPTURE)", "Khiên Ẩn Quay Màn Hình (WDA_EXCLUDEFROMCAPTURE)"), x0 + 52.0 * d, y + 14.0 * d, 12.5, 700, WHITE, 0.98, Anchor::Left, cw - 68.0 * d);
        self.painter.text(&self.content, tr("Hides Settings, Island, and overlay windows from OBS, Discord, and screen shares.", "Ẩn cửa sổ Cài đặt và Island khỏi OBS, Discord và khi chia sẻ màn hình."), x0 + 52.0 * d, y + 34.0 * d, 10.0, 500, WHITE, 0.72, Anchor::Left, cw - 68.0 * d);
        y += 76.0 * d;

        y = self.toggle_row(x0, y, cw, tr("Hide Lumina windows during screen capture", "Ẩn Lumina khi quay hoặc chia sẻ màn hình"), s.stream_privacy_shield, Act::ToggleStreamShield);
        y = self.toggle_row(x0, y, cw, tr("Show live REC MM:SS pill in Dynamic Island", "Hiện đồng hồ REC MM:SS khi đang quay"), s.show_stream_badge, Act::ToggleStreamBadge);
        self.button_row(x0, y + 4.0 * d, cw, tr("Open screen recordings folder", "Mở thư mục video đã quay"), Act::OpenRecordingsFolder)
    }

    fn render_gaming(&mut self, x0: f32, mut y: f32, cw: f32, s: &Settings) -> f32 {
        let vi = prefs::is_vi();
        let tr = |en: &'static str, v: &'static str| if vi { v } else { en };
        y = self.toggle_row(x0, y, cw, tr("Fullscreen Game Strip (CPU / GPU / RAM)", "Thanh thông số Game toàn màn hình (CPU / GPU / RAM)"), s.game_strip, Act::ToggleGameStrip);
        y = self.toggle_row(x0, y, cw, tr("Show FPS counter in Game Strip", "Hiện bộ đếm FPS trong thanh Game"), s.fps_counter, Act::ToggleFpsCounter);
        self.toggle_row(x0, y, cw, tr("Hide resting pill under non-game fullscreen apps", "Ẩn pill chờ dưới ứng dụng toàn màn hình khác"), s.hide_under_fullscreen, Act::ToggleFullscreenHide)
    }

    fn render_ai(&mut self, x0: f32, mut y: f32, cw: f32, s: &Settings) -> f32 {
        let d = self.dpi;
        let vi = prefs::is_vi();
        let tr = |en: &'static str, v: &'static str| if vi { v } else { en };
        y = self.toggle_row(x0, y, cw, tr("Show AI agents tab in Island", "Hiện tab Trợ lý AI trên Island"), s.show_ai_tab, Act::ToggleShowAiTab);
        y = self.toggle_row(x0, y, cw, tr("Copy clipboard snippet before opening assistant", "Tự sao chép ngữ cảnh Clipboard khi mở trợ lý"), s.ai_include_clipboard, Act::ToggleAiClipboard);

        self.painter.text(&self.content, tr("Default AI agent", "Trợ lý AI mặc định"), x0, y + 4.0 * d, 11.5, 700, WHITE, 0.90, Anchor::Left, cw);
        y += 24.0 * d;
        let gap = 7.0 * d;
        let bw = (cw - 4.0 * gap) / 5.0;
        let bh = 38.0 * d;
        for (i, ai) in AiProvider::ALL.into_iter().enumerate() {
            let bx = x0 + i as f32 * (bw + gap);
            let on = s.default_ai == ai;
            self.painter.card(&self.content, bx, y, bw, bh, 10.0 * d, if on { [72, 148, 255] } else { WHITE }, if on { 165 } else { 18 }, if on { 95 } else { 28 });
            self.painter.text(&self.content, ai.label(), bx + bw / 2.0, y + 11.5 * d, 10.0, if on { 700 } else { 500 }, WHITE, 0.96, Anchor::Center, bw - 8.0 * d);
            self.regions.push(Region { x: bx, y, w: bw, h: bh, act: Act::Ai(ai) });
        }
        y += bh + 14.0 * d;

        self.painter.text(&self.content, tr("Quick launch AI workspace", "Mở nhanh trợ lý AI"), x0, y, 11.0, 600, WHITE, 0.78, Anchor::Left, cw);
        y += 22.0 * d;
        for ai in AiProvider::ALL {
            let lbl = if vi {
                format!("Mở {}", ai.label())
            } else {
                format!("Open {}", ai.label())
            };
            y = self.button_row(x0, y, cw, &lbl, Act::LaunchAi(ai));
        }
        y
    }

    fn render_tools(&mut self, x0: f32, mut y: f32, cw: f32, s: &Settings) -> f32 {
        let d = self.dpi;
        let vi = prefs::is_vi();
        let tr = |en: &'static str, v: &'static str| if vi { v } else { en };
        self.painter.card(&self.content, x0, y, cw, 62.0 * d, 14.0 * d, [72, 146, 255], 20, 34);
        self.painter.text(&self.content, tr("Island Tools & + Add Shortcut Slots", "Bộ công cụ Island & 5 ô phím tắt tuỳ chỉnh"), x0 + 14.0 * d, y + 12.0 * d, 12.5, 700, WHITE, 0.96, Anchor::Left, cw - 28.0 * d);
        self.painter.text(&self.content, tr("Includes Screenshot, Snip, MJPEG .avi Screen Recorder, Eyedropper Color Picker, Clipboard QR, File Shelf & 5 app slots.", "Gồm Chụp màn hình, Cắt vùng, Quay màn hình .avi, Lấy mã màu, QR gửi tệp, Ngăn tệp & 5 ô mở nhanh."), x0 + 14.0 * d, y + 32.0 * d, 10.0, 500, WHITE, 0.72, Anchor::Left, cw - 28.0 * d);
        y += 72.0 * d;

        for idx in 0..5 {
            let path = s.tool_shortcuts.get(idx).map(|p| p.as_str()).unwrap_or("");
            let name = if path.is_empty() {
                tr("Empty slot", "Ô trống").to_string()
            } else {
                std::path::Path::new(path)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or(path)
                    .to_string()
            };
            let slot_lbl = if vi {
                format!("Ô {}: {}", idx + 1, name)
            } else {
                format!("Slot {}: {}", idx + 1, name)
            };
            self.painter.card(&self.content, x0, y, cw, 38.0 * d, 11.0 * d, WHITE, 14, 24);
            self.painter.text(&self.content, &slot_lbl, x0 + 14.0 * d, y + 11.0 * d, 11.0, 600, WHITE, if path.is_empty() { 0.56 } else { 0.96 }, Anchor::Left, cw - 154.0 * d);
            let pbw = 72.0 * d;
            let pbx = x0 + cw - pbw - if path.is_empty() { 10.0 * d } else { 64.0 * d };
            self.painter.card(&self.content, pbx, y + 7.0 * d, pbw, 24.0 * d, 10.0 * d, [72, 148, 255], 150, 85);
            self.painter.text(&self.content, tr("Pick...", "Chọn..."), pbx + pbw / 2.0, y + 12.0 * d, 10.0, 700, WHITE, 0.98, Anchor::Center, pbw);
            self.regions.push(Region { x: pbx, y: y + 7.0 * d, w: pbw, h: 24.0 * d, act: Act::PickToolShortcut(idx) });
            if !path.is_empty() {
                let cbx = x0 + cw - 56.0 * d;
                self.painter.card(&self.content, cbx, y + 7.0 * d, 46.0 * d, 24.0 * d, 10.0 * d, [245, 92, 92], 95, 75);
                self.painter.text(&self.content, tr("Clear", "Xoá"), cbx + 23.0 * d, y + 12.0 * d, 9.5, 600, WHITE, 0.96, Anchor::Center, 44.0 * d);
                self.regions.push(Region { x: cbx, y: y + 7.0 * d, w: 46.0 * d, h: 24.0 * d, act: Act::ClearToolShortcut(idx) });
            }
            y += 46.0 * d;
        }
        self.button_row(x0, y + 4.0 * d, cw, tr("Open screen recordings folder", "Mở thư mục video đã quay"), Act::OpenRecordingsFolder)
    }

    fn render_keys(&mut self, x0: f32, mut y: f32, cw: f32, s: &Settings) -> f32 {
        let d = self.dpi;
        let vi = prefs::is_vi();
        let tr = |en: &'static str, v: &'static str| if vi { v } else { en };
        y = self.toggle_row(x0, y, cw, tr("Enable global keyboard shortcuts", "Bật phím tắt toàn hệ thống"), s.hotkeys_enabled, Act::ToggleHotkeys);
        for (k, desc) in [
            ("Ctrl + Alt + N", tr("Expand or collapse the Dynamic Island", "Mở rộng hoặc thu gọn Dynamic Island")),
            ("Ctrl + Alt + M", tr("Toggle universal microphone mute", "Bật / tắt micro trên mọi ứng dụng")),
            ("Ctrl + Alt + V", tr("Open Clipboard History inside Island", "Mở Lịch sử Clipboard trên Island")),
            (tr("Wheel on Island", "Lăn chuột / Island"), tr("Cycle between Music / Widgets / Notes / Clipboard / Tools / AI / Quick / Calls", "Chuyển nhanh giữa các tab của Island")),
            (tr("Wheel on Clock", "Lăn chuột / Đồng hồ"), tr("Resize Desktop Day Clock smoothly", "Thay đổi kích thước Day Clock mượt mà")),
            (tr("Drag from Widgets", "Kéo từ Tiện ích"), tr("Pull any Island widget tile onto desktop to pin it", "Kéo thẻ tiện ích bất kỳ ra desktop để ghim")),
        ] {
            self.painter.card(&self.content, x0, y, cw, 38.0 * d, 11.0 * d, WHITE, 16, 28);
            self.painter.card(&self.content, x0 + 10.0 * d, y + 6.0 * d, 154.0 * d, 26.0 * d, 8.0 * d, [72, 146, 255], 44, 52);
            self.painter.text(&self.content, k, x0 + 87.0 * d, y + 11.5 * d, 10.5, 700, [138, 198, 255], 0.98, Anchor::Center, 148.0 * d);
            self.painter.text(&self.content, desc, x0 + 178.0 * d, y + 11.5 * d, 10.5, 500, WHITE, 0.88, Anchor::Left, cw - 190.0 * d);
            y += 45.0 * d;
        }
        y
    }

    fn render_about(&mut self, x0: f32, mut y: f32, cw: f32) -> f32 {
        let d = self.dpi;
        let vi = prefs::is_vi();
        let tr = |en: &'static str, v: &'static str| if vi { v } else { en };
        self.painter.card(&self.content, x0, y, cw, 112.0 * d, 15.0 * d, [72, 138, 255], 28, 42);
        self.painter.card(&self.content, x0 + 16.0 * d, y + 15.0 * d, 36.0 * d, 36.0 * d, 10.0 * d, [72, 146, 255], 175, 110);
        self.painter.sym(&self.content, Sym::Sparkles, x0 + 34.0 * d, y + 33.0 * d, 18.0, WHITE, 0.98);
        self.painter.text(&self.content, tr("Lumina  (Native Rust Desktop Hub)", "Lumina  (Ứng dụng Desktop Rust gốc)"), x0 + 62.0 * d, y + 15.0 * d, 13.5, 700, WHITE, 1.0, Anchor::Left, cw - 76.0 * d);
        self.painter.text(&self.content, "v0.3.2  ·  Rust + windows-rs 0.62 + Windows.UI.Composition Liquid Glass", x0 + 62.0 * d, y + 37.0 * d, 10.5, 500, WHITE, 0.78, Anchor::Left, cw - 76.0 * d);
        let cfg_lbl = if vi {
            format!("Thư mục cấu hình: {}", paths::config_dir().display())
        } else {
            format!("Config: {}", paths::config_dir().display())
        };
        self.painter.text(&self.content, &cfg_lbl, x0 + 16.0 * d, y + 72.0 * d, 10.5, 500, WHITE, 0.64, Anchor::Left, cw - 32.0 * d);
        y += 124.0 * d;
        y = self.button_row(x0, y, cw, tr("Replay First-run onboarding tour", "Xem lại hướng dẫn khởi đầu"), Act::ReplayTour);
        y = self.button_row(x0, y, cw, tr("Open config folder", "Mở thư mục cấu hình"), Act::OpenConfigFolder);
        self.button_row(x0, y + 4.0 * d, cw, tr("Quit Lumina (restores Windows taskbar)", "Thoát Lumina (khôi phục taskbar Windows)"), Act::QuitApp)
    }

    fn render_search_results(&mut self, x0: f32, mut y: f32, cw: f32, q: &str, s: &Settings) -> f32 {
        let d = self.dpi;
        let vi = prefs::is_vi();
        let mut count = 0usize;
        let items: Vec<(&str, &str, &str, &str, bool, Act)> = vec![
            ("General", "Chung", "Start Lumina on Windows sign-in", "Tự khởi động cùng Windows", startup::is_enabled(), Act::ToggleStartup),
            ("General", "Chung", "24-hour clock", "Định dạng đồng hồ 24 giờ", s.clock_24h, Act::ToggleClock24),
            ("General", "Chung", "Glare strip (5dp slim bar)", "Dải sáng mảnh 5dp", s.glare_strip, Act::ToggleGlareStrip),
            ("General", "Chung", "Hide under fullscreen apps", "Ẩn khi mở toàn màn hình", s.hide_under_fullscreen, Act::ToggleFullscreenHide),
            ("General", "Chung", "Notifications in the island", "Thông báo trên Island", s.notifications_in_island, Act::ToggleNotifications),
            ("Gaming", "Chơi game", "Fullscreen Game Strip (CPU / GPU / RAM)", "Thanh thông số Game", s.game_strip, Act::ToggleGameStrip),
            ("Gaming", "Chơi game", "FPS counter overlay", "Bộ đếm FPS", s.fps_counter, Act::ToggleFpsCounter),
            ("Island", "Island", "Time-based greeting when Island is idle", "Lời chào theo giờ khi Island nghỉ", s.show_greeting, Act::ToggleGreeting),
            ("Island", "Island", "Rotate idle greeting with clock & battery", "Luân chuyển lời chào với giờ & pin", s.greeting_rotate, Act::ToggleGreetingRotate),
            ("Widgets", "Tiện ích", "Show greeting above Desktop Day Clock", "Hiện lời chào trên Day Clock", s.day_clock_greeting, Act::ToggleDayClockGreeting),
            ("Island", "Island", "Volume & brightness HUD flash", "Thanh âm lượng & độ sáng HUD", s.volume_in_island, Act::ToggleVolumeHud),
            ("Panels", "Ghi chú", "Clipboard history in Island", "Lịch sử Clipboard trên Island", s.clipboard_history, Act::ToggleClipboard),
            ("Keys", "Phím tắt", "Global keyboard shortcuts", "Phím tắt toàn hệ thống", s.hotkeys_enabled, Act::ToggleHotkeys),
            ("Accessibility", "Trợ năng", "High-contrast borders & sharper text", "Viền tương phản cao", s.high_contrast, Act::ToggleHighContrast),
            ("Accessibility", "Trợ năng", "Longer hover dwell delay", "Giữ chuột lâu hơn trước khi mở", s.dwell_long, Act::ToggleDwellLong),
            ("Island", "Island", "Keep Island open until clicked outside", "Giữ Island luôn mở", s.sticky_open, Act::ToggleStickyOpen),
            ("Island", "Island", "Enable Dynamic Island", "Bật Dynamic Island", s.island_enabled, Act::ToggleIsland),
            ("Island", "Island", "Album color glow on pill", "Ánh sáng toả màu nhạc", s.island_glow, Act::ToggleGlow),
            ("AI agents", "Trợ lý AI", "Show AI agents tab in Island", "Hiện tab Trợ lý AI", s.show_ai_tab, Act::ToggleShowAiTab),
            ("Calls", "Cuộc gọi", "Show Calls & Meet tab in Island", "Hiện tab Cuộc gọi", s.show_calls_tab, Act::ToggleShowCallsTab),
            ("Calls", "Cuộc gọi", "Show mic & camera indicator banner", "Hiện chấm báo Micro & Camera", s.calls_mic_banner, Act::ToggleCallsBanner),
            ("Streaming", "Phát trực tiếp", "Hide Lumina windows during screen capture (Privacy Shield)", "Ẩn Lumina khi chia sẻ màn hình", s.stream_privacy_shield, Act::ToggleStreamShield),
            ("Streaming", "Phát trực tiếp", "Show live REC badge in Dynamic Island", "Hiện đồng hồ REC khi quay", s.show_stream_badge, Act::ToggleStreamBadge),
            ("Taskbar", "Thanh tác vụ", "Lumina Taskbar (Glass replacement)", "Thanh tác vụ Glass Lumina", s.taskbar_enabled, Act::ToggleTaskbar),
            ("Taskbar", "Thanh tác vụ", "Auto-show taskbar at bottom edge", "Tự hiện thanh tác vụ ở mép dưới", s.taskbar_auto_reveal, Act::ToggleTaskbarAutoReveal),
            ("Taskbar", "Thanh tác vụ", "Auto-hide taskbar when Island is Left/Right", "Tự ẩn taskbar khi Island ở cạnh", s.taskbar_autohide_side, Act::ToggleTaskbarAutohideSide),
            ("Taskbar", "Thanh tác vụ", "Show Now Playing in taskbar", "Hiện bài đang phát trên taskbar", s.taskbar_show_now_playing, Act::ToggleTaskbarNowPlaying),
            ("Widgets", "Tiện ích", "Day clock on the desktop (Anurati)", "Đồng hồ Day Clock (Anurati)", s.day_clock.enabled, Act::ToggleDayClock),
        ];

        for (ce, cv, le, lv, on, act) in items {
            if ce.to_ascii_lowercase().contains(q)
                || cv.to_lowercase().contains(q)
                || le.to_ascii_lowercase().contains(q)
                || lv.to_lowercase().contains(q)
            {
                let (cat, label) = if vi { (cv, lv) } else { (ce, le) };
                y = self.toggle_row(x0, y, cw, &format!("[{}] {}", cat, label), on, act);
                count += 1;
            }
        }
        for nav in Nav::ALL {
            let te = nav.title(false);
            let tv = nav.title(true);
            if te.to_ascii_lowercase().contains(q) || tv.to_lowercase().contains(q) {
                let lbl = if vi {
                    format!("Mở mục {}", tv)
                } else {
                    format!("Open {} section", te)
                };
                y = self.button_row(x0, y, cw, &lbl, Act::Tab(nav));
                count += 1;
            }
        }
        if count == 0 {
            self.painter.text(&self.content, if vi { "Không tìm thấy cài đặt phù hợp." } else { "No matching settings found." }, x0, y + 12.0 * d, 12.0, 500, WHITE, 0.68, Anchor::Left, cw);
            y += 40.0 * d;
        }
        y
    }

    fn draw_hsv_wheel(&mut self, x: f32, y: f32, size: i32) {
        let need_rebuild = match &self.wheel_surf {
            Some((s, _)) => *s != size,
            None => true,
        };
        if need_rebuild {
            let n = size.max(16) as usize;
            let mut bgra = vec![0u8; n * n * 4];
            let half = n as f32 / 2.0;
            let rad = half - 1.5;
            for py in 0..n {
                for px in 0..n {
                    let dx = px as f32 + 0.5 - half;
                    let dy = py as f32 + 0.5 - half;
                    let dist = (dx * dx + dy * dy).sqrt();
                    if dist <= rad + 1.0 {
                        let sat = (dist / rad).clamp(0.0, 1.0);
                        let mut hue = dy.atan2(dx).to_degrees();
                        if hue < 0.0 {
                            hue += 360.0;
                        }
                        let [r, g, b] = hsv_to_rgb(hue, sat, 1.0);
                        let cov = ((rad + 0.5) - dist).clamp(0.0, 1.0);
                        let a = (cov * 255.0).round() as u8;
                        let idx = (py * n + px) * 4;
                        bgra[idx] = ((b as u16 * a as u16) / 255) as u8;
                        bgra[idx + 1] = ((g as u16 * a as u16) / 255) as u8;
                        bgra[idx + 2] = ((r as u16 * a as u16) / 255) as u8;
                        bgra[idx + 3] = a;
                    }
                }
            }
            if let Ok(surf) = self.gfx.surface_from_bgra(size.max(1) as u32, size.max(1) as u32, &bgra) {
                self.wheel_surf = Some((size, surf));
            }
        }
        if let Some((_, surf)) = &self.wheel_surf {
            if let (Ok(v), Ok(brush)) = (self.painter.comp.CreateSpriteVisual(), self.painter.comp.CreateSurfaceBrushWithSurface(surf)) {
                let _ = brush.SetStretch(CompositionStretch::Fill);
                let _ = v.SetBrush(&brush);
                let _ = v.SetSize(windows_numerics::Vector2 { X: size as f32, Y: size as f32 });
                let _ = v.SetOffset(windows_numerics::Vector3 { X: x, Y: y, Z: 0.0 });
                let _ = self.content.Children().and_then(|c| c.InsertAtTop(&v));
            }
        }
    }

    fn draw_value_bar(&mut self, x: f32, y: f32, w: i32, h: i32, pure_rgb: [u8; 3]) {
        let w = w.max(24);
        let h = h.max(10);
        let need_rebuild = match &self.val_bar_surf {
            Some((cw, ch, r, g, b, _)) => *cw != w || *ch != h || *r != pure_rgb[0] || *g != pure_rgb[1] || *b != pure_rgb[2],
            None => true,
        };
        if need_rebuild {
            let (wu, hu) = (w as usize, h as usize);
            let mut bgra = vec![0u8; wu * hu * 4];
            let rad = hu as f32 / 2.0;
            for py in 0..hu {
                for px in 0..wu {
                    // Pill mask
                    let cx = (px as f32 + 0.5).clamp(rad, wu as f32 - rad);
                    let dx = (px as f32 + 0.5) - cx;
                    let dy = (py as f32 + 0.5) - rad;
                    let cov = (rad - (dx * dx + dy * dy).sqrt() + 0.5).clamp(0.0, 1.0);
                    let a = (cov * 255.0).round() as u8;
                    let f = px as f32 / (wu - 1).max(1) as f32;
                    let r = (pure_rgb[0] as f32 * f).round() as u8;
                    let g = (pure_rgb[1] as f32 * f).round() as u8;
                    let b = (pure_rgb[2] as f32 * f).round() as u8;
                    let idx = (py * wu + px) * 4;
                    bgra[idx] = ((b as u16 * a as u16) / 255) as u8;
                    bgra[idx + 1] = ((g as u16 * a as u16) / 255) as u8;
                    bgra[idx + 2] = ((r as u16 * a as u16) / 255) as u8;
                    bgra[idx + 3] = a;
                }
            }
            if let Ok(surf) = self.gfx.surface_from_bgra(w.max(1) as u32, h.max(1) as u32, &bgra) {
                self.val_bar_surf = Some((w, h, pure_rgb[0], pure_rgb[1], pure_rgb[2], surf));
            }
        }
        if let Some((_, _, _, _, _, surf)) = &self.val_bar_surf {
            if let (Ok(v), Ok(brush)) = (self.painter.comp.CreateSpriteVisual(), self.painter.comp.CreateSurfaceBrushWithSurface(surf)) {
                let _ = brush.SetStretch(CompositionStretch::Fill);
                let _ = v.SetBrush(&brush);
                let _ = v.SetSize(windows_numerics::Vector2 { X: w as f32, Y: h as f32 });
                let _ = v.SetOffset(windows_numerics::Vector3 { X: x, Y: y, Z: 0.0 });
                let _ = self.content.Children().and_then(|c| c.InsertAtTop(&v));
            }
        }
    }

    fn toggle_row(&mut self, x: f32, y: f32, w: f32, label: &str, on: bool, act: Act) -> f32 {
        let d = self.dpi;
        let h = 38.0 * d;
        self.painter.card(&self.content, x, y, w, h, 11.0 * d, WHITE, 14, 24);
        self.painter.text(&self.content, label, x + 14.0 * d, y + 11.0 * d, 11.0, 600, WHITE, 0.94, Anchor::Left, w - 76.0 * d);
        let sw = 42.0 * d;
        let sh = 22.0 * d;
        let sx = x + w - sw - 12.0 * d;
        let sy = y + (h - sh) / 2.0;
        self.painter.card(&self.content, sx, sy, sw, sh, sh / 2.0, if on { [64, 156, 255] } else { WHITE }, if on { 220 } else { 28 }, if on { 95 } else { 32 });
        let kd = 16.0 * d;
        let kx = if on { sx + sw - kd - 3.0 * d } else { sx + 3.0 * d };
        self.painter.soft_shadow(&self.content, kx, sy + 3.0 * d, kd, kd, kd / 2.0, 2.5 * d, [0, 0, 0], 70);
        self.painter.rect(&self.content, kx, sy + 3.0 * d, kd, kd, kd / 2.0, rgba(WHITE, 252));
        self.regions.push(Region { x, y, w, h, act });
        y + h + 8.0 * d
    }

    fn seg_row(&mut self, x: f32, y: f32, w: f32, label: &str, opts: &[(&str, Act, bool)]) -> f32 {
        let d = self.dpi;
        let h = 38.0 * d;
        self.painter.card(&self.content, x, y, w, h, 11.0 * d, WHITE, 14, 24);
        self.painter.text(&self.content, label, x + 14.0 * d, y + 11.0 * d, 11.0, 600, WHITE, 0.94, Anchor::Left, 140.0 * d);
        let sw = (w * 0.62).min(316.0 * d);
        let sx = x + w - sw - 10.0 * d;
        let sh = 26.0 * d;
        let sy = y + (h - sh) / 2.0;
        self.painter.card(&self.content, sx, sy, sw, sh, 8.5 * d, [8, 10, 16], 72, 24);
        let item_w = sw / opts.len().max(1) as f32;
        for (i, (title, act, on)) in opts.iter().enumerate() {
            let ix = sx + i as f32 * item_w;
            if *on {
                self.painter.card(&self.content, ix + 2.0 * d, sy + 2.0 * d, item_w - 4.0 * d, sh - 4.0 * d, 6.5 * d, [72, 148, 255], 175, 95);
            }
            self.painter.text(&self.content, title, ix + item_w / 2.0, sy + 5.5 * d, 10.0, if *on { 700 } else { 500 }, WHITE, if *on { 1.0 } else { 0.72 }, Anchor::Center, item_w);
            self.regions.push(Region { x: ix, y: sy, w: item_w, h: sh, act: *act });
        }
        y + h + 8.0 * d
    }

    fn slider_row(&mut self, x: f32, y: f32, w: f32, label: &str, frac: f32, val_txt: &str, make_act: impl Fn(f32, f32) -> Act) -> f32 {
        let d = self.dpi;
        let h = 38.0 * d;
        self.painter.card(&self.content, x, y, w, h, 11.0 * d, WHITE, 14, 24);
        self.painter.text(&self.content, label, x + 14.0 * d, y + 11.0 * d, 11.0, 600, WHITE, 0.94, Anchor::Left, 164.0 * d);
        let sw = 200.0 * d;
        let sx = x + w - sw - 54.0 * d;
        let sy = y + h / 2.0;
        let frac = frac.clamp(0.0, 1.0);
        self.painter.rect(&self.content, sx, sy - 3.0 * d, sw, 6.0 * d, 3.0 * d, rgba(WHITE, 38));
        self.painter.grad_rect(&self.content, sx, sy - 3.0 * d, (sw * frac).max(6.0 * d), 6.0 * d, 3.0 * d, rgba([72, 148, 255], 245), rgba([124, 198, 255], 245), true);
        let kx = sx + sw * frac - 7.5 * d;
        self.painter.soft_shadow(&self.content, kx, sy - 7.5 * d, 15.0 * d, 15.0 * d, 7.5 * d, 3.0 * d, [0, 0, 0], 80);
        self.painter.rect(&self.content, kx, sy - 7.5 * d, 15.0 * d, 15.0 * d, 7.5 * d, rgba(WHITE, 255));
        self.painter.text(&self.content, val_txt, x + w - 12.0 * d, y + 11.0 * d, 10.5, 600, WHITE, 0.88, Anchor::Right, 46.0 * d);
        self.regions.push(Region {
            x: sx - 6.0 * d,
            y,
            w: sw + 12.0 * d,
            h,
            act: make_act(sx, sw),
        });
        y + h + 8.0 * d
    }

    fn button_row(&mut self, x: f32, y: f32, w: f32, label: &str, act: Act) -> f32 {
        let d = self.dpi;
        let h = 36.0 * d;
        self.painter.card(&self.content, x, y, w, h, 11.0 * d, [72, 138, 255], 95, 72);
        self.painter.text(&self.content, label, x + w / 2.0, y + 10.0 * d, 11.5, 600, WHITE, 0.98, Anchor::Center, w - 16.0 * d);
        self.regions.push(Region { x, y, w, h, act });
        y + h + 8.0 * d
    }

    fn apply_act(&mut self, act: Act, lx: f32, ly: f32) {
        if !matches!(act, Act::SearchFocus) && self.search_focused {
            self.search_focused = false;
        }
        if !matches!(act, Act::FocusGreetName) && self.greet_focused {
            self.greet_focused = false;
        }
        match act {
            Act::Close => unsafe {
                let _ = ShowWindow(self.hwnd, SW_HIDE);
            },
            Act::Tab(n) => {
                self.nav = n;
                self.search.clear();
                self.scroll_y = 0.0;
                self.render();
            }
            Act::SearchFocus => {
                self.search_focused = true;
                self.greet_focused = false;
                self.scroll_y = 0.0;
                self.render();
            }
            Act::SearchClear => {
                self.search.clear();
                self.scroll_y = 0.0;
                self.render();
            }
            Act::FocusGreetName => {
                self.greet_focused = true;
                self.search_focused = false;
                self.render();
            }
            Act::ClearGreetName => {
                prefs::update(|s| s.greet_name.clear());
            }
            Act::PreviewToggle => {
                self.preview_open = !self.preview_open;
                self.render();
            }
            Act::Surface(t) => prefs::update(|s| s.surface = t),
            Act::Motion(m) => prefs::update(|s| s.motion = m),
            Act::Vis(v) => prefs::update(|s| s.visualizer = v),
            Act::Shape(sh) => prefs::update(|s| s.shape = sh),
            Act::Edge(e) => prefs::update(|s| s.edge = e),
            Act::OpenTo(t) => prefs::update(|s| s.open_to = t),
            Act::Trigger(tr) => prefs::update(|s| s.open_trigger = tr),
            Act::Lang(l) => prefs::update(|s| s.lang = l),
            Act::Ai(a) => prefs::update(|s| s.default_ai = a),
            Act::LaunchAi(a) => {
                taskbar::launch_path(a.url());
            }
            Act::ToggleStartup => {
                let on = !startup::is_enabled();
                let _ = startup::set_enabled(on);
                self.render();
            }
            Act::ToggleIsland => prefs::update(|s| s.island_enabled = !s.island_enabled),
            Act::ToggleGlow => prefs::update(|s| s.island_glow = !s.island_glow),
            Act::ToggleGlareStrip => prefs::update(|s| s.glare_strip = !s.glare_strip),
            Act::ToggleGreeting => prefs::update(|s| s.show_greeting = !s.show_greeting),
            Act::ToggleGreetingRotate => prefs::update(|s| s.greeting_rotate = !s.greeting_rotate),
            Act::ToggleDayClockGreeting => prefs::update(|s| s.day_clock_greeting = !s.day_clock_greeting),
            Act::ToggleStickyOpen => prefs::update(|s| s.sticky_open = !s.sticky_open),
            Act::ToggleDwellLong => prefs::update(|s| s.dwell_long = !s.dwell_long),
            Act::ToggleHighContrast => prefs::update(|s| s.high_contrast = !s.high_contrast),
            Act::ToggleShowAiTab => prefs::update(|s| s.show_ai_tab = !s.show_ai_tab),
            Act::ToggleShowCallsTab => prefs::update(|s| s.show_calls_tab = !s.show_calls_tab),
            Act::ToggleVolumeHud => prefs::update(|s| s.volume_in_island = !s.volume_in_island),
            Act::ToggleNotifications => prefs::update(|s| s.notifications_in_island = !s.notifications_in_island),
            Act::ToggleGameStrip => prefs::update(|s| s.game_strip = !s.game_strip),
            Act::ToggleFpsCounter => prefs::update(|s| s.fps_counter = !s.fps_counter),
            Act::ToggleFullscreenHide => prefs::update(|s| s.hide_under_fullscreen = !s.hide_under_fullscreen),
            Act::ToggleTaskbar => prefs::update(|s| s.taskbar_enabled = !s.taskbar_enabled),
            Act::ToggleTaskbarAutoReveal => prefs::update(|s| s.taskbar_auto_reveal = !s.taskbar_auto_reveal),
            Act::ToggleTaskbarAutohideSide => prefs::update(|s| s.taskbar_autohide_side = !s.taskbar_autohide_side),
            Act::ToggleTaskbarNowPlaying => prefs::update(|s| s.taskbar_show_now_playing = !s.taskbar_show_now_playing),
            Act::ToggleDayClock => prefs::update(|s| s.day_clock.enabled = !s.day_clock.enabled),
            Act::ToggleClock24 => prefs::update(|s| s.clock_24h = !s.clock_24h),
            Act::ToggleHotkeys => prefs::update(|s| s.hotkeys_enabled = !s.hotkeys_enabled),
            Act::ToggleClipboard => prefs::update(|s| {
                s.clipboard_history = !s.clipboard_history;
                clipboard::set_enabled(s.clipboard_history);
            }),
            Act::ToggleCallsBanner => prefs::update(|s| s.calls_mic_banner = !s.calls_mic_banner),
            Act::ToggleMicMute => {
                island::toggle_mic_mute();
                self.render();
            }
            Act::ToggleStreamShield => prefs::update(|s| s.stream_privacy_shield = !s.stream_privacy_shield),
            Act::ToggleStreamBadge => prefs::update(|s| s.show_stream_badge = !s.show_stream_badge),
            Act::ToggleAiClipboard => prefs::update(|s| s.ai_include_clipboard = !s.ai_include_clipboard),
            Act::ToggleWidget(kind) => {
                desktop_widgets::toggle_widget(kind);
                self.render();
            }
            Act::DayClockColor(c) => prefs::update(|s| s.day_clock.color = c),
            Act::WheelPick(cx, cy, r) => {
                let dx = lx - cx;
                let dy = ly - cy;
                let sat = ((dx * dx + dy * dy).sqrt() / r.max(1.0)).clamp(0.0, 1.0);
                let mut hue = dy.atan2(dx).to_degrees();
                if hue < 0.0 {
                    hue += 360.0;
                }
                let (_, _, v) = rgb_to_hsv(prefs::get().day_clock.color);
                let rgb = hsv_to_rgb(hue, sat, v.max(0.15));
                prefs::update(|s| s.day_clock.color = rgb);
            }
            Act::WheelValue(vx, vw) => {
                let f = ((lx - vx) / vw.max(1.0)).clamp(0.08, 1.0);
                let (h, sat, _) = rgb_to_hsv(prefs::get().day_clock.color);
                let rgb = hsv_to_rgb(h, sat, f);
                prefs::update(|s| s.day_clock.color = rgb);
            }
            Act::SliderOpenSize(sx, sw) => {
                let f = ((lx - sx) / sw.max(1.0)).clamp(0.0, 1.0);
                let v = (80.0 + f * 45.0).round() as u8;
                prefs::update(|s| s.open_size_pct = v);
            }
            Act::SliderSeeThrough(sx, sw) => {
                let f = ((lx - sx) / sw.max(1.0)).clamp(0.0, 1.0);
                let v = (10.0 + f * 90.0).round() as u8;
                prefs::update(|s| s.see_through_pct = v);
            }
            Act::SliderCorners(sx, sw) => {
                let f = ((lx - sx) / sw.max(1.0)).clamp(0.0, 1.0);
                let v = (f * 100.0).round() as u8;
                prefs::update(|s| s.corner_pct = v);
            }
            Act::SliderTextScale(sx, sw) => {
                let f = ((lx - sx) / sw.max(1.0)).clamp(0.0, 1.0);
                let v = (90.0 + f * 50.0).round() as u8;
                prefs::update(|s| s.text_scale_pct = v);
            }
            Act::SliderDayClockSize(sx, sw) => {
                let f = ((lx - sx) / sw.max(1.0)).clamp(0.0, 1.0);
                let v = (55.0 + f * 145.0).round() as u8;
                prefs::update(|s| s.day_clock.size_pct = v);
            }
            Act::SliderDockZoom(sx, sw) => {
                let f = ((lx - sx) / sw.max(1.0)).clamp(0.0, 1.0);
                let v = (100.0 + f * 80.0).round() as u8;
                prefs::update(|s| s.taskbar_dock_zoom_pct = v);
            }
            Act::PickToolShortcut(idx) => {
                if let Some(p) = taskbar::pick_file_dialog(self.hwnd) {
                    prefs::update(|s| {
                        if idx < s.tool_shortcuts.len() {
                            s.tool_shortcuts[idx] = p;
                        }
                    });
                }
            }
            Act::ClearToolShortcut(idx) => {
                prefs::update(|s| {
                    if let Some(slot) = s.tool_shortcuts.get_mut(idx) {
                        slot.clear();
                    }
                });
            }
            Act::OpenSystemTaskbarSettings => taskbar::open_system_taskbar_settings(),
            Act::OpenConfigFolder => {
                let _ = std::fs::create_dir_all(paths::config_dir());
                taskbar::launch_path(&paths::config_dir().to_string_lossy());
            }
            Act::OpenRecordingsFolder => {
                let p = std::env::var_os("USERPROFILE")
                    .map(std::path::PathBuf::from)
                    .unwrap_or_else(paths::config_dir)
                    .join("Videos")
                    .join("Lumina");
                let _ = std::fs::create_dir_all(&p);
                taskbar::launch_path(&p.to_string_lossy());
            }
            Act::ReplayTour => crate::open_tour_window(),
            Act::RestoreWindowsTaskbar => {
                prefs::update(|s| s.taskbar_enabled = false);
                taskbar::restore();
            }
            Act::QuitApp => (self.quit_cb)(),
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
                pt.y < (52.0 * w.dpi) as i32 && !w.regions.iter().any(|r| r.hit(pt.x as f32, pt.y as f32))
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
                // Iterate in reverse so top-most header/overlay regions win
                if let Some(r) = w.regions.iter().rev().find(|r| r.hit(x, y)) {
                    let act = r.act;
                    if act.is_draggable() {
                        w.drag_act = Some(act);
                        SetCapture(hwnd);
                    }
                    w.apply_act(act, x, y);
                } else {
                    w.search_focused = false;
                    w.greet_focused = false;
                    w.render();
                }
            });
            LRESULT(0)
        }
        WM_MOUSEMOVE => {
            let x = (lp.0 & 0xFFFF) as i16 as f32;
            let y = ((lp.0 >> 16) & 0xFFFF) as i16 as f32;
            with(|w| {
                if let Some(act) = w.drag_act {
                    w.apply_act(act, x, y);
                }
            });
            LRESULT(0)
        }
        WM_LBUTTONUP => {
            with(|w| {
                if w.drag_act.take().is_some() {
                    let _ = ReleaseCapture();
                }
            });
            LRESULT(0)
        }
        WM_MOUSEWHEEL => {
            let delta = ((wp.0 >> 16) & 0xFFFF) as i16 as f32;
            with(|w| {
                if w.max_scroll > 0.0 {
                    w.scroll_y = (w.scroll_y - delta * 0.35).clamp(0.0, w.max_scroll);
                    w.render();
                }
            });
            LRESULT(0)
        }
        WM_CHAR => {
            let ch = wp.0 as u32;
            with(|w| {
                if ch < 0x20 {
                    return;
                }
                let Some(c) = char::from_u32(ch) else {
                    return;
                };
                if w.search_focused {
                    if w.search.len() < 48 {
                        w.search.push(c);
                        w.scroll_y = 0.0;
                        w.render();
                    }
                } else if w.greet_focused && prefs::get().greet_name.chars().count() < 24 {
                    prefs::update(|s| s.greet_name.push(c));
                }
            });
            LRESULT(0)
        }
        WM_KEYDOWN => {
            let vk = wp.0 as u32;
            with(|w| {
                if vk == 0x08 {
                    if w.search_focused && !w.search.is_empty() {
                        w.search.pop();
                        w.scroll_y = 0.0;
                        w.render();
                    } else if w.greet_focused && !prefs::get().greet_name.is_empty() {
                        prefs::update(|s| {
                            s.greet_name.pop();
                        });
                    }
                } else if vk == 0x0D && w.greet_focused {
                    w.greet_focused = false;
                    w.render();
                } else if vk == 0x1B {
                    if w.greet_focused {
                        w.greet_focused = false;
                        w.render();
                    } else if !w.search.is_empty() {
                        w.search.clear();
                        w.scroll_y = 0.0;
                        w.render();
                    } else {
                        unsafe {
                            let _ = ShowWindow(hwnd, SW_HIDE);
                        }
                    }
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

