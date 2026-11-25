use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum SurfaceTheme {
    Black,
    PureBlack,
    Graphite,
    #[default]
    Glass,
    Acrylic,
    Rose,
    Lavender,
    Peach,
    Sakura,
    CandyGlass,
}

impl SurfaceTheme {
    pub const ALL: [SurfaceTheme; 10] = [
        SurfaceTheme::Black,
        SurfaceTheme::PureBlack,
        SurfaceTheme::Graphite,
        SurfaceTheme::Glass,
        SurfaceTheme::Acrylic,
        SurfaceTheme::Rose,
        SurfaceTheme::Lavender,
        SurfaceTheme::Peach,
        SurfaceTheme::Sakura,
        SurfaceTheme::CandyGlass,
    ];

    pub fn label(self) -> &'static str {
        self.label_i18n(false)
    }

    pub fn label_i18n(self, vi: bool) -> &'static str {
        if vi {
            match self {
                SurfaceTheme::Black => "Đen nhám",
                SurfaceTheme::PureBlack => "Đen tuyền",
                SurfaceTheme::Graphite => "Than chì",
                SurfaceTheme::Glass => "Kính trong",
                SurfaceTheme::Acrylic => "Acrylic",
                SurfaceTheme::Rose => "Hồng phấn",
                SurfaceTheme::Lavender => "Oải hương",
                SurfaceTheme::Peach => "Hồng đào",
                SurfaceTheme::Sakura => "Anh đào",
                SurfaceTheme::CandyGlass => "Kính kẹo",
            }
        } else {
            match self {
                SurfaceTheme::Black => "Black",
                SurfaceTheme::PureBlack => "Pure black",
                SurfaceTheme::Graphite => "Graphite",
                SurfaceTheme::Glass => "Glass",
                SurfaceTheme::Acrylic => "Acrylic",
                SurfaceTheme::Rose => "Rosé",
                SurfaceTheme::Lavender => "Lavender",
                SurfaceTheme::Peach => "Peach",
                SurfaceTheme::Sakura => "Sakura",
                SurfaceTheme::CandyGlass => "Candy glass",
            }
        }
    }

    pub fn is_see_through(self) -> bool {
        matches!(self, SurfaceTheme::Glass | SurfaceTheme::CandyGlass)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SurfaceSpec {
    pub blur_sigma: f32,
    pub saturation: f32,
    pub tint: [u8; 4],
    pub tint_bottom: [u8; 4],
    pub rim: [u8; 4],
    pub solid: [u8; 4],
    pub glow: [u8; 3],
    pub highlight: f32,
    pub liquify: bool,
    pub fg: [u8; 3],
    pub dark_text: bool,
}

pub fn surface_spec(theme: SurfaceTheme) -> SurfaceSpec {
    match theme {
        SurfaceTheme::Black => SurfaceSpec {
            blur_sigma: 0.0,
            saturation: 1.0,
            tint: [244, 16, 17, 22],
            tint_bottom: [248, 10, 11, 14],
            rim: [78, 255, 255, 255],
            solid: [246, 14, 15, 19],
            glow: [96, 158, 255],
            highlight: 0.35,
            liquify: false,
            fg: [255, 255, 255],
            dark_text: false,
        },
        SurfaceTheme::PureBlack => SurfaceSpec {
            blur_sigma: 0.0,
            saturation: 1.0,
            tint: [255, 0, 0, 0],
            tint_bottom: [255, 0, 0, 0],
            rim: [46, 255, 255, 255],
            solid: [255, 0, 0, 0],
            glow: [110, 176, 255],
            highlight: 0.12,
            liquify: false,
            fg: [255, 255, 255],
            dark_text: false,
        },
        SurfaceTheme::Graphite => SurfaceSpec {
            blur_sigma: 18.0,
            saturation: 1.22,
            tint: [192, 42, 45, 54],
            tint_bottom: [212, 26, 28, 35],
            rim: [115, 255, 255, 255],
            solid: [238, 38, 41, 48],
            glow: [148, 176, 224],
            highlight: 0.72,
            liquify: false,
            fg: [255, 255, 255],
            dark_text: false,
        },
        SurfaceTheme::Glass => SurfaceSpec {
            blur_sigma: 24.0,
            saturation: 1.85,
            tint: [68, 28, 34, 46],
            tint_bottom: [94, 16, 20, 28],
            rim: [205, 255, 255, 255],
            solid: [222, 26, 31, 42],
            glow: [118, 198, 255],
            highlight: 1.15,
            liquify: true,
            fg: [255, 255, 255],
            dark_text: false,
        },
        SurfaceTheme::Acrylic => SurfaceSpec {
            blur_sigma: 28.0,
            saturation: 1.55,
            tint: [148, 26, 29, 38],
            tint_bottom: [176, 18, 20, 27],
            rim: [130, 255, 255, 255],
            solid: [234, 24, 27, 34],
            glow: [108, 174, 255],
            highlight: 0.90,
            liquify: false,
            fg: [255, 255, 255],
            dark_text: false,
        },
        SurfaceTheme::Rose => SurfaceSpec {
            blur_sigma: 22.0,
            saturation: 1.60,
            tint: [118, 110, 38, 64],
            tint_bottom: [148, 66, 20, 42],
            rim: [170, 255, 208, 224],
            solid: [232, 74, 26, 46],
            glow: [255, 118, 162],
            highlight: 0.95,
            liquify: true,
            fg: [255, 246, 249],
            dark_text: false,
        },
        SurfaceTheme::Lavender => SurfaceSpec {
            blur_sigma: 22.0,
            saturation: 1.60,
            tint: [116, 76, 56, 124],
            tint_bottom: [148, 44, 34, 86],
            rim: [175, 226, 210, 255],
            solid: [232, 52, 40, 92],
            glow: [176, 146, 255],
            highlight: 0.95,
            liquify: true,
            fg: [248, 246, 255],
            dark_text: false,
        },
        SurfaceTheme::Peach => SurfaceSpec {
            blur_sigma: 22.0,
            saturation: 1.60,
            tint: [118, 122, 64, 44],
            tint_bottom: [148, 76, 38, 26],
            rim: [175, 255, 220, 188],
            solid: [232, 86, 46, 32],
            glow: [255, 166, 112],
            highlight: 0.95,
            liquify: true,
            fg: [255, 248, 244],
            dark_text: false,
        },
        SurfaceTheme::Sakura => SurfaceSpec {
            blur_sigma: 22.0,
            saturation: 1.65,
            tint: [114, 142, 68, 102],
            tint_bottom: [146, 88, 40, 68],
            rim: [185, 255, 218, 236],
            solid: [232, 102, 48, 74],
            glow: [255, 146, 198],
            highlight: 1.0,
            liquify: true,
            fg: [255, 247, 251],
            dark_text: false,
        },
        SurfaceTheme::CandyGlass => SurfaceSpec {
            blur_sigma: 22.0,
            saturation: 1.90,
            tint: [76, 56, 86, 124],
            tint_bottom: [108, 34, 52, 86],
            rim: [210, 218, 244, 255],
            solid: [222, 46, 68, 102],
            glow: [130, 210, 255],
            highlight: 1.20,
            liquify: true,
            fg: [255, 255, 255],
            dark_text: false,
        },
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum MotionMode {
    #[default]
    Fluid,
    Balanced,
    BatterySaver,
}

impl MotionMode {
    pub const ALL: [MotionMode; 3] = [MotionMode::Fluid, MotionMode::Balanced, MotionMode::BatterySaver];

    pub fn label(self) -> &'static str {
        self.label_i18n(false)
    }

    pub fn label_i18n(self, vi: bool) -> &'static str {
        if vi {
            match self {
                MotionMode::Fluid => "Mượt mà",
                MotionMode::Balanced => "Cân bằng",
                MotionMode::BatterySaver => "Tiết kiệm pin",
            }
        } else {
            match self {
                MotionMode::Fluid => "Fluid",
                MotionMode::Balanced => "Balanced",
                MotionMode::BatterySaver => "Battery saver",
            }
        }
    }

    pub fn period_scale(self) -> f32 {
        match self {
            MotionMode::Fluid => 1.0,
            MotionMode::Balanced => 0.75,
            MotionMode::BatterySaver => 0.0,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum VisStyle {
    #[default]
    Bars,
    Wave,
    Dots,
}

impl VisStyle {
    pub const ALL: [VisStyle; 3] = [VisStyle::Bars, VisStyle::Wave, VisStyle::Dots];

    pub fn label(self) -> &'static str {
        self.label_i18n(false)
    }

    pub fn label_i18n(self, vi: bool) -> &'static str {
        if vi {
            match self {
                VisStyle::Bars => "Cột",
                VisStyle::Wave => "Sóng",
                VisStyle::Dots => "Chấm",
            }
        } else {
            match self {
                VisStyle::Bars => "Bars",
                VisStyle::Wave => "Wave",
                VisStyle::Dots => "Dots",
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum IslandShape {
    #[default]
    Pill,
    Notch,
}

impl IslandShape {
    pub const ALL: [IslandShape; 2] = [IslandShape::Pill, IslandShape::Notch];

    pub fn label(self) -> &'static str {
        self.label_i18n(false)
    }

    pub fn label_i18n(self, vi: bool) -> &'static str {
        if vi {
            match self {
                IslandShape::Pill => "Viên thuốc",
                IslandShape::Notch => "Tai thỏ",
            }
        } else {
            match self {
                IslandShape::Pill => "Island",
                IslandShape::Notch => "Notch",
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum OpenTrigger {
    #[default]
    Hover,
    Click,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum PowerMode {
    Battery,
    #[default]
    Balanced,
    Performance,
}

impl PowerMode {
    pub const ALL: [PowerMode; 3] = [PowerMode::Battery, PowerMode::Balanced, PowerMode::Performance];
    pub fn label(self) -> &'static str {
        self.label_i18n(false)
    }

    pub fn label_i18n(self, vi: bool) -> &'static str {
        if vi {
            match self {
                PowerMode::Battery => "Tiết kiệm",
                PowerMode::Balanced => "Cân bằng",
                PowerMode::Performance => "Hiệu năng",
            }
        } else {
            match self {
                PowerMode::Battery => "Battery",
                PowerMode::Balanced => "Balanced",
                PowerMode::Performance => "Performance",
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum IslandEdge {
    #[default]
    Top,
    Left,
    Right,
}

impl IslandEdge {
    pub const ALL: [IslandEdge; 3] = [IslandEdge::Top, IslandEdge::Left, IslandEdge::Right];

    pub fn label(self) -> &'static str {
        self.label_i18n(false)
    }

    pub fn label_i18n(self, vi: bool) -> &'static str {
        if vi {
            match self {
                IslandEdge::Top => "Trên",
                IslandEdge::Left => "Trái",
                IslandEdge::Right => "Phải",
            }
        } else {
            match self {
                IslandEdge::Top => "Top",
                IslandEdge::Left => "Left",
                IslandEdge::Right => "Right",
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum OpenTarget {
    #[default]
    LastTab,
    Music,
    Widgets,
}

impl OpenTarget {
    pub const ALL: [OpenTarget; 3] = [OpenTarget::LastTab, OpenTarget::Music, OpenTarget::Widgets];

    pub fn label(self) -> &'static str {
        self.label_i18n(false)
    }

    pub fn label_i18n(self, vi: bool) -> &'static str {
        if vi {
            match self {
                OpenTarget::LastTab => "Tab vừa mở",
                OpenTarget::Music => "Nhạc",
                OpenTarget::Widgets => "Tiện ích",
            }
        } else {
            match self {
                OpenTarget::LastTab => "Last tab",
                OpenTarget::Music => "Music",
                OpenTarget::Widgets => "Widgets",
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum AiProvider {
    #[default]
    Antigravity,
    ChatGpt,
    Claude,
    Gemini,
    Perplexity,
}

impl AiProvider {
    pub const ALL: [AiProvider; 5] = [
        AiProvider::Antigravity,
        AiProvider::ChatGpt,
        AiProvider::Claude,
        AiProvider::Gemini,
        AiProvider::Perplexity,
    ];

    pub fn label(self) -> &'static str {
        match self {
            AiProvider::Antigravity => "Antigravity",
            AiProvider::ChatGpt => "ChatGPT",
            AiProvider::Claude => "Claude",
            AiProvider::Gemini => "Gemini",
            AiProvider::Perplexity => "Perplexity",
        }
    }

    pub fn url(self) -> &'static str {
        match self {
            AiProvider::Antigravity => "https://antigravity.google",
            AiProvider::ChatGpt => "https://chatgpt.com",
            AiProvider::Claude => "https://claude.ai",
            AiProvider::Gemini => "https://gemini.google.com",
            AiProvider::Perplexity => "https://www.perplexity.ai",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Lang {
    #[default]
    System,
    En,
    Vi,
}

impl Lang {
    pub const ALL: [Lang; 3] = [Lang::System, Lang::En, Lang::Vi];
    pub fn label(self) -> &'static str {
        self.label_i18n(false)
    }

    pub fn label_i18n(self, vi: bool) -> &'static str {
        match self {
            Lang::System => if vi { "Theo Windows" } else { "Same as Windows" },
            Lang::En => "English",
            Lang::Vi => "Tiếng Việt",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DayClockConfig {
    pub enabled: bool,
    pub size_pct: u8,
    pub color: [u8; 3],
    pub x: i32,
    pub y: i32,
    pub custom_pos: bool,
}

impl Default for DayClockConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            size_pct: 100,
            color: [255, 255, 255],
            x: 110,
            y: 260,
            custom_pos: false,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub lang: Lang,
    pub surface: SurfaceTheme,
    pub motion: MotionMode,
    pub visualizer: VisStyle,
    pub shape: IslandShape,
    pub edge: IslandEdge,
    pub open_trigger: OpenTrigger,
    pub open_to: OpenTarget,
    pub open_size_pct: u8,
    pub corner_pct: u8,
    pub see_through_pct: u8,
    pub island_enabled: bool,
    pub island_top_gap: u8,
    pub island_glow: bool,
    pub glare_strip: bool,
    pub show_greeting: bool,
    pub greeting_rotate: bool,
    pub day_clock_greeting: bool,
    pub greet_name: String,
    pub greet_text: String,
    pub hide_under_fullscreen: bool,
    pub high_contrast: bool,
    pub dwell_long: bool,
    pub sticky_open: bool,
    pub text_scale_pct: u8,
    pub taskbar_enabled: bool,
    pub taskbar_show_all_tray: bool,
    pub taskbar_auto_reveal: bool,
    pub taskbar_autohide_side: bool,
    pub taskbar_show_now_playing: bool,
    pub taskbar_dock_zoom_pct: u8,
    pub day_clock: DayClockConfig,
    pub notifications_in_island: bool,
    pub game_strip: bool,
    pub fps_counter: bool,
    pub volume_in_island: bool,
    pub clipboard_history: bool,
    pub calls_mic_banner: bool,
    pub stream_privacy_shield: bool,
    pub show_stream_badge: bool,
    pub show_ai_tab: bool,
    pub show_calls_tab: bool,
    pub default_ai: AiProvider,
    pub ai_include_clipboard: bool,
    pub notes: Vec<String>,
    pub focus_mins: u8,
    pub break_mins: u8,
    pub weather_city: String,
    pub weather_lat: f64,
    pub weather_lon: f64,
    pub accent: [u8; 3],
    pub clock_24h: bool,
    pub autostart: bool,
    pub hotkeys_enabled: bool,
    pub pinned_apps: Vec<String>,
    pub tool_shortcuts: [String; 5],
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            lang: Lang::System,
            surface: SurfaceTheme::Glass,
            motion: MotionMode::Fluid,
            visualizer: VisStyle::Bars,
            shape: IslandShape::Pill,
            edge: IslandEdge::Top,
            open_trigger: OpenTrigger::Hover,
            open_to: OpenTarget::LastTab,
            open_size_pct: 100,
            corner_pct: 100,
            see_through_pct: 100,
            island_enabled: true,
            island_top_gap: 6,
            island_glow: true,
            glare_strip: true,
            show_greeting: true,
            greeting_rotate: true,
            day_clock_greeting: false,
            greet_name: String::new(),
            greet_text: String::new(),
            hide_under_fullscreen: true,
            high_contrast: false,
            dwell_long: false,
            sticky_open: false,
            text_scale_pct: 100,
            taskbar_enabled: false,
            taskbar_show_all_tray: true,
            taskbar_auto_reveal: false,
            taskbar_autohide_side: true,
            taskbar_show_now_playing: true,
            taskbar_dock_zoom_pct: 145,
            day_clock: DayClockConfig::default(),
            notifications_in_island: true,
            game_strip: true,
            fps_counter: true,
            volume_in_island: true,
            clipboard_history: true,
            calls_mic_banner: true,
            stream_privacy_shield: false,
            show_stream_badge: true,
            show_ai_tab: true,
            show_calls_tab: true,
            default_ai: AiProvider::Antigravity,
            ai_include_clipboard: true,
            notes: vec![
                "Drag any widget card onto desktop".to_string(),
                "Drop files on Island for Phone QR".to_string(),
            ],
            focus_mins: 25,
            break_mins: 5,
            weather_city: "Ho Chi Minh City".to_string(),
            weather_lat: 10.8231,
            weather_lon: 106.6297,
            accent: [232, 68, 78],
            clock_24h: false,
            autostart: false,
            hotkeys_enabled: true,
            pinned_apps: Vec::new(),
            tool_shortcuts: [
                String::new(),
                String::new(),
                String::new(),
                String::new(),
                String::new(),
            ],
        }
    }
}

impl Settings {
    pub fn from_json(text: &str) -> Self {
        serde_json::from_str(text).unwrap_or_default()
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_default()
    }

    pub fn corner_scale(&self) -> f32 {
        self.corner_pct.min(100) as f32 / 100.0
    }

    pub fn open_scale(&self) -> f32 {
        self.open_size_pct.clamp(80, 125) as f32 / 100.0
    }

    pub fn text_scale(&self) -> f32 {
        self.text_scale_pct.clamp(90, 125) as f32 / 100.0
    }
}

pub fn tr(lang: Lang, en: &'static str, vi: &'static str) -> &'static str {
    match lang {
        Lang::System | Lang::En => en,
        Lang::Vi => vi,
    }
}

pub fn hsv_to_rgb(h_deg: f32, s: f32, v: f32) -> [u8; 3] {
    let h = h_deg.rem_euclid(360.0) / 60.0;
    let s = s.clamp(0.0, 1.0);
    let v = v.clamp(0.0, 1.0);
    let c = v * s;
    let x = c * (1.0 - (h % 2.0 - 1.0).abs());
    let m = v - c;
    let (r, g, b) = if h < 1.0 {
        (c, x, 0.0)
    } else if h < 2.0 {
        (x, c, 0.0)
    } else if h < 3.0 {
        (0.0, c, x)
    } else if h < 4.0 {
        (0.0, x, c)
    } else if h < 5.0 {
        (x, 0.0, c)
    } else {
        (c, 0.0, x)
    };
    [
        ((r + m) * 255.0).round().clamp(0.0, 255.0) as u8,
        ((g + m) * 255.0).round().clamp(0.0, 255.0) as u8,
        ((b + m) * 255.0).round().clamp(0.0, 255.0) as u8,
    ]
}

pub fn rgb_to_hsv(rgb: [u8; 3]) -> (f32, f32, f32) {
    let (r, g, b) = (rgb[0] as f32 / 255.0, rgb[1] as f32 / 255.0, rgb[2] as f32 / 255.0);
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let d = max - min;
    let h = if d <= 1e-6 {
        0.0
    } else if max == r {
        60.0 * (((g - b) / d).rem_euclid(6.0))
    } else if max == g {
        60.0 * ((b - r) / d + 2.0)
    } else {
        60.0 * ((r - g) / d + 4.0)
    };
    let s = if max <= 1e-6 { 0.0 } else { d / max };
    (h, s, max)
}

pub fn hex_color(rgb: [u8; 3]) -> String {
    format!("#{:02X}{:02X}{:02X}", rgb[0], rgb[1], rgb[2])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_round_trip_preserves_fields() {
        let mut s = Settings::default();
        s.surface = SurfaceTheme::CandyGlass;
        s.day_clock.color = [120, 200, 255];
        s.taskbar_enabled = true;
        s.open_size_pct = 115;
        s.edge = IslandEdge::Right;
        s.default_ai = AiProvider::Gemini;
        let json = s.to_json();
        let back = Settings::from_json(&json);
        assert_eq!(back, s);
    }

    #[test]
    fn hsv_rgb_round_trip_works() {
        for rgb in [[255, 255, 255], [245, 92, 92], [110, 188, 255], [176, 146, 255]] {
            let (h, s, v) = rgb_to_hsv(rgb);
            let back = hsv_to_rgb(h, s, v);
            for c in 0..3 {
                assert!((rgb[c] as i32 - back[c] as i32).abs() <= 2);
            }
        }
    }

    #[test]
    fn all_ten_surface_themes_have_valid_specs() {
        for t in SurfaceTheme::ALL {
            let spec = surface_spec(t);
            assert!(spec.blur_sigma >= 0.0);
            assert!(spec.saturation >= 1.0);
            assert!(spec.solid[0] >= 200);
            assert!(!t.label_i18n(true).is_empty());
        }
        assert!(SurfaceTheme::Glass.is_see_through());
        assert!(!SurfaceTheme::Black.is_see_through());
        assert_eq!(Lang::System.label_i18n(true), "Theo Windows");
        assert_eq!(IslandEdge::Top.label_i18n(true), "Trên");
        assert_eq!(OpenTarget::Widgets.label_i18n(true), "Tiện ích");
    }
}
