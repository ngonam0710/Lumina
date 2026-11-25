use crate::settings::{IslandEdge, OpenTrigger, Settings};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tab {
    Music,
    Widgets,
    Panels,
    Clipboard,
    Tools,
    Ai,
    Quick,
    Calls,
}

impl Tab {
    pub const ALL: [Tab; 8] = [
        Tab::Music,
        Tab::Widgets,
        Tab::Panels,
        Tab::Clipboard,
        Tab::Tools,
        Tab::Ai,
        Tab::Quick,
        Tab::Calls,
    ];

    pub fn label(self) -> &'static str {
        self.label_i18n(false)
    }

    pub fn label_i18n(self, vi: bool) -> &'static str {
        if vi {
            match self {
                Tab::Music => "Nhạc",
                Tab::Widgets => "Tiện ích",
                Tab::Panels => "Ghi chú",
                Tab::Clipboard => "Clipboard",
                Tab::Tools => "Công cụ",
                Tab::Ai => "AI",
                Tab::Quick => "Nhanh",
                Tab::Calls => "Cuộc gọi",
            }
        } else {
            match self {
                Tab::Music => "Music",
                Tab::Widgets => "Widgets",
                Tab::Panels => "Notes",
                Tab::Clipboard => "Clipboard",
                Tab::Tools => "Tools",
                Tab::Ai => "AI",
                Tab::Quick => "Quick",
                Tab::Calls => "Calls",
            }
        }
    }

    pub fn next(self) -> Tab {
        let i = Self::ALL.iter().position(|t| *t == self).unwrap_or(0);
        Self::ALL[(i + 1) % Self::ALL.len()]
    }

    pub fn prev(self) -> Tab {
        let i = Self::ALL.iter().position(|t| *t == self).unwrap_or(0);
        Self::ALL[(i + Self::ALL.len() - 1) % Self::ALL.len()]
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Rest,
    Peek,
    Open(Tab),
    Drop,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    Enter,
    Leave,
    Dwell,
    Click,
    SelectTab(Tab),
    ToggleOpen,
    DragEnter,
    DragLeave,
    Dropped,
    Escape,
}

pub fn step(mode: Mode, ev: Event, trigger: OpenTrigger, hovering: bool, active_tab: Tab) -> Mode {
    let rest = if hovering { Mode::Peek } else { Mode::Rest };
    match (mode, ev) {
        (_, Event::DragEnter) => Mode::Drop,
        (Mode::Drop, Event::DragLeave) => rest,
        (Mode::Drop, Event::Dropped) => Mode::Open(Tab::Tools),
        (Mode::Drop, _) => Mode::Drop,
        (Mode::Rest, Event::Enter) => Mode::Peek,
        (Mode::Peek, Event::Leave) => Mode::Rest,
        (Mode::Peek, Event::Dwell) if trigger == OpenTrigger::Hover => Mode::Open(active_tab),
        (Mode::Rest | Mode::Peek, Event::Click) => Mode::Open(active_tab),
        (Mode::Rest | Mode::Peek, Event::ToggleOpen) => Mode::Open(active_tab),
        (Mode::Open(_), Event::ToggleOpen) => Mode::Rest,
        (Mode::Open(_), Event::SelectTab(t)) => Mode::Open(t),
        (Mode::Open(_), Event::Leave) => Mode::Rest,
        (Mode::Open(_), Event::Escape) => rest,
        (m, _) => m,
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct IslandSize {
    pub w: f32,
    pub h: f32,
    pub radius: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Daypart {
    Morning,
    Noon,
    Afternoon,
    Evening,
}

pub fn daypart(hour: u32) -> Daypart {
    match hour % 24 {
        4..=10 => Daypart::Morning,
        11..=14 => Daypart::Noon,
        15..=17 => Daypart::Afternoon,
        _ => Daypart::Evening,
    }
}

pub fn time_greeting(hour: u32, vi: bool) -> &'static str {
    match (daypart(hour), vi) {
        (Daypart::Morning, true) => "Chào buổi sáng",
        (Daypart::Noon, true) => "Chào buổi trưa",
        (Daypart::Afternoon, true) => "Chào buổi chiều",
        (Daypart::Evening, true) => "Chào buổi tối",
        (Daypart::Morning, false) => "Good morning",
        (Daypart::Noon | Daypart::Afternoon, false) => "Good afternoon",
        (Daypart::Evening, false) => "Good evening",
    }
}

pub fn greeting_text(custom: &str, name: &str, hour: u32, vi: bool) -> String {
    let custom = custom.trim();
    if !custom.is_empty() {
        return custom.to_string();
    }
    let g = time_greeting(hour, vi);
    let name = name.trim();
    if name.is_empty() {
        g.to_string()
    } else {
        format!("{g}, {name}")
    }
}

pub fn size_for(mode: Mode, has_media: bool, game_strip: bool, fullscreen_game: bool, s: &Settings, dpi: f32) -> IslandSize {
    size_for_state(mode, has_media, game_strip, fullscreen_game, false, false, s, dpi)
}

#[allow(clippy::too_many_arguments)]
pub fn size_for_state(
    mode: Mode,
    has_media: bool,
    game_strip: bool,
    fullscreen_game: bool,
    maximized_top: bool,
    recording: bool,
    s: &Settings,
    dpi: f32,
) -> IslandSize {
    let open_k = s.open_scale();
    let corner_k = s.corner_scale();
    let side = matches!(s.edge, IslandEdge::Left | IslandEdge::Right);
    let (mut w, mut h, mut max_r): (f32, f32, f32) = match mode {
        Mode::Rest if maximized_top && s.glare_strip && !recording => (164.0, 5.0, 2.5),
        Mode::Rest if side => (32.0, if has_media || recording { 196.0 } else { 150.0 }, 16.0),
        Mode::Rest if fullscreen_game && game_strip => (356.0, 26.0, 13.0),
        Mode::Rest if has_media || recording => (246.0, 32.0, 16.0),
        Mode::Rest if s.show_greeting => (232.0, 30.0, 15.0),
        Mode::Rest => (176.0, 30.0, 15.0),
        Mode::Peek if side => (38.0, if has_media || recording { 216.0 } else { 172.0 }, 19.0),
        Mode::Peek if has_media || recording => (272.0, 36.0, 18.0),
        Mode::Peek if s.show_greeting => (264.0, 34.0, 17.0),
        Mode::Peek => (196.0, 34.0, 17.0),
        Mode::Open(Tab::Music) => (566.0 * open_k, 210.0 * open_k, 26.0),
        Mode::Open(Tab::Widgets) => (572.0 * open_k, 242.0 * open_k, 26.0),
        Mode::Open(Tab::Panels) => (566.0 * open_k, 230.0 * open_k, 26.0),
        Mode::Open(Tab::Clipboard) => (566.0 * open_k, 232.0 * open_k, 26.0),
        Mode::Open(Tab::Tools) => (566.0 * open_k, 236.0 * open_k, 26.0),
        Mode::Open(Tab::Ai) => (566.0 * open_k, 224.0 * open_k, 26.0),
        Mode::Open(Tab::Quick) => (566.0 * open_k, 222.0 * open_k, 26.0),
        Mode::Open(Tab::Calls) => (566.0 * open_k, 206.0 * open_k, 26.0),
        Mode::Drop => (540.0, 138.0, 26.0),
    };
    w = w.clamp(28.0, 860.0);
    h = h.clamp(4.0, 360.0);
    max_r = max_r.min(w.min(h) / 2.0);
    let r = (max_r * corner_k).max((2.0 * corner_k).min(h / 2.0));
    IslandSize {
        w: w * dpi,
        h: h * dpi,
        radius: r * dpi,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Idle,
    Running,
    Paused,
    Finished,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    Focus,
    Break,
}

#[derive(Clone, Debug)]
pub struct FocusTimer {
    pub focus_secs: u32,
    pub break_secs: u32,
    pub stage: Stage,
    pub phase: Phase,
    pub remaining_ms: i64,
    pub last_tick_ms: i64,
}

impl FocusTimer {
    pub fn new(focus_mins: u8, break_mins: u8) -> Self {
        let focus_secs = (focus_mins.clamp(1, 120) as u32) * 60;
        let break_secs = (break_mins.clamp(1, 60) as u32) * 60;
        Self {
            focus_secs,
            break_secs,
            stage: Stage::Focus,
            phase: Phase::Idle,
            remaining_ms: focus_secs as i64 * 1000,
            last_tick_ms: 0,
        }
    }

    pub fn sync_config(&mut self, focus_mins: u8, break_mins: u8) {
        self.focus_secs = (focus_mins.clamp(1, 120) as u32) * 60;
        self.break_secs = (break_mins.clamp(1, 60) as u32) * 60;
        if self.phase == Phase::Idle {
            self.remaining_ms = self.current_total_secs() as i64 * 1000;
        }
    }

    pub fn current_total_secs(&self) -> u32 {
        match self.stage {
            Stage::Focus => self.focus_secs,
            Stage::Break => self.break_secs,
        }
    }

    pub fn toggle(&mut self, now_ms: i64) {
        match self.phase {
            Phase::Idle | Phase::Finished => {
                if self.phase == Phase::Finished {
                    self.remaining_ms = self.current_total_secs() as i64 * 1000;
                }
                self.phase = Phase::Running;
                self.last_tick_ms = now_ms;
            }
            Phase::Running => {
                self.tick(now_ms);
                self.phase = Phase::Paused;
            }
            Phase::Paused => {
                self.phase = Phase::Running;
                self.last_tick_ms = now_ms;
            }
        }
    }

    pub fn reset(&mut self) {
        self.phase = Phase::Idle;
        self.stage = Stage::Focus;
        self.remaining_ms = self.focus_secs as i64 * 1000;
    }

    pub fn adjust_minutes(&mut self, delta_mins: i32) {
        let secs = (self.focus_secs as i32 + delta_mins * 60).clamp(60, 120 * 60) as u32;
        self.focus_secs = secs;
        if self.phase == Phase::Idle && self.stage == Stage::Focus {
            self.remaining_ms = secs as i64 * 1000;
        }
    }

    pub fn tick(&mut self, now_ms: i64) -> bool {
        if self.phase != Phase::Running {
            self.last_tick_ms = now_ms;
            return false;
        }
        let dt = (now_ms - self.last_tick_ms).max(0);
        self.last_tick_ms = now_ms;
        self.remaining_ms = (self.remaining_ms - dt).max(0);
        if self.remaining_ms == 0 {
            self.stage = match self.stage {
                Stage::Focus => Stage::Break,
                Stage::Break => Stage::Focus,
            };
            self.phase = Phase::Finished;
            self.remaining_ms = self.current_total_secs() as i64 * 1000;
            return true;
        }
        false
    }

    pub fn remaining_secs(&self) -> u32 {
        ((self.remaining_ms + 999) / 1000).max(0) as u32
    }

    pub fn elapsed_frac(&self) -> f32 {
        let total = (self.current_total_secs() as f32 * 1000.0).max(1.0);
        (1.0 - self.remaining_ms as f32 / total).clamp(0.0, 1.0)
    }

    pub fn fraction_done(&self) -> f32 {
        let total = (self.current_total_secs() as f32 * 1000.0).max(1.0);
        (1.0 - self.remaining_ms as f32 / total).clamp(0.0, 1.0)
    }

    pub fn mmss(&self) -> String {
        let s = self.remaining_secs();
        format!("{:02}:{:02}", s / 60, s % 60)
    }
}

pub mod calendar {
    pub fn is_leap(y: i32) -> bool {
        (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
    }

    pub fn days_in_month(y: i32, m: u32) -> u32 {
        match m {
            1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
            4 | 6 | 9 | 11 => 30,
            2 if is_leap(y) => 29,
            2 => 28,
            _ => 30,
        }
    }

    /// 0 = Sunday .. 6 = Saturday
    pub fn weekday(y: i32, m: u32, d: u32) -> u32 {
        const T: [i32; 12] = [0, 3, 2, 5, 0, 3, 5, 1, 4, 6, 2, 4];
        let y = if m < 3 { y - 1 } else { y };
        let wd = (y + y / 4 - y / 100 + y / 400 + T[(m.clamp(1, 12) - 1) as usize] + d as i32) % 7;
        ((wd + 7) % 7) as u32
    }

    pub fn day_of_year(y: i32, m: u32, d: u32) -> u32 {
        (1..m).map(|mm| days_in_month(y, mm)).sum::<u32>() + d
    }

    pub fn iso_week(y: i32, m: u32, d: u32) -> u32 {
        let doy = day_of_year(y, m, d) as i32;
        let wd = ((weekday(y, m, d) + 6) % 7 + 1) as i32; // Mon=1..Sun=7
        let w = (doy - wd + 10) / 7;
        if w < 1 {
            52
        } else {
            (w as u32).min(53)
        }
    }

    pub fn weekday_upper(wd: u32) -> &'static str {
        const NAMES: [&str; 7] = [
            "SUNDAY", "MONDAY", "TUESDAY", "WEDNESDAY", "THURSDAY", "FRIDAY", "SATURDAY",
        ];
        NAMES[(wd % 7) as usize]
    }

    pub fn weekday_short(wd: u32) -> &'static str {
        const NAMES: [&str; 7] = ["SUN", "MON", "TUE", "WED", "THU", "FRI", "SAT"];
        NAMES[(wd % 7) as usize]
    }

    pub fn month_upper(m: u32) -> &'static str {
        const NAMES: [&str; 12] = [
            "JANUARY", "FEBRUARY", "MARCH", "APRIL", "MAY", "JUNE", "JULY", "AUGUST",
            "SEPTEMBER", "OCTOBER", "NOVEMBER", "DECEMBER",
        ];
        NAMES[(m.clamp(1, 12) - 1) as usize]
    }

    pub fn month_short(m: u32) -> &'static str {
        const NAMES: [&str; 12] = [
            "JAN", "FEB", "MAR", "APR", "MAY", "JUN", "JUL", "AUG", "SEP", "OCT", "NOV", "DEC",
        ];
        NAMES[(m.clamp(1, 12) - 1) as usize]
    }
}

pub fn dominant_art_color(bgra: &[u8], w: u32, h: u32) -> Option<[u8; 3]> {
    let n = (w as usize) * (h as usize);
    if n == 0 || bgra.len() < n * 4 {
        return None;
    }
    let step = ((n as f32 / 2048.0).sqrt().ceil() as usize).max(1);
    let mut buckets = [0f32; 12];
    let mut sum = [[0f32; 3]; 12];
    let mut total = 0f32;
    for y in (0..h as usize).step_by(step) {
        for x in (0..w as usize).step_by(step) {
            let i = (y * w as usize + x) * 4;
            let (b, g, r, a) = (
                bgra[i] as f32,
                bgra[i + 1] as f32,
                bgra[i + 2] as f32,
                bgra[i + 3] as f32,
            );
            if a < 128.0 {
                continue;
            }
            total += 1.0;
            let (rf, gf, bf) = (r / 255.0, g / 255.0, b / 255.0);
            let max = rf.max(gf).max(bf);
            let min = rf.min(gf).min(bf);
            let d = max - min;
            let sat = if max > 0.0 { d / max } else { 0.0 };
            if sat < 0.25 || max < 0.20 {
                continue;
            }
            let hue = if d <= f32::EPSILON {
                0.0
            } else if max == rf {
                60.0 * (((gf - bf) / d).rem_euclid(6.0))
            } else if max == gf {
                60.0 * ((bf - rf) / d + 2.0)
            } else {
                60.0 * ((rf - gf) / d + 4.0)
            };
            let wgt = sat * sat * max;
            let k = ((hue / 360.0 * 12.0) as usize).min(11);
            buckets[k] += wgt;
            sum[k][0] += r * wgt;
            sum[k][1] += g * wgt;
            sum[k][2] += b * wgt;
        }
    }
    let (k, best) = buckets.iter().copied().enumerate().max_by(|a, b| a.1.total_cmp(&b.1))?;
    if total == 0.0 || best / total < 0.03 {
        return None;
    }
    let c = [sum[k][0] / best, sum[k][1] / best, sum[k][2] / best];
    let max = c[0].max(c[1]).max(c[2]).max(1.0);
    let f = 235.0 / max;
    Some([
        (c[0] * f).round().min(255.0) as u8,
        (c[1] * f).round().min(255.0) as u8,
        (c[2] * f).round().min(255.0) as u8,
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn island_state_transitions_work() {
        let m = step(Mode::Rest, Event::Enter, OpenTrigger::Hover, true, Tab::Music);
        assert_eq!(m, Mode::Peek);
        let m = step(m, Event::Dwell, OpenTrigger::Hover, true, Tab::Widgets);
        assert_eq!(m, Mode::Open(Tab::Widgets));
        let m = step(m, Event::SelectTab(Tab::Quick), OpenTrigger::Hover, true, Tab::Widgets);
        assert_eq!(m, Mode::Open(Tab::Quick));
        assert_eq!(step(m, Event::Leave, OpenTrigger::Hover, false, Tab::Quick), Mode::Rest);
    }

    #[test]
    fn timer_counts_down_and_transitions_to_break() {
        let mut t = FocusTimer::new(25, 5);
        assert_eq!(t.mmss(), "25:00");
        assert!(t.elapsed_frac() < 0.001);
        t.toggle(1000);
        assert!(!t.tick(61_000));
        assert_eq!(t.mmss(), "24:00");
        assert!(t.elapsed_frac() > 0.03);
        assert!(t.tick(1_501_000));
        assert_eq!(t.stage, Stage::Break);
        assert_eq!(t.phase, Phase::Finished);
        assert_eq!(t.mmss(), "05:00");
    }

    #[test]
    fn calendar_knows_thursday_nov_12_2026_is_week_46() {
        assert_eq!(calendar::weekday(2026, 11, 12), 4);
        assert_eq!(calendar::weekday_upper(4), "THURSDAY");
        assert_eq!(calendar::iso_week(2026, 11, 12), 46);
    }

    #[test]
    fn time_greeting_updates_by_hour_and_language() {
        assert_eq!(time_greeting(7, false), "Good morning");
        assert_eq!(time_greeting(12, false), "Good afternoon");
        assert_eq!(time_greeting(16, false), "Good afternoon");
        assert_eq!(time_greeting(22, false), "Good evening");
        assert_eq!(time_greeting(7, true), "Chào buổi sáng");
        assert_eq!(time_greeting(12, true), "Chào buổi trưa");
        assert_eq!(time_greeting(16, true), "Chào buổi chiều");
        assert_eq!(time_greeting(23, true), "Chào buổi tối");
        assert_eq!(greeting_text("", "An", 8, true), "Chào buổi sáng, An");
        assert_eq!(greeting_text("Yo", "An", 8, true), "Yo");
    }
}
