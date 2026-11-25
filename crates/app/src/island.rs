use crate::{desktop_widgets, prefs, taskbar_bar};
use np_core::island::{
    calendar, dominant_art_color, greeting_text, size_for_state, step, Event, FocusTimer, Mode,
    Phase, Stage, Tab,
};
use np_core::qr;
use np_core::settings::{
    surface_spec, AiProvider, IslandEdge, IslandShape, Lang, OpenTarget, PowerMode, SurfaceTheme,
    VisStyle,
};
use np_core::symbols::Sym;
use np_core::widgets::{format_speed, SystemMetrics, WidgetKind};
use np_platform::media::{self, ArtUpdate, Command, Media};
use np_platform::{
    audio, brightness, capture, clipboard, monitors, sendsrv, startup, sys, taskbar, time,
    weather, wifi_bt,
};
use np_ui::gfx::Gfx;
use np_ui::glass::GlassPanel;
use np_ui::host::{attach, Surface};
use np_ui::painter::{rgba, Anchor, Painter, WHITE};
use std::cell::RefCell;
use std::time::Instant;
use windows::core::w;
use windows::UI::Color;
use windows::UI::Composition::{CompositionDrawingSurface, Compositor, ContainerVisual};
use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows::Win32::Graphics::Gdi::{ClientToScreen, CreateRectRgn, SetWindowRgn};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    ReleaseCapture, SetCapture, TrackMouseEvent, TME_LEAVE, TRACKMOUSEEVENT,
};
use windows::Win32::UI::Shell::{DragAcceptFiles, DragFinish, DragQueryFileW, HDROP};
use windows::Win32::UI::WindowsAndMessaging::*;
use windows_numerics::{Vector2, Vector3};

const WM_MOUSELEAVE: u32 = 0x02A3;
const WM_MEDIA: u32 = WM_APP + 10;
const WM_AUDIO: u32 = WM_APP + 11;
const WM_SEND: u32 = WM_APP + 12;
const WM_WEATHER: u32 = WM_APP + 13;
const WM_RECORD: u32 = WM_APP + 14;

const T_DWELL: usize = 1;
const T_LEAVE: usize = 2;
const T_ANIM: usize = 3;
const T_SYS: usize = 4;

const WIN_W: f32 = 860.0;
const WIN_H: f32 = 380.0;

#[derive(Clone, Copy, Debug)]
enum Action {
    SelectTab(Tab),
    Settings,
    PlayPause,
    Next,
    Prev,
    Seek(f32, f32),
    TimerToggle,
    TimerReset,
    TimerDelta(i32),
    PinWidget(WidgetKind),
    BrightnessBar(f32, f32),
    VolumeBar(f32, f32),
    ToggleWifi,
    ToggleBt,
    ToggleNight,
    ToggleMicMute,
    ToggleVolMute,
    PowerMode(PowerMode),
    Screenshot,
    Snip,
    RecordToggle,
    PickColor,
    SendShelfQr,
    ToolSlot(usize),
    ClearShelf,
    OpenShelfItem(usize),
    ClipCopy(usize),
    ClipPin(usize),
    ClipClear,
    AddClipboardNote,
    CopyNote(usize),
    DeleteNote(usize),
    SelectDefaultAi(AiProvider),
    LaunchAi(AiProvider),
    AskAiWithClipboard,
    ToggleCaptureShield,
    CloseIsland,
}

struct Region {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    action: Action,
    drag_widget: Option<WidgetKind>,
}

impl Region {
    fn hit(&self, x: f32, y: f32) -> bool {
        x >= self.x && x < self.x + self.w && y >= self.y && y < self.y + self.h
    }
}

enum DragOp {
    Seek(f32, f32),
    Brightness(f32, f32),
    Volume(f32, f32),
    WidgetCandidate(WidgetKind, i32, i32, bool),
}

struct HudFlash {
    text: String,
    frac: Option<f32>,
    rgb: [u8; 3],
    at: Instant,
}

struct Island {
    hwnd: HWND,
    gfx: Gfx,
    _surface: Surface,
    panel: GlassPanel,
    content: ContainerVisual,
    painter: Painter,
    dpi: f32,
    win: (i32, i32),
    mon: monitors::Monitor,

    mode: Mode,
    active_tab: Tab,
    tracking: bool,
    cur_rect: (f32, f32, f32, f32),
    regions: Vec<Region>,
    drag: Option<DragOp>,

    t0: Instant,
    wave_phase: f32,
    media: Media,
    has_media: bool,
    title: String,
    artist: String,
    playing: bool,
    playback_rate: f64,
    pos_ms: i64,
    dur_ms: i64,
    pos_at: Instant,
    can_seek: bool,
    art: Option<CompositionDrawingSurface>,
    art_tint: Option<[u8; 3]>,
    _loopback: audio::Loopback,
    audio: audio::Audio,
    display: brightness::DisplayCtl,

    timer: FocusTimer,
    metrics: SystemMetrics,
    privacy: (bool, bool),
    fullscreen_game: bool,
    maximized_top: bool,
    power_mode: PowerMode,
    shelf: Vec<String>,
    send_srv: Option<sendsrv::Server>,
    send_qr: Option<CompositionDrawingSurface>,
    recorder: Option<capture::Recorder>,
    tool_status: String,
    picked_color: Option<[u8; 3]>,
    hud: Option<HudFlash>,
    last_vol: f32,
    last_br: u8,
    last_mic_muted: bool,
    glance_idx: usize,
    last_glance_at: Instant,
    demo_media: bool,
    open_settings_cb: fn(),
}

thread_local! {
    static ISLAND: RefCell<Option<Island>> = const { RefCell::new(None) };
}

fn with<R>(f: impl FnOnce(&mut Island) -> R) -> Option<R> {
    ISLAND.with(|c| c.try_borrow_mut().ok().and_then(|mut b| b.as_mut().map(f)))
}

pub fn toggle_open() {
    with(|i| {
        i.dispatch(Event::ToggleOpen);
    });
}

pub fn open_clipboard_tab() {
    with(|i| {
        i.active_tab = Tab::Clipboard;
        if !matches!(i.mode, Mode::Open(_)) {
            i.dispatch(Event::ToggleOpen);
        } else {
            i.dispatch(Event::SelectTab(Tab::Clipboard));
        }
    });
}

pub fn toggle_mic_mute() {
    with(|i| {
        let _ = i.audio.commands.send(audio::Cmd::ToggleMicMute);
    });
}

pub fn shutdown() {
    if let Some(mut i) = ISLAND.with(|c| c.borrow_mut().take()) {
        if let Some(rec) = i.recorder.take() {
            let _ = rec.stop();
        }
        let hwnd = i.hwnd;
        prefs::unlisten(hwnd);
        drop(i);
        unsafe {
            let _ = DestroyWindow(hwnd);
        }
    }
}

pub fn init(comp: &Compositor, demo_media: bool, open_settings_cb: fn()) -> windows_core::Result<()> {
    unsafe {
        let hi: HINSTANCE = GetModuleHandleW(None)?.into();
        let class = w!("LuminaIsland");
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
            w!("np-island"),
            WS_POPUP,
            0,
            0,
            600,
            260,
            None,
            None,
            Some(hi),
            None,
        )?;
        let mon = monitors::primary();
        let dpi = mon.scale();
        let win = ((WIN_W * dpi).ceil() as i32, (WIN_H * dpi).ceil() as i32);
        let surface = attach(comp, hwnd, (win.0 as f32, win.1 as f32))?;
        let panel = GlassPanel::new(comp, &surface.root, prefs::glass_params((180.0, 32.0), 16.0))?;
        let content = comp.CreateContainerVisual()?;
        surface.root.Children()?.InsertAtTop(&content)?;
        let gfx = Gfx::shared(comp)?;
        let hw = hwnd.0 as isize;
        let media = media::spawn(move || {
            let _ = PostMessageW(Some(HWND(hw as *mut _)), WM_MEDIA, WPARAM(0), LPARAM(0));
        });
        let audio_ctl = audio::spawn(move || {
            let _ = PostMessageW(Some(HWND(hw as *mut _)), WM_AUDIO, WPARAM(0), LPARAM(0));
        });
        let display_ctl = brightness::spawn(move || {
            let _ = PostMessageW(Some(HWND(hw as *mut _)), WM_AUDIO, WPARAM(0), LPARAM(0));
        });
        let loopback = audio::start_loopback();
        let s = prefs::get();
        weather::start_weather_poller(s.weather_lat, s.weather_lon, move || {
            let _ = PostMessageW(Some(HWND(hw as *mut _)), WM_WEATHER, WPARAM(0), LPARAM(0));
        });
        sys::start_telemetry_sampler();
        wifi_bt::poll_status();
        DragAcceptFiles(hwnd, true);

        let painter = Painter::new(comp.clone(), gfx.clone(), dpi);
        let aud = audio::latest();
        let mut island = Island {
            hwnd,
            gfx,
            _surface: surface,
            panel,
            content,
            painter,
            dpi,
            win,
            mon,
            mode: Mode::Rest,
            active_tab: Tab::Music,
            tracking: false,
            cur_rect: (0.0, 0.0, 180.0, 32.0),
            regions: Vec::new(),
            drag: None,
            t0: Instant::now(),
            wave_phase: 0.0,
            media,
            has_media: demo_media,
            title: if demo_media {
                "Static Bloom".to_string()
            } else {
                String::new()
            },
            artist: if demo_media {
                "Juno Vale".to_string()
            } else {
                String::new()
            },
            playing: demo_media,
            playback_rate: 1.0,
            pos_ms: if demo_media { 98_000 } else { 0 },
            dur_ms: if demo_media { 140_000 } else { 0 },
            pos_at: Instant::now(),
            can_seek: true,
            art: None,
            art_tint: demo_media.then_some([226, 54, 68]),
            _loopback: loopback,
            audio: audio_ctl,
            display: display_ctl,
            timer: FocusTimer::new(s.focus_mins, s.break_mins),
            metrics: SystemMetrics::default(),
            privacy: (false, false),
            fullscreen_game: false,
            maximized_top: false,
            power_mode: PowerMode::Balanced,
            shelf: Vec::new(),
            send_srv: None,
            send_qr: None,
            recorder: None,
            tool_status: String::new(),
            picked_color: None,
            hud: None,
            last_vol: aud.volume,
            last_br: brightness::current(),
            last_mic_muted: aud.mic_muted,
            glance_idx: 0,
            last_glance_at: Instant::now(),
            demo_media,
            open_settings_cb,
        };
        island.apply_geometry(false);
        island.render();
        ISLAND.with(|c| *c.borrow_mut() = Some(island));
        prefs::listen(hwnd);
        SetTimer(Some(hwnd), T_ANIM, 70, None);
        SetTimer(Some(hwnd), T_SYS, 1000, None);
    }
    Ok(())
}

impl Island {
    fn now_ms(&self) -> i64 {
        self.t0.elapsed().as_millis() as i64
    }

    fn effective_vi(&self) -> bool {
        match prefs::get().lang {
            Lang::Vi => true,
            Lang::En => false,
            Lang::System => startup::system_is_vietnamese(),
        }
    }

    fn current_greeting(&self) -> String {
        let s = prefs::get();
        let vi = self.effective_vi();
        let hour = time::local_now().hour;
        let name = if s.greet_name.trim().is_empty() {
            std::env::var("USERNAME").unwrap_or_default()
        } else {
            s.greet_name
        };
        greeting_text(&s.greet_text, &name, hour, vi)
    }

    fn flash_hud(&mut self, text: impl Into<String>, frac: Option<f32>, rgb: [u8; 3]) {
        self.hud = Some(HudFlash {
            text: text.into(),
            frac,
            rgb,
            at: Instant::now(),
        });
    }

    fn active_hud(&self) -> Option<&HudFlash> {
        self.hud
            .as_ref()
            .filter(|h| h.at.elapsed().as_secs_f32() < 2.2)
    }

    fn dispatch(&mut self, ev: Event) {
        let s = prefs::get();
        let default_tab = match s.open_to {
            OpenTarget::LastTab => self.active_tab,
            OpenTarget::Music => Tab::Music,
            OpenTarget::Widgets => Tab::Widgets,
        };
        let target_tab = if matches!(self.mode, Mode::Rest | Mode::Peek) {
            default_tab
        } else {
            self.active_tab
        };
        let next = step(self.mode, ev, s.open_trigger, self.tracking, target_tab);
        if let Mode::Open(t) = next {
            self.active_tab = t;
        }
        if next != self.mode {
            self.mode = next;
            self.apply_geometry(true);
            self.render();
        }
    }

    fn top_gap(&self) -> f32 {
        let s = prefs::get();
        if s.shape == IslandShape::Pill {
            s.island_top_gap as f32 * self.dpi
        } else {
            0.0
        }
    }

    fn apply_geometry(&mut self, animate: bool) {
        let s = prefs::get();
        capture::set_capture_excluded(
            self.hwnd,
            s.stream_privacy_shield || s.surface == SurfaceTheme::PureBlack,
        );
        if !s.island_enabled {
            unsafe {
                let _ = ShowWindow(self.hwnd, SW_HIDE);
            }
            return;
        }
        if s.hide_under_fullscreen && self.fullscreen_game && !s.game_strip && self.mode == Mode::Rest {
            unsafe {
                let _ = ShowWindow(self.hwnd, SW_HIDE);
            }
            return;
        }
        self.mon = monitors::primary();
        self.dpi = self.mon.scale();
        let win = ((WIN_W * self.dpi).ceil() as i32, (WIN_H * self.dpi).ceil() as i32);
        self.win = win;
        let (wx, wy) = match s.edge {
            IslandEdge::Top => (self.mon.area.x + (self.mon.area.w - win.0) / 2, self.mon.area.y),
            IslandEdge::Left => (self.mon.area.x, self.mon.area.y + (self.mon.area.h - win.1) / 2),
            IslandEdge::Right => (
                self.mon.area.right() - win.0,
                self.mon.area.y + (self.mon.area.h - win.1) / 2,
            ),
        };
        unsafe {
            let _ = SetWindowPos(
                self.hwnd,
                Some(HWND_TOPMOST),
                wx,
                wy,
                win.0,
                win.1,
                SWP_NOACTIVATE | SWP_SHOWWINDOW,
            );
        }

        let sz = size_for_state(
            self.mode,
            self.has_media || self.active_hud().is_some(),
            s.game_strip,
            self.fullscreen_game,
            self.maximized_top,
            self.recorder.is_some(),
            &s,
            self.dpi,
        );
        let gap = self.top_gap();
        let (x0, y0) = match s.edge {
            IslandEdge::Top => (((self.win.0 as f32 - sz.w) / 2.0).floor(), gap),
            IslandEdge::Left => (gap, ((self.win.1 as f32 - sz.h) / 2.0).floor()),
            IslandEdge::Right => (
                (self.win.0 as f32 - sz.w - gap).floor(),
                ((self.win.1 as f32 - sz.h) / 2.0).floor(),
            ),
        };
        self.cur_rect = (x0, y0, sz.w, sz.h);

        let mut gp = prefs::glass_params((sz.w, sz.h), sz.radius);
        if s.surface.is_see_through() {
            let k = s.see_through_pct.clamp(25, 100) as f32 / 100.0;
            gp.blur_sigma *= k;
            gp.highlight *= k;
        }
        if s.high_contrast {
            gp.rim.A = gp.rim.A.max(230);
        }
        let glow_rgb = if s.island_glow && self.has_media {
            self.art_tint
        } else if self.recorder.is_some() {
            Some([245, 72, 82])
        } else {
            None
        };
        if let Some(t) = glow_rgb {
            gp.rim = Color {
                A: gp.rim.A.max(180),
                R: t[0],
                G: t[1],
                B: t[2],
            };
        }
        let _ = self.panel.set_notch_flush(
            s.edge == IslandEdge::Top && s.shape == IslandShape::Notch && !self.maximized_top,
        );
        let _ = self.panel.set_ambient(glow_rgb);
        let _ = self.panel.set_params(gp, prefs::auto_tier());
        let ms_scale = s.motion.period_scale();
        if animate && ms_scale > 0.0 {
            let period = (64.0 * ms_scale).round() as u32;
            let _ = self.panel.animate_rect(x0, y0, sz.w, sz.h, sz.radius, 0.80, period);
        } else {
            let _ = self.panel.set_rect(x0, y0, sz.w, sz.h);
            let _ = self.panel.set_radius(sz.radius);
        }

        let _ = self.content.SetOffset(Vector3 { X: x0, Y: y0, Z: 0.0 });
        let _ = self.content.SetSize(Vector2 { X: sz.w, Y: sz.h });
        unsafe {
            let pad = (6.0 * self.dpi).ceil() as i32;
            let rgn = CreateRectRgn(
                (x0 as i32 - pad).max(0),
                (y0 as i32 - pad).max(0),
                (x0 + sz.w).ceil() as i32 + pad,
                (y0 + sz.h).ceil() as i32 + pad,
            );
            SetWindowRgn(self.hwnd, Some(rgn), true);
        }
    }

    fn effective_pos_ms(&self) -> i64 {
        if self.playing && !matches!(self.drag, Some(DragOp::Seek(..))) {
            let delta = (self.pos_at.elapsed().as_secs_f64() * 1000.0 * self.playback_rate) as i64;
            self.pos_ms.saturating_add(delta).clamp(0, self.dur_ms)
        } else {
            self.pos_ms
        }
    }

    fn visible_tabs(&self) -> Vec<(Tab, Sym)> {
        let s = prefs::get();
        let mut out = vec![(Tab::Music, Sym::Music), (Tab::Widgets, Sym::Widgets), (Tab::Panels, Sym::Document)];
        if s.clipboard_history {
            out.push((Tab::Clipboard, Sym::Clipboard));
        }
        out.push((Tab::Tools, Sym::Tools));
        if s.show_ai_tab {
            out.push((Tab::Ai, Sym::Sparkles));
        }
        out.push((Tab::Quick, Sym::Sliders));
        if s.show_calls_tab {
            out.push((Tab::Calls, Sym::Mic));
        }
        out
    }

    fn render(&mut self) {
        let s = prefs::get();
        if !s.island_enabled {
            return;
        }
        if let Ok(kids) = self.content.Children() {
            let _ = kids.RemoveAll();
        }
        self.regions.clear();
        self.painter.reset(self.dpi * s.text_scale());

        let d = self.dpi;
        let (_, _, w, h) = self.cur_rect;
        let fg = surface_spec(s.surface).fg;
        let accent = self.art_tint.unwrap_or(s.accent);
        let side = matches!(s.edge, IslandEdge::Left | IslandEdge::Right);

        match self.mode {
            Mode::Rest | Mode::Peek => {
                if self.mode == Mode::Rest && self.maximized_top && s.glare_strip && self.recorder.is_none() && s.edge == IslandEdge::Top {
                    self.painter.grad_rect(&self.content, 8.0 * d, 1.0, (w - 16.0 * d).max(20.0), 3.0, 1.5, rgba(WHITE, 145), rgba(accent, 115), true);
                    return;
                }
                if side {
                    if let Some(ref rec) = self.recorder {
                        let sec = rec.elapsed_secs();
                        self.painter.rect(&self.content, (w - 8.0 * d) / 2.0, 14.0 * d, 8.0 * d, 8.0 * d, 4.0 * d, rgba([255, 76, 86], 245));
                        self.painter.pill_text(&self.content, &format!("{:02}:{:02}", sec / 60, sec % 60), w / 2.0, 34.0 * d, 8.5, 700, WHITE, 0.95, w);
                    }
                    if self.has_media {
                        self.painter.sym(&self.content, Sym::Music, w / 2.0, h / 2.0 - 24.0 * d, 13.0, accent, 0.96);
                        let spec = audio::spectrum16();
                        for idx in 0..5 {
                            let e = if self.playing {
                                spec[(idx * 3) % 16].max(0.22 + 0.55 * (self.wave_phase + idx as f32).sin().abs())
                            } else {
                                0.18
                            };
                            let bw = (4.0 * d + e * 14.0 * d).min(w - 8.0 * d);
                            self.painter.rect(&self.content, (w - bw) / 2.0, h / 2.0 + idx as f32 * 6.5 * d, bw, 3.0 * d, 1.5 * d, rgba(accent, 235));
                        }
                    } else {
                        self.painter.card(&self.content, (w - 5.0 * d) / 2.0, (h - 30.0 * d) / 2.0, 5.0 * d, 30.0 * d, 2.5 * d, fg, 26, 48);
                    }
                    return;
                }

                if let Some(ref rec) = self.recorder {
                    let sec = rec.elapsed_secs();
                    self.painter.rect(&self.content, 12.0 * d, h / 2.0 - 4.5 * d, 9.0 * d, 9.0 * d, 4.5 * d, rgba([255, 72, 82], 250));
                    self.painter.text(&self.content, &format!("REC {:02}:{:02}", sec / 60, sec % 60), 26.0 * d, (h - 14.0 * d) / 2.0, 10.5, 700, [255, 118, 124], 0.98, Anchor::Left, 95.0 * d);
                    self.painter.card(&self.content, w - 54.0 * d, (h - 20.0 * d) / 2.0, 44.0 * d, 20.0 * d, 10.0 * d, [255, 72, 82], 145, 85);
                    self.painter.pill_text(&self.content, prefs::tr("Stop", "Dừng"), w - 32.0 * d, h / 2.0, 9.5, 700, WHITE, 0.98, 40.0 * d);
                    self.regions.push(Region {
                        x: w - 58.0 * d,
                        y: 0.0,
                        w: 58.0 * d,
                        h,
                        action: Action::RecordToggle,
                        drag_widget: None,
                    });
                    return;
                }

                if let Some(flash) = self.hud.as_ref().filter(|h| h.at.elapsed().as_secs_f32() < 2.2) {
                    self.painter.pill_text(&self.content, &flash.text, if flash.frac.is_some() { 54.0 * d } else { w / 2.0 }, h / 2.0, 10.2, 600, fg, 0.96, w - 24.0 * d);
                    if let Some(f) = flash.frac {
                        let bx = 94.0 * d;
                        let bw = (w - bx - 14.0 * d).max(40.0 * d);
                        self.painter.rect(&self.content, bx, h / 2.0 - 3.0 * d, bw, 6.0 * d, 3.0 * d, rgba(fg, 36));
                        self.painter.grad_rect(&self.content, bx, h / 2.0 - 3.0 * d, (bw * f.clamp(0.0, 1.0)).max(6.0 * d), 6.0 * d, 3.0 * d, rgba(flash.rgb, 245), rgba(WHITE, 230), true);
                    }
                    return;
                }

                if self.fullscreen_game && s.game_strip {
                    let fps = if s.fps_counter { "FPS 60  ·  " } else { "" };
                    let msg = format!(
                        "{fps}CPU {}%  ·  GPU {}%{}  ·  RAM {}%",
                        self.metrics.cpu_pct,
                        self.metrics.gpu_pct,
                        self.metrics
                            .gpu_temp_c
                            .map(|t| format!(" {t}°"))
                            .unwrap_or_default(),
                        self.metrics.ram_pct
                    );
                    self.painter.pill_text(&self.content, &msg, w / 2.0, h / 2.0, 10.2, 600, fg, 0.95, w - 16.0 * d);
                } else if self.has_media {
                    let peek = self.mode == Mode::Peek;
                    let art_sz = (h - 9.0 * d).max(18.0 * d);
                    let ax = 7.0 * d;
                    let ay = (h - art_sz) / 2.0;
                    if peek {
                        self.painter.soft_shadow(&self.content, ax, ay, art_sz, art_sz, 6.0 * d, 4.0 * d, accent, 95);
                    }
                    if let Some(a) = &self.art {
                        self.painter.image(&self.content, a, ax, ay, art_sz, art_sz, 6.0 * d);
                    } else {
                        self.painter.grad_rect(
                            &self.content,
                            ax,
                            ay,
                            art_sz,
                            art_sz,
                            6.0 * d,
                            rgba(accent, 235),
                            rgba([accent[0] / 2, accent[1] / 2, accent[2] / 2], 235),
                            false,
                        );
                        self.painter.sym(&self.content, Sym::Music, ax + art_sz / 2.0, h / 2.0, 11.5, WHITE, 0.96);
                    }
                    self.painter.stroke_rect(&self.content, ax, ay, art_sz, art_sz, 6.0 * d, 1.0, rgba(WHITE, 60), rgba(WHITE, 18));

                    let spec = audio::spectrum16();
                    let bars = 6usize;
                    let bw = 2.6 * d;
                    let bgap = 2.2 * d;
                    let eq_w = bars as f32 * bw + (bars - 1) as f32 * bgap;
                    let rx = w - 11.0 * d - eq_w;
                    let text_w = w - art_sz - eq_w - 30.0 * d;
                    let mid_text = self.painter.fit(&self.title, if peek { 11.0 } else { 10.5 }, 600, text_w);
                    self.painter.pill_text(&self.content, &mid_text, w / 2.0 - 2.0 * d, h / 2.0, if peek { 11.0 } else { 10.5 }, 600, fg, 0.95, text_w);

                    for i in 0..bars {
                        let e = if self.playing {
                            let v = spec[(i * 2) % 16];
                            if v > 0.03 {
                                v
                            } else {
                                0.25 + 0.62 * ((self.wave_phase + i as f32 * 0.95).sin().abs())
                            }
                        } else {
                            0.18
                        };
                        let bx = rx + i as f32 * (bw + bgap);
                        match s.visualizer {
                            VisStyle::Dots => {
                                let rad = (bw * (0.75 + e * 0.7)).min(h - 12.0 * d);
                                let by = h / 2.0 - rad / 2.0 + ((self.wave_phase + i as f32 * 0.7).sin() * e * 3.5 * d);
                                self.painter.rect(&self.content, bx, by.clamp(4.0 * d, h - rad - 4.0 * d), rad, rad, rad / 2.0, rgba(accent, 235));
                            }
                            VisStyle::Wave => {
                                let wave = (self.wave_phase + i as f32 * 0.85).sin();
                                let cy = h / 2.0 + wave * e * 6.0 * d;
                                let bh = (4.5 * d + e * 5.0 * d).min(h - 10.0 * d);
                                self.painter.rect(&self.content, bx, (cy - bh / 2.0).clamp(4.0 * d, h - bh - 4.0 * d), bw, bh, bw / 2.0, rgba(accent, 240));
                            }
                            VisStyle::Bars => {
                                let bh = (4.0 * d + e * (h - 12.0 * d)).min(h - 10.0 * d);
                                let by = (h - bh) / 2.0;
                                self.painter.grad_rect(&self.content, bx, by, bw, bh, bw / 2.0, rgba(WHITE, 235), rgba(accent, 245), false);
                            }
                        }
                    }
                } else if self.mode == Mode::Peek {
                    let n = time::local_now();
                    let clk = if s.clock_24h {
                        format!("{:02}:{:02}", n.hour, n.minute)
                    } else {
                        let h12 = if n.hour % 12 == 0 { 12 } else { n.hour % 12 };
                        format!("{h12}:{:02}", n.minute)
                    };
                    self.painter.sym(&self.content, Sym::Sparkles, 18.0 * d, h / 2.0, 12.5, surface_spec(s.surface).glow, 0.92);
                    if s.show_greeting {
                        let greet = self.current_greeting();
                        let right_pad = if s.show_stream_badge && (self.privacy.0 || self.privacy.1) { 30.0 * d } else { 14.0 * d };
                        let clk_w = self.painter.measure(&clk, 10.5, 600);
                        let left = 30.0 * d;
                        let max_g = (w - left - right_pad - clk_w - 14.0 * d).max(40.0 * d);
                        let g_shown = self.painter.fit(&greet, 10.5, 600, max_g);
                        let g_w = self.painter.measure(&g_shown, 10.5, 600);
                        self.painter.pill_text(&self.content, &g_shown, left + g_w / 2.0, h / 2.0, 10.5, 600, fg, 0.95, g_w + 4.0 * d);
                        self.painter.pill_text(&self.content, &clk, w - right_pad - clk_w / 2.0, h / 2.0, 10.5, 600, fg, 0.72, clk_w + 4.0 * d);
                    } else {
                        self.painter.pill_text(&self.content, &clk, w / 2.0 + 4.0 * d, h / 2.0, 11.0, 600, fg, 0.92, w - 40.0 * d);
                    }
                } else if s.show_greeting {
                    let slot = if s.greeting_rotate { self.glance_idx % 3 } else { 0 };
                    let vi = self.effective_vi();
                    let (sym, msg, dot_col) = match slot {
                        1 => {
                            let n = time::local_now();
                            let wd = calendar::weekday(n.year, n.month, n.day) as usize;
                            let clk = if s.clock_24h {
                                format!("{:02}:{:02}", n.hour, n.minute)
                            } else {
                                let h12 = if n.hour % 12 == 0 { 12 } else { n.hour % 12 };
                                format!("{h12}:{:02}", n.minute)
                            };
                            let date = if vi {
                                let days = ["CN", "T2", "T3", "T4", "T5", "T6", "T7"];
                                format!("{clk}  ·  {}, {}/{}", days[wd % 7], n.day, n.month)
                            } else {
                                let day = &calendar::weekday_upper(wd as u32)[..3];
                                let mon = &calendar::month_upper(n.month)[..3];
                                format!("{clk}  ·  {day} {} {mon}", n.day)
                            };
                            (Some(Sym::Clock), date, [120, 192, 255])
                        }
                        2 => {
                            let txt = match sys::battery() {
                                Some((pct, ch)) => format!(
                                    "{}{}%  ·  CPU {}%",
                                    if ch { "+" } else { "" },
                                    pct,
                                    self.metrics.cpu_pct
                                ),
                                None => format!("CPU {}%  ·  RAM {}%", self.metrics.cpu_pct, self.metrics.ram_pct),
                            };
                            (Some(Sym::Bolt), txt, [72, 224, 134])
                        }
                        _ => (None, self.current_greeting(), surface_spec(s.surface).glow),
                    };
                    let right_pad = if s.show_stream_badge && (self.privacy.0 || self.privacy.1) { 28.0 * d } else { 12.0 * d };
                    if let Some(ic) = sym {
                        self.painter.sym(&self.content, ic, 16.0 * d, h / 2.0, 11.5, dot_col, 0.94);
                    } else {
                        self.painter.rect(&self.content, 11.5 * d, (h - 7.0 * d) / 2.0, 7.0 * d, 7.0 * d, 3.5 * d, rgba(dot_col, 240));
                    }
                    let text_left = 25.0 * d;
                    let avail = (w - text_left - right_pad).max(60.0 * d);
                    let shown = self.painter.fit(&msg, 10.4, 600, avail);
                    self.painter.pill_text(&self.content, &shown, text_left + avail / 2.0, h / 2.0, 10.4, 600, fg, 0.95, avail);
                } else {
                    let cw = 26.0 * d;
                    let ch = 6.0 * d;
                    self.painter.card(&self.content, (w - cw) / 2.0, (h - ch) / 2.0, cw, ch, ch / 2.0, fg, 22, 42);
                    let lens = 8.0 * d;
                    self.painter.rect(&self.content, w - 20.0 * d, (h - lens) / 2.0, lens, lens, lens / 2.0, rgba([26, 34, 54], 210));
                    self.painter.stroke_rect(&self.content, w - 20.0 * d, (h - lens) / 2.0, lens, lens, lens / 2.0, 1.0, rgba([110, 175, 255], 110), rgba([110, 175, 255], 40));
                }
                if s.show_stream_badge && (self.privacy.0 || self.privacy.1) {
                    if self.privacy.0 {
                        self.painter.rect(&self.content, w - 13.0 * d, h / 2.0 - 3.5 * d, 7.0 * d, 7.0 * d, 3.5 * d, rgba([255, 159, 10], 245));
                    }
                    if self.privacy.1 {
                        self.painter.rect(&self.content, w - 23.0 * d, h / 2.0 - 3.5 * d, 7.0 * d, 7.0 * d, 3.5 * d, rgba([48, 209, 88], 245));
                    }
                }
            }
            Mode::Drop => {
                let pad = 16.0 * d;
                let bw = (w - 3.0 * pad) / 2.0;
                let bh = h - 2.0 * pad;
                for (k, (sym, title, fill_rgb, fill_a, border_a)) in [
                    (Sym::Folder, prefs::tr("Keep on Shelf", "Lưu vào Ngăn tệp"), fg, 28u8, 45u8),
                    (Sym::Qr, prefs::tr("Send to Phone (QR)", "Gửi qua Điện thoại (QR)"), accent, 128u8, 95u8),
                ]
                .into_iter()
                .enumerate()
                {
                    let bx = pad + k as f32 * (bw + pad);
                    self.painter.card(&self.content, bx, pad, bw, bh, 18.0 * d, fill_rgb, fill_a, border_a);
                    self.painter.rect(&self.content, bx + bw / 2.0 - 18.0 * d, pad + bh / 2.0 - 30.0 * d, 36.0 * d, 36.0 * d, 18.0 * d, rgba(WHITE, 32));
                    self.painter.sym(&self.content, sym, bx + bw / 2.0, pad + bh / 2.0 - 12.0 * d, 18.0, fg, 0.98);
                    self.painter.pill_text(&self.content, title, bx + bw / 2.0, pad + bh / 2.0 + 21.0 * d, 12.0, 600, fg, 0.96, bw - 14.0 * d);
                }
            }
            Mode::Open(tab) => {
                self.draw_tab_bar(w, d, tab, fg, accent);
                let top = 44.0 * d;
                let pad = 16.0 * d;
                let inner_h = h - top - pad;
                let inner_w = w - 2.0 * pad;
                match tab {
                    Tab::Music => self.draw_music_tab(pad, top, inner_w, inner_h, fg, accent),
                    Tab::Widgets => self.draw_widgets_tab(pad, top, inner_w, inner_h, fg),
                    Tab::Panels => self.draw_panels_tab(pad, top, inner_w, inner_h, fg, accent),
                    Tab::Clipboard => self.draw_clipboard_tab(pad, top, inner_w, inner_h, fg, accent),
                    Tab::Tools => self.draw_tools_tab(pad, top, inner_w, inner_h, fg, accent),
                    Tab::Ai => self.draw_ai_tab(pad, top, inner_w, inner_h, fg, accent),
                    Tab::Quick => self.draw_quick_tab(pad, top, inner_w, inner_h, fg, accent),
                    Tab::Calls => self.draw_calls_tab(pad, top, inner_w, inner_h, fg, accent),
                }
            }
        }
    }

    fn draw_tab_bar(&mut self, w: f32, d: f32, active: Tab, fg: [u8; 3], accent: [u8; 3]) {
        let vi = prefs::is_vi();
        let tabs = self.visible_tabs();
        let active_w = 82.0 * d;
        let icon_w = 31.0 * d;
        let total_w: f32 = tabs.iter().map(|(t, _)| if *t == active { active_w } else { icon_w }).sum::<f32>() + 12.0 * d;
        let mut x = (w - total_w - 28.0 * d).max(12.0 * d) / 2.0 + 4.0 * d;
        let bar_y = 8.0 * d;
        let bar_h = 28.0 * d;
        self.painter.card(&self.content, x, bar_y, total_w, bar_h, bar_h / 2.0, fg, 14, 34);
        x += 6.0 * d;
        let cy = bar_y + bar_h / 2.0;
        for (t, sym) in tabs {
            let on = t == active;
            let tw = if on { active_w } else { icon_w };
            if on {
                self.painter.card(&self.content, x, bar_y + 2.5 * d, tw, bar_h - 5.0 * d, (bar_h - 5.0 * d) / 2.0, accent, 108, 95);
                self.painter.sym(&self.content, sym, x + 15.0 * d, cy, 12.0, WHITE, 1.0);
                self.painter.text(&self.content, t.label_i18n(vi), x + 26.0 * d, bar_y + 6.5 * d, 10.0, 700, WHITE, 0.98, Anchor::Left, tw - 30.0 * d);
            } else {
                self.painter.sym(&self.content, sym, x + tw / 2.0, cy, 12.5, fg, 0.58);
            }
            self.regions.push(Region {
                x,
                y: bar_y - 2.0 * d,
                w: tw,
                h: bar_h + 4.0 * d,
                action: Action::SelectTab(t),
                drag_widget: None,
            });
            x += tw;
        }

        // Settings gear & Close (for sticky accessibility)
        let gx = w - 24.0 * d;
        self.painter.card(&self.content, gx - 13.0 * d, cy - 13.0 * d, 26.0 * d, 26.0 * d, 13.0 * d, fg, 18, 36);
        self.painter.sym(&self.content, Sym::Gear, gx, cy, 12.0, fg, 0.85);
        self.regions.push(Region {
            x: gx - 15.0 * d,
            y: cy - 15.0 * d,
            w: 30.0 * d,
            h: 30.0 * d,
            action: Action::Settings,
            drag_widget: None,
        });
        if prefs::get().sticky_open {
            let kx = gx - 31.0 * d;
            self.painter.card(&self.content, kx - 12.0 * d, cy - 12.0 * d, 24.0 * d, 24.0 * d, 12.0 * d, fg, 16, 30);
            self.painter.sym(&self.content, Sym::Close, kx, cy, 9.5, fg, 0.80);
            self.regions.push(Region {
                x: kx - 13.0 * d,
                y: cy - 13.0 * d,
                w: 26.0 * d,
                h: 26.0 * d,
                action: Action::CloseIsland,
                drag_widget: None,
            });
        }
    }

    fn draw_music_tab(&mut self, x: f32, y: f32, w: f32, h: f32, fg: [u8; 3], accent: [u8; 3]) {
        let d = self.dpi;
        self.painter.card(&self.content, x, y, w, h, 18.0 * d, accent, 24, 38);
        self.painter.grad_rect(&self.content, x, y, w, h, 18.0 * d, rgba(accent, 42), rgba(accent, 4), true);
        if !self.has_media {
            self.painter.card(&self.content, x + w / 2.0 - 24.0 * d, y + h / 2.0 - 44.0 * d, 48.0 * d, 48.0 * d, 14.0 * d, fg, 22, 40);
            self.painter.sym(&self.content, Sym::Music, x + w / 2.0, y + h / 2.0 - 20.0 * d, 22.0, accent, 0.85);
            let s = prefs::get();
            if s.show_greeting {
                let greet = self.current_greeting();
                let sub = if self.effective_vi() {
                    "Chưa phát nhạc  ·  Kéo thả tệp vào đây"
                } else {
                    "No media playing  ·  Drop files to share"
                };
                self.painter.pill_text(&self.content, &greet, x + w / 2.0, y + h / 2.0 + 20.0 * d, 14.0, 700, fg, 0.94, w - 32.0 * d);
                self.painter.pill_text(&self.content, sub, x + w / 2.0, y + h / 2.0 + 42.0 * d, 10.8, 500, fg, 0.62, w - 32.0 * d);
            } else {
                self.painter.pill_text(&self.content, "No media playing", x + w / 2.0, y + h / 2.0 + 26.0 * d, 12.5, 600, fg, 0.72, w);
            }
            return;
        }
        let art = 70.0 * d;
        let ax = x + 14.0 * d;
        let ay = y + 14.0 * d;
        self.painter.soft_shadow(&self.content, ax, ay, art, art, 14.0 * d, 10.0 * d, accent, 128);
        if let Some(a) = &self.art {
            self.painter.image(&self.content, a, ax, ay, art, art, 14.0 * d);
        } else {
            self.painter.grad_rect(
                &self.content,
                ax,
                ay,
                art,
                art,
                14.0 * d,
                rgba(accent, 235),
                rgba([accent[0] / 2, accent[1] / 2, accent[2] / 2], 240),
                false,
            );
            self.painter.sym(&self.content, Sym::Music, ax + art / 2.0, ay + art / 2.0, 26.0, WHITE, 0.96);
        }
        self.painter.stroke_rect(&self.content, ax, ay, art, art, 14.0 * d, 1.1, rgba(WHITE, 82), rgba(WHITE, 20));

        let tx = ax + art + 15.0 * d;
        let tw = w - (tx - x) - 134.0 * d;
        let title = self.painter.fit(&self.title, 15.0, 700, tw);
        let artist = self.painter.fit(&self.artist, 12.0, 500, tw);
        self.painter.text(&self.content, &title, tx, ay + 11.0 * d, 15.0, 700, fg, 1.0, Anchor::Left, tw);
        self.painter.text(&self.content, &artist, tx, ay + 34.0 * d, 12.0, 500, fg, 0.72, Anchor::Left, tw);

        let cy_btn = ay + 35.0 * d;
        let bx_mid = x + w - 68.0 * d;
        for (sym, cx, sz, btn_r, is_primary, act) in [
            (Sym::Prev, bx_mid - 40.0 * d, 13.5, 14.0 * d, false, Action::Prev),
            (if self.playing { Sym::Pause } else { Sym::Play }, bx_mid, 16.5, 19.0 * d, true, Action::PlayPause),
            (Sym::Next, bx_mid + 40.0 * d, 13.5, 14.0 * d, false, Action::Next),
        ] {
            if is_primary {
                self.painter.soft_shadow(&self.content, cx - btn_r, cy_btn - btn_r, btn_r * 2.0, btn_r * 2.0, btn_r, 6.0 * d, accent, 110);
                self.painter.card(&self.content, cx - btn_r, cy_btn - btn_r, btn_r * 2.0, btn_r * 2.0, btn_r, accent, 155, 120);
            } else {
                self.painter.card(&self.content, cx - btn_r, cy_btn - btn_r, btn_r * 2.0, btn_r * 2.0, btn_r, fg, 20, 32);
            }
            self.painter.sym(&self.content, sym, cx, cy_btn, sz, WHITE, 0.98);
            self.regions.push(Region {
                x: cx - 18.0 * d,
                y: cy_btn - 18.0 * d,
                w: 36.0 * d,
                h: 36.0 * d,
                action: act,
                drag_widget: None,
            });
        }

        let pos = self.effective_pos_ms();
        let frac = if self.dur_ms > 0 {
            (pos as f32 / self.dur_ms as f32).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let sy = y + h - 25.0 * d;
        let mmss = |ms: i64| {
            let sec = ms.max(0) / 1000;
            format!("{}:{:02}", sec / 60, sec % 60)
        };
        let elapsed = mmss(pos);
        let remaining = format!("-{}", mmss(self.dur_ms - pos));
        let left = self.painter.measure(&elapsed, 10.0, 600);
        let right = self.painter.measure(&remaining, 10.0, 600);
        let sx = x + 22.0 * d + left;
        let sw = (w - 44.0 * d - left - right).max(1.0);
        self.painter.text(&self.content, &elapsed, x + 14.0 * d, sy - 7.0 * d, 10.0, 600, fg, 0.70, Anchor::Left, left + 2.0 * d);
        self.painter.text(
            &self.content,
            &remaining,
            x + w - 14.0 * d,
            sy - 7.0 * d,
            10.0,
            600,
            fg,
            0.70,
            Anchor::Right,
            right + 2.0 * d,
        );
        let fill = sw * frac;
        let r = 1.5 * d;
        self.painter.rect(&self.content, sx, sy - r, sw, 2.0 * r, r, rgba(fg, 72));
        if fill > 0.0 {
            self.painter.rect(&self.content, sx, sy - r, fill, 2.0 * r, r.min(fill / 2.0), rgba(accent, 255));
        }
        if self.dur_ms > 0 {
            self.painter.rect(&self.content, sx + fill - 2.0 * d, sy - 6.0 * d, 4.0 * d, 12.0 * d, 2.0 * d, rgba(WHITE, 255));
        }
        if self.can_seek && self.dur_ms > 0 {
            self.regions.push(Region {
                x: sx - 4.0 * d,
                y: sy - 12.0 * d,
                w: sw + 8.0 * d,
                h: 24.0 * d,
                action: Action::Seek(sx, sw),
                drag_widget: None,
            });
        }
    }

    fn draw_widgets_tab(&mut self, x: f32, y: f32, w: f32, h: f32, fg: [u8; 3]) {
        let vi = prefs::is_vi();
        let d = self.dpi;
        let gap = 10.0 * d;
        let cw = (w - gap) / 2.0;
        let ch = (h - gap) / 2.0;

        // 1. Calendar (top-left)
        let (c_x, c_y) = (x, y);
        self.painter.card(&self.content, c_x, c_y, cw, ch, 15.0 * d, fg, 18, 36);
        let n = time::local_now();
        let wd = calendar::weekday(n.year, n.month, n.day);
        let wk = calendar::iso_week(n.year, n.month, n.day);
        let wd_lbl = if vi {
            ["CN", "T2", "T3", "T4", "T5", "T6", "T7"][(wd as usize) % 7]
        } else {
            calendar::weekday_short(wd)
        };
        let mo_lbl = if vi {
            format!("Th{}", n.month)
        } else {
            calendar::month_short(n.month).to_string()
        };
        self.painter.text(&self.content, &format!("{:02}", n.day), c_x + 12.0 * d, c_y + 7.0 * d, 18.5, 700, fg, 1.0, Anchor::Left, 36.0 * d);
        self.painter.text_shadow(&self.content, wd_lbl, c_x + 44.0 * d, c_y + 8.0 * d, 9.5, 700, [255, 114, 108], 1.0, Anchor::Left, 40.0 * d);
        self.painter.text(&self.content, &mo_lbl, c_x + 44.0 * d, c_y + 20.0 * d, 9.0, 500, fg, 0.76, Anchor::Left, 40.0 * d);
        let wk_x = c_x + cw - 66.0 * d;
        let wk_y = c_y + 9.0 * d;
        let wk_w = 54.0 * d;
        let wk_h = 17.0 * d;
        self.painter.card(&self.content, wk_x, wk_y, wk_w, wk_h, 8.5 * d, fg, 22, 26);
        let wk_txt = if vi { format!("Tuần {wk}") } else { format!("Wk {wk}") };
        self.painter.pill_text(&self.content, &wk_txt, wk_x + wk_w / 2.0, wk_y + wk_h / 2.0, 8.8, 600, fg, 0.84, wk_w - 4.0 * d);
        let gx = c_x + 12.0 * d;
        let gy = c_y + 36.0 * d;
        let cell_w = (cw - 24.0 * d) / 7.0;
        let first = calendar::weekday(n.year, n.month, 1);
        let dim = calendar::days_in_month(n.year, n.month);
        let start_row = (first + n.day - 1) / 7;
        let day_hdrs = if vi {
            ["CN", "T2", "T3", "T4", "T5", "T6", "T7"]
        } else {
            ["S", "M", "T", "W", "T", "F", "S"]
        };
        for col in 0..7u32 {
            let day_num = (start_row * 7 + col) as i32 - first as i32 + 1;
            let cx = gx + cell_w * (col as f32 + 0.5);
            self.painter.pill_text(&self.content, day_hdrs[col as usize], cx, gy + 5.5 * d, 8.2, 600, fg, 0.48, cell_w);
            if day_num >= 1 && day_num <= dim as i32 {
                let today = day_num as u32 == n.day;
                let cy_mid = gy + 21.0 * d;
                if today {
                    self.painter.grad_rect(&self.content, cx - 9.0 * d, cy_mid - 7.75 * d, 18.0 * d, 15.5 * d, 7.75 * d, rgba([255, 102, 96], 250), rgba([224, 52, 74], 250), false);
                }
                self.painter.pill_text(&self.content, &day_num.to_string(), cx, cy_mid, 9.0, if today { 700 } else { 500 }, fg, if today { 1.0 } else { 0.86 }, cell_w);
            }
        }
        self.regions.push(Region {
            x: c_x,
            y: c_y,
            w: cw,
            h: ch,
            action: Action::PinWidget(WidgetKind::Calendar),
            drag_widget: Some(WidgetKind::Calendar),
        });

        // 2. Focus timer (top-right)
        let (f_x, f_y) = (x + cw + gap, y);
        let ring_col = if self.timer.stage == Stage::Break { [72, 224, 146] } else { [255, 168, 62] };
        self.painter.card(&self.content, f_x, f_y, cw, ch, 15.0 * d, fg, 18, 36);
        self.painter.ring_progress(&self.content, f_x + 28.0 * d, f_y + ch / 2.0, 30.0 * d, 3.2 * d, self.timer.elapsed_frac(), fg, ring_col);
        self.painter.sym(&self.content, Sym::Clock, f_x + 28.0 * d, f_y + ch / 2.0, 12.0, ring_col, 0.92);
        self.painter.text(&self.content, &self.timer.mmss(), f_x + 50.0 * d, f_y + 12.0 * d, 19.0, 700, fg, 1.0, Anchor::Left, 82.0 * d);
        let st_lbl = match (self.timer.phase, self.timer.stage, vi) {
            (Phase::Running, Stage::Break, true) => "Đang nghỉ ngơi",
            (Phase::Running, Stage::Break, false) => "Break active",
            (Phase::Paused, _, true) => "Đã tạm dừng",
            (Phase::Paused, _, false) => "Paused",
            (_, _, true) => "Tập trung",
            (_, _, false) => "Focus timer",
        };
        self.painter.text(&self.content, st_lbl, f_x + 50.0 * d, f_y + 39.0 * d, 10.0, 600, ring_col, 0.90, Anchor::Left, 84.0 * d);
        let min_lbl = if vi {
            format!("{} + {} phút", self.timer.focus_secs / 60, self.timer.break_secs / 60)
        } else {
            format!("{} + {} min", self.timer.focus_secs / 60, self.timer.break_secs / 60)
        };
        self.painter.text(
            &self.content,
            &min_lbl,
            f_x + 50.0 * d,
            f_y + 54.0 * d,
            9.0,
            500,
            fg,
            0.55,
            Anchor::Left,
            84.0 * d,
        );
        let btn_y = f_y + ch / 2.0;
        for (sym, bx, primary, act) in [
            (Sym::Minus, f_x + cw - 76.0 * d, false, Action::TimerDelta(-5)),
            (if self.timer.phase == Phase::Running { Sym::Pause } else { Sym::Play }, f_x + cw - 47.0 * d, true, Action::TimerToggle),
            (Sym::Plus, f_x + cw - 18.0 * d, false, Action::TimerDelta(5)),
        ] {
            let br = if primary { 13.0 * d } else { 11.5 * d };
            self.painter.card(&self.content, bx - br, btn_y - br, br * 2.0, br * 2.0, br, if primary { ring_col } else { fg }, if primary { 130 } else { 24 }, if primary { 90 } else { 32 });
            self.painter.sym(&self.content, sym, bx, btn_y, 10.5, WHITE, 0.96);
            self.regions.push(Region {
                x: bx - 13.0 * d,
                y: btn_y - 13.0 * d,
                w: 26.0 * d,
                h: 26.0 * d,
                action: act,
                drag_widget: None,
            });
        }
        self.regions.push(Region {
            x: f_x,
            y: f_y,
            w: cw - 92.0 * d,
            h: ch,
            action: Action::TimerReset,
            drag_widget: Some(WidgetKind::FocusTimer),
        });

        // 3. Weather (bottom-left)
        let (w_x, w_y) = (x, y + ch + gap);
        self.painter.card(&self.content, w_x, w_y, cw, ch, 15.0 * d, [82, 148, 255], 24, 38);
        let wx = weather::latest();
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
        self.painter.text(&self.content, &format!("{}°", wx.temp_c), w_x + 14.0 * d, w_y + 10.0 * d, 23.0, 700, fg, 1.0, Anchor::Left, 76.0 * d);
        self.painter.layered_weather(&self.content, w_x + cw - 28.0 * d, w_y + 25.0 * d, 26.0);
        self.painter.text(&self.content, cond, w_x + 14.0 * d, w_y + ch - 26.0 * d, 10.5, 600, fg, 0.88, Anchor::Left, cw * 0.56);
        self.painter.card(&self.content, w_x + cw - 82.0 * d, w_y + ch - 28.0 * d, 72.0 * d, 18.0 * d, 9.0 * d, fg, 18, 22);
        let hl = if vi {
            format!("C:{}° T:{}°", wx.high_c, wx.low_c)
        } else {
            format!("H:{}° L:{}°", wx.high_c, wx.low_c)
        };
        self.painter.pill_text(&self.content, &hl, w_x + cw - 46.0 * d, w_y + ch - 19.0 * d, 9.2, 600, fg, 0.82, 70.0 * d);
        self.regions.push(Region {
            x: w_x,
            y: w_y,
            w: cw,
            h: ch,
            action: Action::PinWidget(WidgetKind::Weather),
            drag_widget: Some(WidgetKind::Weather),
        });

        // 4. System monitor (bottom-right)
        let (s_x, s_y) = (x + cw + gap, y + ch + gap);
        self.painter.card(&self.content, s_x, s_y, cw, ch, 15.0 * d, fg, 18, 36);
        let m = &self.metrics;
        self.painter.sparkline(&self.content, s_x + 10.0 * d, s_y + ch - 36.0 * d, cw - 20.0 * d, 20.0 * d, &m.cpu_hist, [96, 175, 255]);
        let rows: [(&str, String, f32, [u8; 3]); 3] = [
            ("CPU", format!("{}%", m.cpu_pct), m.cpu_pct as f32 / 100.0, [96, 175, 255]),
            (
                "GPU",
                match m.gpu_temp_c {
                    Some(t) => format!("{}% {t}°", m.gpu_pct),
                    None => format!("{}%", m.gpu_pct),
                },
                m.gpu_pct as f32 / 100.0,
                [255, 118, 92],
            ),
            ("RAM", format!("{}%", m.ram_pct), m.ram_pct as f32 / 100.0, [64, 218, 128]),
        ];
        let mut ry = s_y + 9.0 * d;
        let bar_x = s_x + 44.0 * d;
        let bar_w = cw - 106.0 * d;
        for (lbl, val, fr, col) in rows {
            self.painter.text(&self.content, lbl, s_x + 12.0 * d, ry, 9.5, 700, fg, 0.74, Anchor::Left, 30.0 * d);
            self.painter.rect(&self.content, bar_x, ry + 5.0 * d, bar_w, 4.8 * d, 2.4 * d, rgba(fg, 32));
            self.painter.grad_rect(&self.content, bar_x, ry + 5.0 * d, (bar_w * fr).max(5.0 * d), 4.8 * d, 2.4 * d, rgba(col, 250), rgba(WHITE, 210), true);
            self.painter.text(&self.content, &val, s_x + cw - 12.0 * d, ry, 9.5, 600, fg, 0.94, Anchor::Right, 54.0 * d);
            ry += 15.0 * d;
        }
        let net = format!("↓ {}   ↑ {}", format_speed(m.net_down_bps), format_speed(m.net_up_bps));
        self.painter.pill_text(&self.content, &net, s_x + cw / 2.0, s_y + ch - 12.5 * d, 9.0, 600, fg, 0.80, cw - 16.0 * d);
        self.regions.push(Region {
            x: s_x,
            y: s_y,
            w: cw,
            h: ch,
            action: Action::PinWidget(WidgetKind::SystemMonitor),
            drag_widget: Some(WidgetKind::SystemMonitor),
        });
    }

    fn draw_panels_tab(&mut self, x: f32, y: f32, w: f32, h: f32, fg: [u8; 3], accent: [u8; 3]) {
        let d = self.dpi;
        let vi = prefs::is_vi();
        let notes = prefs::get().notes;
        let title = if vi {
            format!("Ghi chú nhanh ({})", notes.len())
        } else {
            format!("Quick Notes ({})", notes.len())
        };
        self.painter.text(&self.content, &title, x + 4.0 * d, y + 3.0 * d, 12.0, 700, fg, 0.94, Anchor::Left, 180.0 * d);
        self.painter.card(&self.content, x + w - 134.0 * d, y, 134.0 * d, 22.0 * d, 11.0 * d, accent, 82, 64);
        self.painter.pill_text(&self.content, if vi { "+ Dán Clipboard" } else { "+ Paste Clipboard" }, x + w - 67.0 * d, y + 11.0 * d, 9.5, 600, WHITE, 0.98, 126.0 * d);
        self.regions.push(Region {
            x: x + w - 136.0 * d,
            y,
            w: 136.0 * d,
            h: 24.0 * d,
            action: Action::AddClipboardNote,
            drag_widget: None,
        });

        let mut ry = y + 28.0 * d;
        let row_h = 30.0 * d;
        if notes.is_empty() {
            self.painter.card(&self.content, x, ry, w, h - 32.0 * d, 14.0 * d, fg, 14, 28);
            self.painter.pill_text(
                &self.content,
                if vi {
                    "Chưa có ghi chú — sao chép chữ rồi bấm '+ Dán Clipboard'"
                } else {
                    "No notes yet — copy text and click '+ Paste Clipboard'"
                },
                x + w / 2.0,
                y + h / 2.0 + 10.0 * d,
                11.0,
                500,
                fg,
                0.68,
                w - 24.0 * d,
            );
            return;
        }
        for (i, note) in notes.iter().take(4).enumerate() {
            self.painter.card(&self.content, x, ry, w, row_h, 11.0 * d, fg, 18, 30);
            self.painter.sym(&self.content, Sym::Document, x + 16.0 * d, ry + row_h / 2.0, 12.0, accent, 0.90);
            let txt = self.painter.fit(note, 11.0, 500, w - 66.0 * d);
            self.painter.text(&self.content, &txt, x + 28.0 * d, ry + 7.5 * d, 11.0, 500, fg, 0.94, Anchor::Left, w - 66.0 * d);
            self.painter.sym(&self.content, Sym::Trash, x + w - 18.0 * d, ry + row_h / 2.0, 11.5, [255, 118, 118], 0.85);
            self.regions.push(Region {
                x: x + w - 34.0 * d,
                y: ry,
                w: 34.0 * d,
                h: row_h,
                action: Action::DeleteNote(i),
                drag_widget: None,
            });
            self.regions.push(Region {
                x,
                y: ry,
                w: w - 34.0 * d,
                h: row_h,
                action: Action::CopyNote(i),
                drag_widget: None,
            });
            ry += row_h + 6.0 * d;
        }
    }

    fn draw_quick_tab(&mut self, x: f32, y: f32, w: f32, h: f32, fg: [u8; 3], accent: [u8; 3]) {
        let d = self.dpi;
        let br = brightness::current();
        let aud = audio::latest();
        let vol = (aud.volume * 100.0).round() as u8;

        // Vertical pill sliders (Brightness + Volume)
        let sw = 48.0 * d;
        let sh = h - 35.0 * d;
        for (i, (val, sym, tint, is_br)) in [
            (br, Sym::Sun, [255, 206, 98], true),
            (vol, if aud.muted { Sym::SpeakerMute } else { Sym::Speaker }, [118, 188, 255], false),
        ]
        .into_iter()
        .enumerate()
        {
            let sx = x + i as f32 * (sw + 10.0 * d);
            self.painter.card(&self.content, sx, y, sw, sh, 16.0 * d, [8, 10, 14], 96, 38);
            let fh = (sh * val as f32 / 100.0).max(10.0 * d);
            self.painter.grad_rect(
                &self.content,
                sx + 1.0 * d,
                y + sh - fh + 1.0 * d,
                sw - 2.0 * d,
                (fh - 2.0 * d).max(6.0 * d),
                15.0 * d,
                rgba(WHITE, 238),
                rgba(tint, 215),
                false,
            );
            self.painter.sym(&self.content, sym, sx + sw / 2.0, y + sh - 18.0 * d, 14.0, [20, 22, 28], 0.92);
            self.painter.pill_text(&self.content, &format!("{val}%"), sx + sw / 2.0, y + 14.0 * d, 9.5, 700, if val > 78 { [20, 22, 28] } else { fg }, 0.94, sw);
            self.regions.push(Region {
                x: sx,
                y,
                w: sw,
                h: sh,
                action: if is_br { Action::BrightnessBar(y, sh) } else { Action::VolumeBar(y, sh) },
                drag_widget: None,
            });
        }

        // Toggle tiles grid (3x2)
        let gx = x + 2.0 * sw + 20.0 * d;
        let gw = w - (gx - x);
        let tgap = 8.0 * d;
        let tw = (gw - 2.0 * tgap) / 3.0;
        let th = (sh - tgap) / 2.0;
        let vi = prefs::is_vi();
        let wifi_name = wifi_bt::wifi_ssid();
        let out_name = audio::output_name();
        let on_lbl = if vi { "Bật" } else { "On" };
        let off_lbl = if vi { "Tắt" } else { "Off" };
        let tiles: [(bool, Sym, [u8; 3], &str, &str, Action); 6] = [
            (wifi_bt::wifi_on(), Sym::Wifi, [66, 148, 255], "Wi-Fi", if wifi_name.is_empty() { on_lbl } else { &wifi_name }, Action::ToggleWifi),
            (brightness::night_light(), Sym::Moon, [255, 166, 62], if vi { "Ánh sáng đêm" } else { "Night light" }, if brightness::night_light() { on_lbl } else { off_lbl }, Action::ToggleNight),
            (wifi_bt::bt_on(), Sym::Bluetooth, [74, 178, 255], "Bluetooth", if wifi_bt::bt_on() { on_lbl } else { off_lbl }, Action::ToggleBt),
            (!aud.mic_muted, if aud.mic_muted { Sym::MicMute } else { Sym::Mic }, if aud.mic_muted { [255, 92, 92] } else { [64, 214, 134] }, "Mic", if aud.mic_muted { if vi { "Đã tắt" } else { "Muted" } } else { on_lbl }, Action::ToggleMicMute),
            (!aud.muted, if aud.muted { Sym::SpeakerMute } else { Sym::Speaker }, [72, 210, 142], if vi { "Âm thanh" } else { "Sound" }, if out_name.is_empty() { if vi { "Loa" } else { "Speakers" } } else { &out_name }, Action::ToggleVolMute),
            (self.timer.phase == Phase::Running, Sym::Clock, [168, 126, 255], if vi { "Tập trung" } else { "Focus" }, if self.timer.phase == Phase::Running { if vi { "Đang chạy" } else { "Running" } } else { off_lbl }, Action::TimerToggle),
        ];
        for (i, (on, sym, badge, title, sub, act)) in tiles.into_iter().enumerate() {
            let (col, row) = (i % 3, i / 3);
            let tx = gx + col as f32 * (tw + tgap);
            let ty = y + row as f32 * (th + tgap);
            self.painter.card(
                &self.content,
                tx,
                ty,
                tw,
                th,
                14.0 * d,
                if on { accent } else { fg },
                if on { 112 } else { 18 },
                if on { 78 } else { 32 },
            );
            let bx = tx + 17.0 * d;
            let by = ty + th / 2.0;
            let br = 12.0 * d;
            self.painter.rect(&self.content, bx - br, by - br, br * 2.0, br * 2.0, br, if on { rgba(badge, 235) } else { rgba(fg, 32) });
            self.painter.sym(&self.content, sym, bx, by, 12.5, WHITE, 0.98);
            let st = self.painter.fit(sub, 9.2, 500, tw - 42.0 * d);
            self.painter.text(&self.content, title, tx + 34.0 * d, ty + 11.0 * d, 11.0, 700, WHITE, 0.98, Anchor::Left, tw - 38.0 * d);
            self.painter.text(&self.content, &st, tx + 34.0 * d, ty + 27.0 * d, 9.2, 500, WHITE, 0.74, Anchor::Left, tw - 38.0 * d);
            self.regions.push(Region {
                x: tx,
                y: ty,
                w: tw,
                h: th,
                action: act,
                drag_widget: None,
            });
        }

        // Bottom Power Mode segmented bar
        let py = y + sh + 8.0 * d;
        let ph = 27.0 * d;
        self.painter.card(&self.content, x, py, w, ph, ph / 2.0, fg, 16, 30);
        let seg_w = w / 3.0;
        for (i, pm) in PowerMode::ALL.into_iter().enumerate() {
            let px = x + i as f32 * seg_w;
            let on = self.power_mode == pm;
            if on {
                self.painter.card(&self.content, px + 2.5 * d, py + 2.5 * d, seg_w - 5.0 * d, ph - 5.0 * d, (ph - 5.0 * d) / 2.0, accent, 110, 72);
            }
            self.painter.pill_text(&self.content, pm.label_i18n(vi), px + seg_w / 2.0, py + ph / 2.0, 10.5, if on { 700 } else { 500 }, fg, if on { 1.0 } else { 0.72 }, seg_w);
            self.regions.push(Region {
                x: px,
                y: py,
                w: seg_w,
                h: ph,
                action: Action::PowerMode(pm),
                drag_widget: None,
            });
        }
    }

    fn draw_tools_tab(&mut self, x: f32, y: f32, w: f32, h: f32, fg: [u8; 3], accent: [u8; 3]) {
        let d = self.dpi;
        let vi = prefs::is_vi();
        let s = prefs::get();
        let gap = 8.0 * d;
        let bw = (w - 4.0 * gap) / 5.0;
        let bh = 52.0 * d;
        let rec_active = self.recorder.is_some();
        let rec_lbl = if let Some(ref r) = self.recorder {
            format!("{:02}:{:02}", r.elapsed_secs() / 60, r.elapsed_secs() % 60)
        } else if vi {
            "Quay MH".to_string()
        } else {
            "Record".to_string()
        };
        let top_acts: [(Sym, [u8; 3], &str, Action); 5] = [
            (Sym::Camera, [84, 158, 255], if vi { "Chụp MH" } else { "Screenshot" }, Action::Screenshot),
            (Sym::Scissors, [118, 206, 255], if vi { "Cắt vùng" } else { "Snip" }, Action::Snip),
            (
                if rec_active { Sym::Stop } else { Sym::Record },
                [255, 76, 86],
                &rec_lbl,
                Action::RecordToggle,
            ),
            (Sym::Eyedropper, [255, 134, 178], if vi { "Lấy màu" } else { "Color" }, Action::PickColor),
            (Sym::Qr, accent, if vi { "Gửi QR" } else { "Send QR" }, Action::SendShelfQr),
        ];
        for (i, (sym, badge, lbl, act)) in top_acts.into_iter().enumerate() {
            let bx = x + i as f32 * (bw + gap);
            let hl = i == 2 && rec_active;
            self.painter.card(&self.content, bx, y, bw, bh, 14.0 * d, if hl { [255, 76, 86] } else { fg }, if hl { 80 } else { 20 }, if hl { 78 } else { 34 });
            self.painter.rect(&self.content, bx + bw / 2.0 - 12.0 * d, y + 6.0 * d, 24.0 * d, 24.0 * d, 12.0 * d, rgba(badge, 220));
            self.painter.sym(&self.content, sym, bx + bw / 2.0, y + 18.0 * d, 12.5, WHITE, 0.98);
            self.painter.pill_text(&self.content, lbl, bx + bw / 2.0, y + 40.0 * d, 9.8, 600, fg, 0.94, bw - 4.0 * d);
            self.regions.push(Region {
                x: bx,
                y,
                w: bw,
                h: bh,
                action: act,
                drag_widget: None,
            });
        }

        // Row of 5 "+ Add" customizable quick shortcut launchers (00:35)
        let sc_y = y + bh + 8.0 * d;
        let sc_h = 44.0 * d;
        let sw = (w - 4.0 * gap) / 5.0;
        for (i, path) in s.tool_shortcuts.iter().enumerate() {
            let sx = x + i as f32 * (sw + gap);
            let set = !path.is_empty();
            self.painter.card(&self.content, sx, sc_y, sw, sc_h, 12.0 * d, if set { accent } else { fg }, if set { 32 } else { 12 }, if set { 44 } else { 26 });
            if set {
                let name = std::path::Path::new(path)
                    .file_stem()
                    .or_else(|| std::path::Path::new(path).file_name())
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| "App".to_string());
                let fit = self.painter.fit(&name, 9.5, 600, sw - 10.0 * d);
                self.painter.sym(&self.content, Sym::Document, sx + sw / 2.0, sc_y + 15.0 * d, 12.5, accent, 0.95);
                self.painter.pill_text(&self.content, &fit, sx + sw / 2.0, sc_y + 33.0 * d, 9.2, 600, fg, 0.92, sw - 6.0 * d);
            } else {
                self.painter.sym(&self.content, Sym::Plus, sx + sw / 2.0, sc_y + 15.0 * d, 11.5, fg, 0.65);
                self.painter.pill_text(&self.content, if vi { "Thêm" } else { "Add" }, sx + sw / 2.0, sc_y + 33.0 * d, 9.2, 500, fg, 0.65, sw);
            }
            self.regions.push(Region {
                x: sx,
                y: sc_y,
                w: sw,
                h: sc_h,
                action: Action::ToolSlot(i),
                drag_widget: None,
            });
        }

        // Bottom Shelf / QR / Status strip
        let sy = sc_y + sc_h + 8.0 * d;
        let sh = h - bh - sc_h - 16.0 * d;
        self.painter.card(&self.content, x, sy, w, sh, 14.0 * d, fg, 15, 32);
        if let (Some(srv), Some(qr_surf)) = (&self.send_srv, &self.send_qr) {
            let qsz = (sh - 10.0 * d).min(64.0 * d);
            self.painter.rect(&self.content, x + 8.0 * d, sy + 5.0 * d, qsz, qsz, 8.0 * d, rgba(WHITE, 255));
            self.painter.image(&self.content, qr_surf, x + 10.0 * d, sy + 7.0 * d, qsz - 4.0 * d, qsz - 4.0 * d, 4.0 * d);
            let tx = x + qsz + 18.0 * d;
            self.painter.text(&self.content, if vi { "Quét QR bằng điện thoại (cùng Wi-Fi)" } else { "Scan QR from phone (same Wi-Fi)" }, tx, sy + 12.0 * d, 11.0, 700, fg, 0.96, Anchor::Left, w - qsz - 34.0 * d);
            let u = self.painter.fit(&srv.url, 10.0, 600, w - qsz - 34.0 * d);
            self.painter.text(&self.content, &u, tx, sy + 30.0 * d, 10.0, 600, accent, 0.98, Anchor::Left, w - qsz - 34.0 * d);
            return;
        }
        if self.shelf.is_empty() {
            let msg = if self.tool_status.is_empty() {
                if vi {
                    "Kéo thả tệp vào Island để lưu/QR  ·  Chuột phải ô trên để xoá"
                } else {
                    "Drop files onto Island for Shelf/QR  ·  Right-click a shortcut above to clear"
                }
            } else {
                &self.tool_status
            };
            if let Some(rgb) = self.picked_color {
                let sw_box = 16.0 * d;
                let tx = x + w / 2.0;
                self.painter.rect(&self.content, tx - 68.0 * d, sy + sh / 2.0 - 8.0 * d, sw_box, sw_box, 5.0 * d, rgba(rgb, 255));
                self.painter.stroke_rect(&self.content, tx - 68.0 * d, sy + sh / 2.0 - 8.0 * d, sw_box, sw_box, 5.0 * d, 1.0, rgba(WHITE, 140), rgba(WHITE, 50));
                self.painter.pill_text(&self.content, msg, tx + 10.0 * d, sy + sh / 2.0, 11.0, 600, fg, 0.92, w - 60.0 * d);
            } else {
                self.painter.pill_text(&self.content, msg, x + w / 2.0, sy + sh / 2.0, 10.2, 500, fg, 0.72, w - 20.0 * d);
            }
        } else {
            let shelf_hdr = if vi { format!("Ngăn ({})", self.shelf.len()) } else { format!("Shelf ({})", self.shelf.len()) };
            self.painter.text(&self.content, &shelf_hdr, x + 12.0 * d, sy + 7.0 * d, 10.0, 700, fg, 0.80, Anchor::Left, 90.0 * d);
            let slot_w = (w - 150.0 * d) / 3.0;
            for (i, path) in self.shelf.iter().take(3).enumerate() {
                let sx = x + 92.0 * d + i as f32 * (slot_w + 5.0 * d);
                self.painter.card(&self.content, sx, sy + 6.0 * d, slot_w, sh - 12.0 * d, 9.0 * d, fg, 20, 26);
                let name = std::path::Path::new(path)
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| path.clone());
                let fit = self.painter.fit(&name, 9.2, 600, slot_w - 8.0 * d);
                self.painter.pill_text(&self.content, &fit, sx + slot_w / 2.0, sy + sh / 2.0, 9.2, 600, fg, 0.92, slot_w - 6.0 * d);
                self.regions.push(Region {
                    x: sx,
                    y: sy + 6.0 * d,
                    w: slot_w,
                    h: sh - 12.0 * d,
                    action: Action::OpenShelfItem(i),
                    drag_widget: None,
                });
            }
            self.painter.card(&self.content, x + w - 50.0 * d, sy + 7.0 * d, 42.0 * d, sh - 14.0 * d, 9.0 * d, accent, 72, 54);
            self.painter.pill_text(&self.content, if vi { "Xoá" } else { "Clear" }, x + w - 29.0 * d, sy + sh / 2.0, 9.5, 600, WHITE, 0.95, 40.0 * d);
            self.regions.push(Region {
                x: x + w - 52.0 * d,
                y: sy,
                w: 52.0 * d,
                h: sh,
                action: Action::ClearShelf,
                drag_widget: None,
            });
        }
    }

    fn draw_ai_tab(&mut self, x: f32, y: f32, w: f32, h: f32, fg: [u8; 3], accent: [u8; 3]) {
        let d = self.dpi;
        let vi = prefs::is_vi();
        let s = prefs::get();
        self.painter.text(&self.content, if vi { "Trung tâm Trợ lý AI" } else { "AI Agents Hub" }, x + 4.0 * d, y + 3.0 * d, 12.0, 700, fg, 0.94, Anchor::Left, 180.0 * d);

        let ask_w = 196.0 * d;
        self.painter.card(&self.content, x + w - ask_w, y, ask_w, 24.0 * d, 12.0 * d, accent, 108, 76);
        self.painter.sym(&self.content, Sym::Sparkles, x + w - ask_w + 15.0 * d, y + 12.0 * d, 11.5, WHITE, 0.98);
        let ask_lbl = if vi {
            format!("Hỏi {} kèm Clipboard", s.default_ai.label())
        } else {
            format!("Ask {} w/ Clipboard", s.default_ai.label())
        };
        self.painter.pill_text(&self.content, &ask_lbl, x + w - ask_w / 2.0 + 8.0 * d, y + 12.0 * d, 9.5, 600, WHITE, 0.98, ask_w - 28.0 * d);
        self.regions.push(Region {
            x: x + w - ask_w,
            y,
            w: ask_w,
            h: 26.0 * d,
            action: Action::AskAiWithClipboard,
            drag_widget: None,
        });

        let gy = y + 32.0 * d;
        let gap = 8.0 * d;
        let bw = (w - 4.0 * gap) / 5.0;
        let bh = 68.0 * d;
        for (i, ai) in AiProvider::ALL.into_iter().enumerate() {
            let bx = x + i as f32 * (bw + gap);
            let on = s.default_ai == ai;
            self.painter.card(&self.content, bx, gy, bw, bh, 14.0 * d, if on { accent } else { fg }, if on { 82 } else { 18 }, if on { 85 } else { 32 });
            self.painter.rect(&self.content, bx + bw / 2.0 - 14.0 * d, gy + 11.0 * d, 28.0 * d, 28.0 * d, 14.0 * d, rgba(if on { accent } else { [112, 168, 255] }, if on { 235 } else { 70 }));
            self.painter.sym(&self.content, Sym::Sparkles, bx + bw / 2.0, gy + 25.0 * d, 13.5, WHITE, 0.98);
            self.painter.pill_text(&self.content, ai.label(), bx + bw / 2.0, gy + 51.0 * d, 9.8, if on { 700 } else { 600 }, fg, 0.95, bw - 6.0 * d);
            self.regions.push(Region {
                x: bx,
                y: gy,
                w: bw,
                h: bh,
                action: Action::LaunchAi(ai),
                drag_widget: None,
            });
        }

        let cy = gy + bh + 10.0 * d;
        let ch = h - (cy - y);
        self.painter.card(&self.content, x, cy, w, ch, 14.0 * d, fg, 15, 28);
        let hist = clipboard::history();
        let preview = hist
            .first()
            .map(|c| {
                if vi {
                    format!("Clipboard gần nhất: {}", c.text.replace('\n', " "))
                } else {
                    format!("Latest clipboard: {}", c.text.replace('\n', " "))
                }
            })
            .unwrap_or_else(|| {
                if vi {
                    "Bấm chọn trợ lý AI để mở, hoặc vào Cài đặt → Trợ lý AI để đặt mặc định.".to_string()
                } else {
                    "Click any AI card to launch, or right-click Settings → AI agents to set default.".to_string()
                }
            });
        let fit = self.painter.fit(&preview, 10.2, 500, w - 24.0 * d);
        self.painter.pill_text(&self.content, &fit, x + w / 2.0, cy + ch / 2.0, 10.2, 500, fg, 0.78, w - 24.0 * d);
    }

    fn draw_calls_tab(&mut self, x: f32, y: f32, w: f32, h: f32, fg: [u8; 3], accent: [u8; 3]) {
        let d = self.dpi;
        let vi = prefs::is_vi();
        let s = prefs::get();
        let aud = audio::latest();
        let gap = 10.0 * d;
        let bw = (w - gap) / 2.0;

        // Big Universal Mic Mute button
        let mic_on = !aud.mic_muted;
        let mic_rgb = if mic_on { [64, 214, 134] } else { [255, 84, 92] };
        self.painter.card(&self.content, x, y, bw, h, 16.0 * d, mic_rgb, if mic_on { 58 } else { 112 }, 76);
        self.painter.rect(&self.content, x + bw / 2.0 - 20.0 * d, y + 22.0 * d, 40.0 * d, 40.0 * d, 20.0 * d, rgba(mic_rgb, 235));
        self.painter.sym(&self.content, if mic_on { Sym::Mic } else { Sym::MicMute }, x + bw / 2.0, y + 42.0 * d, 18.0, WHITE, 0.98);
        self.painter.pill_text(&self.content, if mic_on { if vi { "Mic đang mở" } else { "Microphone Live" } } else { if vi { "Đã tắt Mic" } else { "Microphone Muted" } }, x + bw / 2.0, y + 78.0 * d, 13.0, 700, WHITE, 0.98, bw - 16.0 * d);
        self.painter.pill_text(&self.content, if vi { "Ctrl + Alt + M  tắt/mở nhanh" } else { "Ctrl + Alt + M  universal mute" }, x + bw / 2.0, y + 100.0 * d, 9.8, 500, fg, 0.72, bw - 16.0 * d);
        self.regions.push(Region {
            x,
            y,
            w: bw,
            h,
            action: Action::ToggleMicMute,
            drag_widget: None,
        });

        // Right column: Hardware activity + Screen Capture Shield
        let rx = x + bw + gap;
        let rh = (h - gap) / 2.0;
        self.painter.card(&self.content, rx, y, bw, rh, 15.0 * d, fg, 18, 32);
        let mic_live = if self.privacy.0 { if vi { "Đang dùng" } else { "In Use" } } else { if vi { "Rảnh" } else { "Idle" } };
        let cam_live = if self.privacy.1 { if vi { "Đang bật" } else { "Active" } } else { if vi { "Tắt" } else { "Off" } };
        let mic_hw_txt = if vi { format!("Phần cứng Mic: {mic_live}") } else { format!("Mic hardware: {mic_live}") };
        let cam_hw_txt = if vi { format!("Camera: {cam_live}") } else { format!("Camera hardware: {cam_live}") };
        self.painter.rect(&self.content, rx + 14.0 * d, y + 18.0 * d, 8.0 * d, 8.0 * d, 4.0 * d, rgba(if self.privacy.0 { [255, 159, 10] } else { fg }, 240));
        self.painter.text(&self.content, &mic_hw_txt, rx + 28.0 * d, y + 14.0 * d, 10.5, 600, fg, 0.92, Anchor::Left, bw - 36.0 * d);
        self.painter.rect(&self.content, rx + 14.0 * d, y + 38.0 * d, 8.0 * d, 8.0 * d, 4.0 * d, rgba(if self.privacy.1 { [48, 209, 88] } else { fg }, 240));
        self.painter.text(&self.content, &cam_hw_txt, rx + 28.0 * d, y + 34.0 * d, 10.5, 600, fg, 0.92, Anchor::Left, bw - 36.0 * d);

        let sy = y + rh + gap;
        let shield = s.stream_privacy_shield;
        self.painter.card(&self.content, rx, sy, bw, rh, 15.0 * d, if shield { accent } else { fg }, if shield { 86 } else { 18 }, if shield { 82 } else { 32 });
        self.painter.sym(&self.content, Sym::Shield, rx + 22.0 * d, sy + rh / 2.0, 15.0, if shield { WHITE } else { fg }, 0.95);
        self.painter.text(&self.content, if vi { "Khiên chống quay lén" } else { "Stream Capture Shield" }, rx + 38.0 * d, sy + 13.0 * d, 11.0, 700, fg, 0.95, Anchor::Left, bw - 44.0 * d);
        self.painter.text(&self.content, if shield { if vi { "Đang ẩn khi chia sẻ màn hình" } else { "Hidden in screen shares" } } else { if vi { "Hiện trong bản ghi màn hình" } else { "Visible in recordings" } }, rx + 38.0 * d, sy + 30.0 * d, 9.5, 500, fg, 0.72, Anchor::Left, bw - 44.0 * d);
        self.regions.push(Region {
            x: rx,
            y: sy,
            w: bw,
            h: rh,
            action: Action::ToggleCaptureShield,
            drag_widget: None,
        });
    }

    fn draw_clipboard_tab(&mut self, x: f32, y: f32, w: f32, h: f32, fg: [u8; 3], accent: [u8; 3]) {
        let d = self.dpi;
        let vi = prefs::is_vi();
        let items = clipboard::history();
        self.painter.text(&self.content, &format!("Clipboard ({})", items.len()), x + 4.0 * d, y + 3.0 * d, 12.0, 700, fg, 0.92, Anchor::Left, 180.0 * d);
        if !items.is_empty() {
            self.painter.card(&self.content, x + w - 104.0 * d, y, 102.0 * d, 22.0 * d, 11.0 * d, accent, 64, 52);
            self.painter.pill_text(&self.content, if vi { "Xoá mục chưa ghim" } else { "Clear unpinned" }, x + w - 53.0 * d, y + 11.0 * d, 9.2, 600, WHITE, 0.96, 98.0 * d);
            self.regions.push(Region {
                x: x + w - 106.0 * d,
                y,
                w: 106.0 * d,
                h: 24.0 * d,
                action: Action::ClipClear,
                drag_widget: None,
            });
        }
        if items.is_empty() {
            self.painter.card(&self.content, x, y + 28.0 * d, w, h - 32.0 * d, 14.0 * d, fg, 14, 28);
            self.painter.sym(&self.content, Sym::Clipboard, x + w / 2.0, y + h / 2.0 - 8.0 * d, 22.0, fg, 0.45);
            self.painter.pill_text(&self.content, if vi { "Sao chép văn bản bất kỳ nơi đâu để xem lịch sử tại đây." } else { "Copy text anywhere to see history here." }, x + w / 2.0, y + h / 2.0 + 22.0 * d, 11.5, 500, fg, 0.65, w);
            return;
        }
        let mut ry = y + 28.0 * d;
        let row_h = 30.0 * d;
        for (i, item) in items.iter().take(4).enumerate() {
            self.painter.card(
                &self.content,
                x,
                ry,
                w,
                row_h,
                11.0 * d,
                if item.pinned { accent } else { fg },
                if item.pinned { 40 } else { 18 },
                if item.pinned { 64 } else { 30 },
            );
            self.painter.rect(&self.content, x + 8.0 * d, ry + 12.0 * d, 6.0 * d, 6.0 * d, 3.0 * d, rgba(if item.pinned { accent } else { fg }, if item.pinned { 240 } else { 85 }));
            let preview = self.painter.fit(&item.text.replace('\n', " "), 11.0, 500, w - 58.0 * d);
            self.painter.text(&self.content, &preview, x + 22.0 * d, ry + 7.5 * d, 11.0, 500, fg, 0.94, Anchor::Left, w - 58.0 * d);
            self.painter.sym(&self.content, Sym::Pin, x + w - 18.0 * d, ry + row_h / 2.0, 12.0, if item.pinned { accent } else { fg }, if item.pinned { 1.0 } else { 0.48 });
            self.regions.push(Region {
                x: x + w - 34.0 * d,
                y: ry,
                w: 34.0 * d,
                h: row_h,
                action: Action::ClipPin(i),
                drag_widget: None,
            });
            self.regions.push(Region {
                x,
                y: ry,
                w: w - 34.0 * d,
                h: row_h,
                action: Action::ClipCopy(i),
                drag_widget: None,
            });
            ry += row_h + 6.0 * d;
        }
    }

    fn execute_action(&mut self, act: Action, lx: f32, ly: f32) {
        match act {
            Action::SelectTab(t) => {
                self.active_tab = t;
                self.dispatch(Event::SelectTab(t));
            }
            Action::Settings => (self.open_settings_cb)(),
            Action::CloseIsland => self.dispatch(Event::Escape),
            Action::PlayPause => {
                if self.demo_media {
                    self.playing = !self.playing;
                } else {
                    let _ = self.media.commands.send(Command::PlayPause);
                }
                self.render();
            }
            Action::Next => {
                let _ = self.media.commands.send(Command::Next);
            }
            Action::Prev => {
                let _ = self.media.commands.send(Command::Prev);
            }
            Action::Seek(sx, sw) => {
                let frac = ((lx - sx) / sw.max(1.0)).clamp(0.0, 1.0);
                let ms = (self.dur_ms as f32 * frac) as i64;
                self.pos_ms = ms;
                self.pos_at = Instant::now();
                self.render();
            }
            Action::TimerToggle => {
                self.timer.toggle(self.now_ms());
                self.render();
            }
            Action::TimerReset => {
                self.timer.reset();
                self.render();
            }
            Action::TimerDelta(dm) => {
                self.timer.adjust_minutes(dm);
                self.render();
            }
            Action::PinWidget(kind) => {
                desktop_widgets::toggle_widget(kind);
            }
            Action::BrightnessBar(top, h) => {
                let frac = (1.0 - (ly - top) / h.max(1.0)).clamp(0.0, 1.0);
                let pct = (frac * 100.0).round() as u8;
                let _ = self.display.commands.send(brightness::Cmd::SetBrightness(pct));
            }
            Action::VolumeBar(top, h) => {
                let frac = (1.0 - (ly - top) / h.max(1.0)).clamp(0.0, 1.0);
                let _ = self.audio.commands.send(audio::Cmd::SetVolume(frac));
            }
            Action::ToggleWifi => {
                wifi_bt::toggle_wifi();
                self.render();
            }
            Action::ToggleBt => {
                wifi_bt::toggle_bluetooth();
                self.render();
            }
            Action::ToggleNight => {
                let _ = self.display.commands.send(brightness::Cmd::SetNightLight(!brightness::night_light()));
            }
            Action::ToggleMicMute => {
                let _ = self.audio.commands.send(audio::Cmd::ToggleMicMute);
            }
            Action::ToggleVolMute => {
                let _ = self.audio.commands.send(audio::Cmd::ToggleMute);
            }
            Action::PowerMode(pm) => {
                self.power_mode = pm;
                sys::set_power_mode(pm);
                self.render();
            }
            Action::Screenshot => {
                let vi = prefs::is_vi();
                if let Some(p) = capture::capture_fullscreen_png() {
                    let name = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                    let msg = if vi { format!("Đã lưu {name}") } else { format!("Saved {name}") };
                    self.tool_status = msg.clone();
                    self.shelf.insert(0, p.to_string_lossy().to_string());
                    if prefs::get().notifications_in_island {
                        self.flash_hud(msg, None, [84, 158, 255]);
                    }
                } else {
                    self.tool_status = if vi { "Chụp màn hình thất bại" } else { "Screenshot failed" }.to_string();
                }
                self.render();
            }
            Action::Snip => {
                let vi = prefs::is_vi();
                let ok = capture::trigger_snip();
                self.tool_status = if ok {
                    if vi { "Đã mở công cụ cắt ảnh" } else { "Snipping overlay opened" }
                } else if vi {
                    "Không mở được Snipping"
                } else {
                    "Snip unavailable"
                }
                .to_string();
                if ok && prefs::get().notifications_in_island {
                    self.flash_hud(self.tool_status.clone(), None, [118, 206, 255]);
                }
                self.render();
            }
            Action::RecordToggle => {
                let vi = prefs::is_vi();
                if let Some(rec) = self.recorder.take() {
                    let out = rec.stop();
                    let name = out.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                    let msg = if vi { format!("Đã lưu {name}") } else { format!("Saved {name}") };
                    self.tool_status = msg.clone();
                    self.shelf.insert(0, out.to_string_lossy().to_string());
                    if prefs::get().notifications_in_island {
                        self.flash_hud(msg, None, [255, 92, 102]);
                    }
                } else {
                    let hw = self.hwnd.0 as isize;
                    self.recorder = capture::Recorder::start(move || {
                        let _ = unsafe { PostMessageW(Some(HWND(hw as *mut _)), WM_RECORD, WPARAM(0), LPARAM(0)) };
                    });
                    if self.recorder.is_some() {
                        self.tool_status = if vi { "Đang quay màn hình (.avi)..." } else { "Recording screen (.avi)..." }.to_string();
                    } else {
                        self.tool_status = if vi { "Quay màn hình thất bại" } else { "Recorder failed" }.to_string();
                    }
                }
                self.apply_geometry(true);
                self.render();
            }
            Action::PickColor => {
                let vi = prefs::is_vi();
                if let Some([r, g, b]) = capture::pick_color_at_cursor() {
                    let hex = format!("#{:02X}{:02X}{:02X}", r, g, b);
                    clipboard::copy_text(&hex);
                    self.picked_color = Some([r, g, b]);
                    let msg = if vi { format!("Đã chép {hex}") } else { format!("Copied {hex}") };
                    self.tool_status = msg.clone();
                    if prefs::get().notifications_in_island {
                        self.flash_hud(msg, None, [r, g, b]);
                    }
                }
                self.render();
            }
            Action::SendShelfQr => {
                if self.send_srv.is_some() {
                    self.send_srv = None;
                    self.send_qr = None;
                } else if !self.shelf.is_empty() {
                    let hw = self.hwnd.0 as isize;
                    if let Ok(srv) = sendsrv::start(&self.shelf, move || {
                        let _ = unsafe { PostMessageW(Some(HWND(hw as *mut _)), WM_SEND, WPARAM(0), LPARAM(0)) };
                    }) {
                        let px = (96.0 * self.dpi).round() as u32;
                        self.send_qr = qr::encode(&srv.url).and_then(|m| {
                            let bmp = qr::render(&m, px, 1);
                            self.gfx.surface_from_bgra(px, px, &bmp).ok()
                        });
                        self.send_srv = Some(srv);
                    }
                } else {
                    self.tool_status = "Drop files onto the Island first".to_string();
                }
                self.render();
            }
            Action::ToolSlot(idx) => {
                let cur = prefs::get().tool_shortcuts.get(idx).cloned().unwrap_or_default();
                if cur.is_empty() {
                    if let Some(p) = taskbar::pick_file_dialog(self.hwnd) {
                        prefs::update(|s| {
                            if idx < s.tool_shortcuts.len() {
                                s.tool_shortcuts[idx] = p;
                            }
                        });
                    }
                } else {
                    taskbar::launch_path(&cur);
                }
            }
            Action::ClearShelf => {
                self.shelf.clear();
                self.send_srv = None;
                self.send_qr = None;
                self.render();
            }
            Action::OpenShelfItem(i) => {
                if let Some(p) = self.shelf.get(i) {
                    taskbar::launch_path(p);
                }
            }
            Action::ClipCopy(i) => {
                let items = clipboard::history();
                if let Some(it) = items.get(i) {
                    clipboard::copy_text(&it.text);
                }
                self.render();
            }
            Action::ClipPin(i) => {
                clipboard::toggle_pin(i);
                self.render();
            }
            Action::ClipClear => {
                clipboard::clear_unpinned();
                self.render();
            }
            Action::AddClipboardNote => {
                let hist = clipboard::history();
                if let Some(first) = hist.first() {
                    let line: String = first.text.replace('\n', " ").chars().take(120).collect();
                    prefs::update(|s| {
                        s.notes.insert(0, line);
                        if s.notes.len() > 16 {
                            s.notes.truncate(16);
                        }
                    });
                }
            }
            Action::CopyNote(i) => {
                if let Some(n) = prefs::get().notes.get(i) {
                    clipboard::copy_text(n);
                    if prefs::get().notifications_in_island {
                        self.flash_hud("Note copied to clipboard", None, [110, 188, 255]);
                    }
                }
                self.render();
            }
            Action::DeleteNote(i) => {
                prefs::update(|s| {
                    if i < s.notes.len() {
                        s.notes.remove(i);
                    }
                });
            }
            Action::SelectDefaultAi(ai) => prefs::update(|s| s.default_ai = ai),
            Action::LaunchAi(ai) => {
                prefs::update(|s| s.default_ai = ai);
                taskbar::launch_path(ai.url());
            }
            Action::AskAiWithClipboard => {
                let s = prefs::get();
                taskbar::launch_path(s.default_ai.url());
            }
            Action::ToggleCaptureShield => prefs::update(|s| s.stream_privacy_shield = !s.stream_privacy_shield),
        }
    }

    fn on_media(&mut self) {
        let mut changed = false;
        while let Ok(snap) = self.media.snapshots.try_recv() {
            if !snap.has_media && self.demo_media {
                continue;
            }
            if matches!(self.drag, Some(DragOp::Seek(..))) && (!snap.can_seek || self.title != snap.title || self.artist != snap.artist || self.dur_ms != snap.duration_ms) {
                self.drag = None;
                unsafe { let _ = ReleaseCapture(); }
            }
            self.has_media = snap.has_media;
            self.title = snap.title;
            self.artist = snap.artist;
            self.playing = snap.playing;
            self.playback_rate = snap.playback_rate;
            if !matches!(self.drag, Some(DragOp::Seek(..))) {
                self.pos_ms = snap.position_ms;
            }
            self.dur_ms = snap.duration_ms;
            self.pos_at = Instant::now();
            self.can_seek = snap.can_seek;
            match snap.art {
                ArtUpdate::New(bmp) => {
                    self.art_tint = dominant_art_color(&bmp.bgra, bmp.w, bmp.h);
                    self.art = self.gfx.surface_from_bgra(bmp.w, bmp.h, &bmp.bgra).ok();
                }
                ArtUpdate::None => {
                    self.art = None;
                    self.art_tint = None;
                }
                ArtUpdate::Unchanged => {}
            }
            changed = true;
        }
        if changed {
            let np = if self.has_media {
                if self.artist.is_empty() {
                    self.title.clone()
                } else {
                    format!("{} — {}", self.title, self.artist)
                }
            } else {
                String::new()
            };
            taskbar_bar::set_now_playing(np);
            self.apply_geometry(true);
            self.render();
        }
    }
}

unsafe extern "system" fn proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        prefs::WM_PREFS => {
            with(|i| {
                let s = prefs::get();
                i.timer.sync_config(s.focus_mins, s.break_mins);
                i.apply_geometry(true);
                i.render();
            });
            LRESULT(0)
        }
        WM_MEDIA => {
            with(|i| i.on_media());
            LRESULT(0)
        }
        WM_AUDIO => {
            with(|i| {
                let s = prefs::get();
                let aud = audio::latest();
                let br = brightness::current();
                if s.volume_in_island && matches!(i.mode, Mode::Rest | Mode::Peek) {
                    let vi = prefs::is_vi();
                    if (aud.volume - i.last_vol).abs() > 0.015 {
                        let pct = (aud.volume * 100.0).round() as u8;
                        let lbl = if vi { format!("Âm lượng {pct}%") } else { format!("Volume {pct}%") };
                        i.flash_hud(lbl, Some(aud.volume), [110, 188, 255]);
                        i.apply_geometry(true);
                    } else if br != i.last_br {
                        let lbl = if vi { format!("Độ sáng {br}%") } else { format!("Brightness {br}%") };
                        i.flash_hud(lbl, Some(br as f32 / 100.0), [255, 206, 98]);
                        i.apply_geometry(true);
                    } else if aud.mic_muted != i.last_mic_muted && s.notifications_in_island {
                        i.flash_hud(
                            if aud.mic_muted {
                                if vi { "Đã tắt Mic" } else { "Mic Muted" }
                            } else if vi {
                                "Mic đang mở"
                            } else {
                                "Mic Live"
                            },
                            None,
                            if aud.mic_muted { [255, 92, 92] } else { [64, 214, 134] },
                        );
                        i.apply_geometry(true);
                    }
                }
                i.last_vol = aud.volume;
                i.last_br = br;
                i.last_mic_muted = aud.mic_muted;
                i.render();
            });
            LRESULT(0)
        }
        WM_WEATHER | WM_SEND | WM_RECORD => {
            with(|i| i.render());
            LRESULT(0)
        }
        WM_TIMER if wp.0 == T_DWELL => {
            let _ = KillTimer(Some(hwnd), T_DWELL);
            with(|i| i.dispatch(Event::Dwell));
            LRESULT(0)
        }
        WM_TIMER if wp.0 == T_LEAVE => {
            let _ = KillTimer(Some(hwnd), T_LEAVE);
            with(|i| {
                if !i.tracking && i.drag.is_none() && !prefs::get().sticky_open {
                    i.dispatch(Event::Leave);
                }
            });
            LRESULT(0)
        }
        WM_TIMER if wp.0 == T_ANIM => {
            with(|i| {
                let mut need = false;
                if i.hud.is_some() && i.active_hud().is_none() {
                    i.hud = None;
                    i.apply_geometry(true);
                    need = true;
                }
                if i.has_media && i.playing {
                    i.wave_phase += 0.32;
                    if matches!(i.mode, Mode::Rest | Mode::Peek | Mode::Open(Tab::Music)) {
                        need = true;
                    }
                }
                if need {
                    i.render();
                }
            });
            LRESULT(0)
        }
        WM_TIMER if wp.0 == T_SYS => {
            with(|i| {
                let now_ms = i.now_ms();
                let finished = i.timer.tick(now_ms);
                let (cpu, gpu, temp, ram, down, up) = sys::telemetry_snapshot();
                i.metrics.push_sample(cpu, gpu, ram);
                i.metrics.gpu_temp_c = temp;
                i.metrics.net_down_bps = down;
                i.metrics.net_up_bps = up;
                i.privacy = sys::privacy_mic_cam();
                let fs = taskbar::foreground_is_fullscreen(std::process::id(), i.mon.area);
                let max_top = !fs && taskbar::foreground_is_maximized(std::process::id());
                if max_top != i.maximized_top || fs != i.fullscreen_game {
                    i.fullscreen_game = fs;
                    i.maximized_top = max_top;
                    i.apply_geometry(true);
                }
                let sp = prefs::get();
                if sp.show_greeting && sp.greeting_rotate && i.last_glance_at.elapsed().as_secs() >= 8 {
                    i.glance_idx = (i.glance_idx + 1) % 3;
                    i.last_glance_at = Instant::now();
                }
                if finished
                    || matches!(
                        i.mode,
                        Mode::Open(Tab::Widgets) | Mode::Open(Tab::Calls) | Mode::Rest | Mode::Peek
                    )
                {
                    i.render();
                }
            });
            LRESULT(0)
        }
        WM_MOUSEMOVE => {
            let mx = (lp.0 & 0xFFFF) as i16 as f32;
            let my = ((lp.0 >> 16) & 0xFFFF) as i16 as f32;
            let mut pt = POINT::default();
            let _ = GetCursorPos(&mut pt);
            with(|i| {
                let (x0, y0, cw, ch) = i.cur_rect;
                let (lx, ly) = (mx - x0, my - y0);
                if let Some(drag) = &mut i.drag {
                    match *drag {
                        DragOp::Seek(sx, sw) => {
                            let frac = ((lx - sx) / sw.max(1.0)).clamp(0.0, 1.0);
                            i.pos_ms = (i.dur_ms as f32 * frac) as i64;
                            i.pos_at = Instant::now();
                            i.render();
                            return;
                        }
                        DragOp::Brightness(top, h) => {
                            let pct = ((1.0 - (ly - top) / h.max(1.0)).clamp(0.0, 1.0) * 100.0).round() as u8;
                            let _ = i.display.commands.send(brightness::Cmd::SetBrightness(pct));
                            return;
                        }
                        DragOp::Volume(top, h) => {
                            let vol = (1.0 - (ly - top) / h.max(1.0)).clamp(0.0, 1.0);
                            let _ = i.audio.commands.send(audio::Cmd::SetVolume(vol));
                            return;
                        }
                        DragOp::WidgetCandidate(kind, sx, sy, ref mut live) => {
                            let dist = ((pt.x - sx).pow(2) + (pt.y - sy).pow(2)) as f32;
                            let mut tl = POINT {
                                x: x0.round() as i32,
                                y: y0.round() as i32,
                            };
                            let _ = ClientToScreen(hwnd, &mut tl);
                            let outside = pt.y > tl.y + ch.round() as i32 - 6
                                || pt.y < tl.y + 6
                                || pt.x < tl.x - 6
                                || pt.x > tl.x + cw.round() as i32 + 6;
                            if *live || (dist > 256.0 && outside) {
                                *live = true;
                                desktop_widgets::preview_at(kind, pt.x - 105, pt.y - 50);
                            }
                            return;
                        }
                    }
                }
                if !i.tracking {
                    i.tracking = true;
                    let mut tme = TRACKMOUSEEVENT {
                        cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                        dwFlags: TME_LEAVE,
                        hwndTrack: hwnd,
                        dwHoverTime: 0,
                    };
                    let _ = TrackMouseEvent(&mut tme);
                    let _ = KillTimer(Some(hwnd), T_LEAVE);
                    i.dispatch(Event::Enter);
                    let s = prefs::get();
                    if s.open_trigger == np_core::settings::OpenTrigger::Hover {
                        let dwell = if s.dwell_long { 520 } else { 220 };
                        SetTimer(Some(hwnd), T_DWELL, dwell, None);
                    }
                }
            });
            LRESULT(0)
        }
        WM_MOUSELEAVE => {
            let _ = KillTimer(Some(hwnd), T_DWELL);
            with(|i| {
                i.tracking = false;
                if !prefs::get().sticky_open {
                    SetTimer(Some(hwnd), T_LEAVE, 240, None);
                }
            });
            LRESULT(0)
        }
        WM_LBUTTONDOWN => {
            let mx = (lp.0 & 0xFFFF) as i16 as f32;
            let my = ((lp.0 >> 16) & 0xFFFF) as i16 as f32;
            let mut pt = POINT::default();
            let _ = GetCursorPos(&mut pt);
            with(|i| {
                let (x0, y0, _, _) = i.cur_rect;
                let (lx, ly) = (mx - x0, my - y0);
                if matches!(i.mode, Mode::Rest | Mode::Peek) {
                    if let Some(r) = i.regions.iter().find(|r| r.hit(lx, ly)) {
                        let act = r.action;
                        i.execute_action(act, lx, ly);
                        return;
                    }
                    i.dispatch(Event::Click);
                    return;
                }
                if let Some(r) = i.regions.iter().find(|r| r.hit(lx, ly)) {
                    let act = r.action;
                    let dw = r.drag_widget;
                    match act {
                        Action::Seek(sx, sw) => {
                            i.drag = Some(DragOp::Seek(sx, sw));
                            SetCapture(hwnd);
                            i.execute_action(act, lx, ly);
                        }
                        Action::BrightnessBar(top, h) => {
                            i.drag = Some(DragOp::Brightness(top, h));
                            SetCapture(hwnd);
                            i.execute_action(act, lx, ly);
                        }
                        Action::VolumeBar(top, h) => {
                            i.drag = Some(DragOp::Volume(top, h));
                            SetCapture(hwnd);
                            i.execute_action(act, lx, ly);
                        }
                        _ if dw.is_some() => {
                            i.drag = Some(DragOp::WidgetCandidate(dw.unwrap(), pt.x, pt.y, false));
                            SetCapture(hwnd);
                        }
                        _ => i.execute_action(act, lx, ly),
                    }
                }
            });
            LRESULT(0)
        }
        WM_LBUTTONUP => {
            let mx = (lp.0 & 0xFFFF) as i16 as f32;
            let my = ((lp.0 >> 16) & 0xFFFF) as i16 as f32;
            let mut pt = POINT::default();
            let _ = GetCursorPos(&mut pt);
            with(|i| {
                if let Some(drag) = i.drag.take() {
                    let _ = ReleaseCapture();
                    if let DragOp::Seek(sx, sw) = drag {
                        i.execute_action(Action::Seek(sx, sw), mx - i.cur_rect.0, my - i.cur_rect.1);
                        let _ = i.media.commands.send(Command::SeekMs(i.pos_ms));
                    }
                    if let DragOp::WidgetCandidate(kind, sx, sy, was_live) = drag {
                        let dist = ((pt.x - sx).pow(2) + (pt.y - sy).pow(2)) as f32;
                        let (x0, y0, cw, ch) = i.cur_rect;
                        let mut tl = POINT {
                            x: x0.round() as i32,
                            y: y0.round() as i32,
                        };
                        let _ = ClientToScreen(hwnd, &mut tl);
                        let outside = pt.y > tl.y + ch.round() as i32 - 6
                            || pt.y < tl.y + 6
                            || pt.x < tl.x - 6
                            || pt.x > tl.x + cw.round() as i32 + 6;
                        if was_live || (dist > 256.0 && outside) {
                            desktop_widgets::spawn_at(kind, pt.x - 105, pt.y - 50);
                        } else {
                            let (lx, ly) = (mx - x0, my - y0);
                            if let Some(r) = i.regions.iter().find(|r| r.hit(lx, ly)) {
                                let act = r.action;
                                i.execute_action(act, lx, ly);
                            }
                        }
                    }
                }
            });
            LRESULT(0)
        }
        WM_MOUSEWHEEL => {
            let delta = ((wp.0 >> 16) as i16) as i32;
            with(|i| {
                if matches!(i.mode, Mode::Open(_)) {
                    let next = if delta < 0 {
                        i.active_tab.next()
                    } else {
                        i.active_tab.prev()
                    };
                    i.dispatch(Event::SelectTab(next));
                }
            });
            LRESULT(0)
        }
        WM_DROPFILES => {
            let hdrop = HDROP(wp.0 as *mut _);
            let count = DragQueryFileW(hdrop, 0xFFFF_FFFF, None);
            let mut dropped = Vec::new();
            for idx in 0..count {
                let len = DragQueryFileW(hdrop, idx, None);
                if len > 0 {
                    let mut buf = vec![0u16; len as usize + 1];
                    let got = DragQueryFileW(hdrop, idx, Some(&mut buf));
                    if got > 0 {
                        dropped.push(String::from_utf16_lossy(&buf[..got as usize]));
                    }
                }
            }
            DragFinish(hdrop);
            with(|i| {
                for p in dropped {
                    if !i.shelf.contains(&p) {
                        i.shelf.insert(0, p);
                    }
                }
                i.active_tab = Tab::Tools;
                i.dispatch(Event::Dropped);
            });
            LRESULT(0)
        }
        WM_RBUTTONUP => {
            let mx = (lp.0 & 0xFFFF) as i16 as f32;
            let my = ((lp.0 >> 16) & 0xFFFF) as i16 as f32;
            with(|i| {
                let (x0, y0, _, _) = i.cur_rect;
                let (lx, ly) = (mx - x0, my - y0);
                if let Some(Action::ToolSlot(idx)) = i.regions.iter().find(|r| r.hit(lx, ly)).map(|r| r.action) {
                    prefs::update(|s| {
                        if idx < s.tool_shortcuts.len() {
                            s.tool_shortcuts[idx].clear();
                        }
                    });
                    return;
                }
                if let Some(Action::LaunchAi(ai)) = i.regions.iter().find(|r| r.hit(lx, ly)).map(|r| r.action) {
                    i.execute_action(Action::SelectDefaultAi(ai), lx, ly);
                    return;
                }
                (i.open_settings_cb)();
            });
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}
