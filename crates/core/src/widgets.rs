use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum WidgetKind {
    Calendar,
    FocusTimer,
    Weather,
    SystemMonitor,
}

impl WidgetKind {
    pub const ALL: [WidgetKind; 4] = [
        WidgetKind::Calendar,
        WidgetKind::FocusTimer,
        WidgetKind::Weather,
        WidgetKind::SystemMonitor,
    ];

    pub fn label(self) -> &'static str {
        self.label_i18n(false)
    }

    pub fn label_i18n(self, vi: bool) -> &'static str {
        if vi {
            match self {
                WidgetKind::Calendar => "Lịch",
                WidgetKind::FocusTimer => "Đồng hồ tập trung",
                WidgetKind::Weather => "Thời tiết",
                WidgetKind::SystemMonitor => "Giám sát hệ thống",
            }
        } else {
            match self {
                WidgetKind::Calendar => "Calendar",
                WidgetKind::FocusTimer => "Focus timer",
                WidgetKind::Weather => "Weather",
                WidgetKind::SystemMonitor => "System monitor",
            }
        }
    }

    pub fn desktop_size(self, dpi: f32) -> (f32, f32) {
        match self {
            WidgetKind::Calendar => (224.0 * dpi, 148.0 * dpi),
            WidgetKind::FocusTimer => (210.0 * dpi, 108.0 * dpi),
            WidgetKind::Weather => (210.0 * dpi, 88.0 * dpi),
            WidgetKind::SystemMonitor => (230.0 * dpi, 88.0 * dpi),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DesktopWidget {
    pub kind: WidgetKind,
    pub x: i32,
    pub y: i32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct WeatherData {
    pub temp_c: i32,
    pub condition: String,
    pub high_c: i32,
    pub low_c: i32,
    pub is_night: bool,
    pub valid: bool,
}

impl Default for WeatherData {
    fn default() -> Self {
        Self {
            temp_c: 28,
            condition: "Partly cloudy".to_string(),
            high_c: 30,
            low_c: 23,
            is_night: false,
            valid: false,
        }
    }
}

#[derive(Clone, Debug)]
pub struct SystemMetrics {
    pub cpu_pct: u8,
    pub gpu_pct: u8,
    pub gpu_temp_c: Option<u8>,
    pub ram_pct: u8,
    pub net_down_bps: u64,
    pub net_up_bps: u64,
    pub cpu_hist: VecDeque<u8>,
    pub gpu_hist: VecDeque<u8>,
    pub ram_hist: VecDeque<u8>,
}

impl Default for SystemMetrics {
    fn default() -> Self {
        Self {
            cpu_pct: 0,
            gpu_pct: 0,
            gpu_temp_c: None,
            ram_pct: 0,
            net_down_bps: 0,
            net_up_bps: 0,
            cpu_hist: VecDeque::from(vec![0; 24]),
            gpu_hist: VecDeque::from(vec![0; 24]),
            ram_hist: VecDeque::from(vec![0; 24]),
        }
    }
}

impl SystemMetrics {
    pub fn push_sample(&mut self, cpu: u8, gpu: u8, ram: u8) {
        self.cpu_pct = cpu.min(100);
        self.gpu_pct = gpu.min(100);
        self.ram_pct = ram.min(100);
        for (hist, val) in [
            (&mut self.cpu_hist, self.cpu_pct),
            (&mut self.gpu_hist, self.gpu_pct),
            (&mut self.ram_hist, self.ram_pct),
        ] {
            if hist.len() >= 24 {
                hist.pop_front();
            }
            hist.push_back(val);
        }
    }
}

pub fn format_speed(bps: u64) -> String {
    if bps < 1024 {
        format!("{bps} B/s")
    } else if bps < 1024 * 1024 {
        format!("{:.1} KB/s", bps as f64 / 1024.0)
    } else if bps < 1024 * 1024 * 1024 {
        format!("{:.1} MB/s", bps as f64 / (1024.0 * 1024.0))
    } else {
        format!("{:.1} GB/s", bps as f64 / (1024.0 * 1024.0 * 1024.0))
    }
}

/// Renders Anurati-inspired futuristic geometric uppercase vector glyphs into premultiplied BGRA.
pub mod anurati {
    type Pt = (f32, f32);

    #[derive(Clone)]
    enum Shape {
        Poly(Vec<Pt>),
        Rect(f32, f32, f32, f32),
        RingArc {
            cx: f32,
            cy: f32,
            ro: f32,
            ri: f32,
            rx_scale: f32,
            ry_scale: f32,
            a0: f32,
            a1: f32,
        },
    }

    fn advance(ch: char) -> f32 {
        match ch {
            'I' => 24.0,
            'J' => 56.0,
            'L' => 64.0,
            'F' => 66.0,
            'P' | 'B' | 'E' | 'K' => 70.0,
            'T' | 'S' => 72.0,
            'U' | 'R' | 'Z' => 74.0,
            'H' | 'D' | 'Y' | 'C' | 'N' | 'V' => 76.0,
            'G' | 'O' | 'Q' | 'X' => 80.0,
            'A' => 82.0,
            'M' => 86.0,
            'W' => 96.0,
            ' ' => 36.0,
            _ => 68.0,
        }
    }

    fn glyph_shapes(ch: char) -> Vec<Shape> {
        const S: f32 = 14.0;
        let arc_band = |cx: f32, cy: f32, r_mid: f32, half_t: f32, a_start_deg: f32, a_end_deg: f32| -> Shape {
            Shape::RingArc {
                cx,
                cy,
                ro: r_mid + half_t,
                ri: r_mid - half_t,
                rx_scale: 1.0,
                ry_scale: 1.0,
                a0: a_start_deg.to_radians(),
                a1: a_end_deg.to_radians(),
            }
        };
        match ch {
            'T' => vec![
                Shape::Poly(vec![(0.0, 0.0), (72.0, 0.0), (60.0, S), (12.0, S)]),
                Shape::Rect(29.0, S - 1.0, S, 100.0 - (S - 1.0)),
            ],
            'H' => vec![
                Shape::Rect(0.0, 43.0, S, 57.0),
                Shape::Rect(0.0, 43.0, 54.0, S),
                Shape::Rect(62.0, 0.0, S, 100.0),
            ],
            'U' => vec![
                Shape::Rect(0.0, 0.0, S, 63.0),
                Shape::Rect(60.0, 35.0, S, 28.0),
                arc_band(37.0, 63.0, 30.0, S / 2.0, 0.0, 180.0),
            ],
            'R' => vec![
                Shape::Rect(S, 0.0, 30.0, S),
                arc_band(44.0, 30.0, 23.0, S / 2.0, -90.0, 90.0),
                Shape::Rect(12.0, 46.0, 32.0, S),
                Shape::Rect(12.0, 46.0, S, 54.0),
                Shape::Poly(vec![(34.0, 59.0), (49.0, 59.0), (74.0, 100.0), (58.0, 100.0)]),
            ],
            'S' => vec![
                Shape::Rect(28.5, 0.0, 41.5, S),
                arc_band(28.5, 28.5, 21.5, S / 2.0, 90.0, 270.0),
                Shape::Rect(28.5, 43.0, 15.0, S),
                arc_band(43.5, 71.5, 21.5, S / 2.0, -90.0, 90.0),
                Shape::Poly(vec![(14.0, 86.0), (43.5, 86.0), (43.5, 100.0), (0.0, 100.0)]),
            ],
            'D' => vec![
                Shape::Poly(vec![(0.0, 0.0), (26.0, 0.0), (26.0, S), (S, S)]),
                arc_band(26.0, 50.0, 43.0, S / 2.0, -90.0, 90.0),
                Shape::Poly(vec![(S, 100.0 - S), (26.0, 100.0 - S), (26.0, 100.0), (0.0, 100.0)]),
            ],
            'A' => vec![
                Shape::Poly(vec![
                    (35.0, 0.0),
                    (47.0, 0.0),
                    (69.4, 63.0),
                    (54.4, 63.0),
                    (41.0, 21.0),
                    (15.0, 100.0),
                    (0.0, 100.0),
                ]),
                Shape::Poly(vec![(58.2, 74.0), (73.2, 74.0), (82.0, 100.0), (67.0, 100.0)]),
            ],
            'Y' => vec![
                Shape::Poly(vec![(61.0, 0.0), (76.0, 0.0), (35.0, 100.0), (20.0, 100.0)]),
                Shape::Poly(vec![(0.0, 0.0), (15.0, 0.0), (38.0, 48.0), (23.0, 48.0)]),
            ],
            'M' => vec![
                Shape::Poly(vec![
                    (0.0, 0.0),
                    (16.0, 0.0),
                    (47.0, 66.0),
                    (33.0, 66.0),
                    (14.0, 24.0),
                    (14.0, 100.0),
                    (0.0, 100.0),
                ]),
                Shape::Poly(vec![
                    (52.0, 56.0),
                    (70.0, 0.0),
                    (86.0, 0.0),
                    (86.0, 100.0),
                    (72.0, 100.0),
                    (72.0, 24.0),
                    (58.0, 70.0),
                ]),
            ],
            'O' => vec![Shape::RingArc {
                cx: 40.0,
                cy: 50.0,
                ro: 44.0,
                ri: 30.0,
                rx_scale: 40.0 / 44.0,
                ry_scale: 50.0 / 44.0,
                a0: (-112.0f32).to_radians(),
                a1: (218.0f32).to_radians(),
            }],
            'N' => vec![
                Shape::Rect(0.0, 0.0, S, 100.0),
                Shape::Poly(vec![(19.0, 10.0), (33.0, 10.0), (57.0, 90.0), (43.0, 90.0)]),
                Shape::Rect(62.0, 0.0, S, 100.0),
            ],
            'E' => vec![
                Shape::Poly(vec![(0.0, 0.0), (70.0, 0.0), (56.0, S), (S, S)]),
                Shape::Rect(10.0, 43.0, 46.0, S),
                Shape::Rect(10.0, 43.0, S, 57.0),
                Shape::Rect(10.0, 100.0 - S, 60.0, S),
            ],
            'F' => vec![
                Shape::Poly(vec![(0.0, 0.0), (66.0, 0.0), (52.0, S), (S, S)]),
                Shape::Rect(10.0, 23.0, S, 77.0),
                Shape::Rect(10.0, 43.0, 44.0, S),
            ],
            'I' => vec![Shape::Rect(5.0, 0.0, S, S), Shape::Rect(5.0, 23.0, S, 77.0)],
            'C' => vec![
                Shape::Poly(vec![(72.0, S), (44.0, S), (44.0, 0.0), (60.0, 0.0)]),
                arc_band(44.0, 50.0, 43.0, S / 2.0, 90.0, 270.0),
                Shape::Poly(vec![(44.0, 100.0 - S), (72.0, 100.0 - S), (60.0, 100.0), (44.0, 100.0)]),
            ],
            'G' => vec![
                Shape::Poly(vec![(74.0, S), (44.0, S), (44.0, 0.0), (62.0, 0.0)]),
                arc_band(44.0, 50.0, 43.0, S / 2.0, 90.0, 270.0),
                Shape::Rect(44.0, 100.0 - S, 36.0, S),
                Shape::Rect(66.0, 44.0, S, 56.0),
                Shape::Rect(44.0, 44.0, 36.0, S),
            ],
            'L' => vec![Shape::Poly(vec![
                (0.0, 0.0),
                (S, 0.0),
                (S, 100.0 - S),
                (64.0, 100.0 - S),
                (50.0, 100.0),
                (0.0, 100.0),
            ])],
            'V' => vec![
                Shape::Poly(vec![(0.0, 0.0), (S, 0.0), (45.0, 100.0), (31.0, 100.0)]),
                Shape::Poly(vec![(62.0, 0.0), (76.0, 0.0), (48.0, 72.0), (34.0, 72.0)]),
            ],
            'W' => vec![
                Shape::Poly(vec![
                    (0.0, 0.0),
                    (15.0, 0.0),
                    (29.0, 74.0),
                    (41.0, 22.0),
                    (55.0, 22.0),
                    (34.0, 100.0),
                    (20.0, 100.0),
                ]),
                Shape::Poly(vec![
                    (46.0, 50.0),
                    (67.0, 100.0),
                    (81.0, 100.0),
                    (96.0, 0.0),
                    (81.0, 0.0),
                    (72.0, 72.0),
                    (57.0, 36.0),
                ]),
            ],
            'P' => vec![
                Shape::Rect(S, 0.0, 28.0, S),
                arc_band(42.0, 30.0, 23.0, S / 2.0, -90.0, 90.0),
                Shape::Rect(10.0, 46.0, 32.0, S),
                Shape::Rect(10.0, 46.0, S, 54.0),
            ],
            'B' => vec![
                Shape::Poly(vec![
                    (0.0, 0.0),
                    (44.0, 0.0),
                    (64.0, 24.0),
                    (44.0, 48.0),
                    (12.0, 48.0),
                    (12.0, 34.0),
                    (40.0, 34.0),
                    (48.0, 24.0),
                    (40.0, S),
                    (S, S),
                ]),
                Shape::Poly(vec![
                    (10.0, 46.0),
                    (48.0, 46.0),
                    (70.0, 73.0),
                    (48.0, 100.0),
                    (10.0, 100.0),
                ]),
            ],
            'J' => vec![
                Shape::Rect(42.0, 0.0, S, 70.0),
                arc_band(28.0, 70.0, 21.0, S / 2.0, 0.0, 180.0),
            ],
            'K' => vec![
                Shape::Rect(0.0, 0.0, S, 100.0),
                Shape::Poly(vec![(55.0, 0.0), (70.0, 0.0), (30.0, 50.0), (S + 2.0, 50.0)]),
                Shape::Poly(vec![(22.0, 56.0), (37.0, 56.0), (70.0, 100.0), (54.0, 100.0)]),
            ],
            'Z' => vec![
                Shape::Poly(vec![(0.0, 0.0), (74.0, 0.0), (60.0, S), (14.0, S)]),
                Shape::Poly(vec![(58.0, 12.0), (74.0, 12.0), (16.0, 88.0), (0.0, 88.0)]),
                Shape::Poly(vec![(S, 86.0), (74.0, 86.0), (74.0, 100.0), (0.0, 100.0)]),
            ],
            'Q' => vec![
                Shape::RingArc {
                    cx: 40.0,
                    cy: 50.0,
                    ro: 44.0,
                    ri: 30.0,
                    rx_scale: 40.0 / 44.0,
                    ry_scale: 50.0 / 44.0,
                    a0: 0.0,
                    a1: std::f32::consts::TAU,
                },
                Shape::Poly(vec![(48.0, 68.0), (62.0, 68.0), (80.0, 100.0), (66.0, 100.0)]),
            ],
            'X' => vec![
                Shape::Poly(vec![(0.0, 0.0), (16.0, 0.0), (80.0, 100.0), (64.0, 100.0)]),
                Shape::Poly(vec![(64.0, 0.0), (80.0, 0.0), (50.0, 44.0), (34.0, 44.0)]),
                Shape::Poly(vec![(30.0, 56.0), (46.0, 56.0), (16.0, 100.0), (0.0, 100.0)]),
            ],
            _ => Vec::new(),
        }
    }

    fn point_in_poly(p: Pt, pts: &[Pt]) -> bool {
        let mut inside = false;
        let mut j = pts.len().saturating_sub(1);
        for i in 0..pts.len() {
            let (pi, pj) = (pts[i], pts[j]);
            if (pi.1 > p.1) != (pj.1 > p.1)
                && p.0 < (pj.0 - pi.0) * (p.1 - pi.1) / (pj.1 - pi.1) + pi.0
            {
                inside = !inside;
            }
            j = i;
        }
        inside
    }

    fn angle_in_arc(ang: f32, a0: f32, a1: f32) -> bool {
        let tau = std::f32::consts::TAU;
        let span = (a1 - a0).rem_euclid(tau);
        if (a1 - a0).abs() >= tau - 0.001 {
            return true;
        }
        let d = (ang - a0).rem_euclid(tau);
        d <= span
    }

    fn hit_shape(s: &Shape, p: Pt) -> bool {
        match s {
            Shape::Rect(x, y, w, h) => p.0 >= *x && p.0 <= x + w && p.1 >= *y && p.1 <= y + h,
            Shape::Poly(pts) => point_in_poly(p, pts),
            Shape::RingArc {
                cx,
                cy,
                ro,
                ri,
                rx_scale,
                ry_scale,
                a0,
                a1,
            } => {
                let dx = (p.0 - cx) / rx_scale;
                let dy = (p.1 - cy) / ry_scale;
                let d = (dx * dx + dy * dy).sqrt();
                if d < *ri || d > *ro {
                    return false;
                }
                let ang = dy.atan2(dx);
                angle_in_arc(ang, *a0, *a1)
            }
        }
    }

    pub fn render_word(text: &str, glyph_h_px: f32, letter_spacing_px: f32, rgb: [u8; 3]) -> (u32, u32, Vec<u8>) {
        let upper = text.trim().to_ascii_uppercase();
        if upper.is_empty() {
            return (1, 1, vec![0; 4]);
        }
        let scale = (glyph_h_px / 100.0).max(0.1);
        let norm_gap = letter_spacing_px / scale;
        let mut cursor_x = 0.0f32;
        let mut placed: Vec<(f32, Vec<Shape>)> = Vec::with_capacity(upper.len());
        for (i, ch) in upper.chars().enumerate() {
            if i > 0 {
                cursor_x += norm_gap;
            }
            let shapes = glyph_shapes(ch);
            if !shapes.is_empty() {
                placed.push((cursor_x, shapes));
            }
            cursor_x += advance(ch);
        }
        let w = (cursor_x * scale).ceil().max(1.0) as u32 + 2;
        let h = glyph_h_px.ceil().max(1.0) as u32 + 2;
        let mut out = vec![0u8; (w * h * 4) as usize];
        const SS: u32 = 4;
        for y in 0..h {
            for x in 0..w {
                let mut hits = 0u32;
                for sy in 0..SS {
                    for sx in 0..SS {
                        let px = (x as f32 + (sx as f32 + 0.5) / SS as f32 - 1.0) / scale;
                        let py = (y as f32 + (sy as f32 + 0.5) / SS as f32 - 1.0) / scale;
                        if py >= -1.0 && py <= 101.0 {
                            if placed.iter().any(|(ox, shapes)| {
                                let lx = px - ox;
                                lx >= -2.0 && lx <= 100.0 && shapes.iter().any(|s| hit_shape(s, (lx, py)))
                            }) {
                                hits += 1;
                            }
                        }
                    }
                }
                if hits == 0 {
                    continue;
                }
                let a = (hits * 255 / (SS * SS)) as u32;
                let idx = ((y * w + x) * 4) as usize;
                out[idx] = (rgb[2] as u32 * a / 255) as u8;
                out[idx + 1] = (rgb[1] as u32 * a / 255) as u8;
                out[idx + 2] = (rgb[0] as u32 * a / 255) as u8;
                out[idx + 3] = a as u8;
            }
        }
        (w, h, out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn speed_formatting_is_clean() {
        assert_eq!(format_speed(104), "104 B/s");
        assert_eq!(format_speed(2048), "2.0 KB/s");
        assert_eq!(format_speed(3 * 1024 * 1024), "3.0 MB/s");
    }

    #[test]
    fn anurati_renders_thursday_with_non_empty_premultiplied_pixels() {
        let (w, h, bgra) = anurati::render_word("THURSDAY", 36.0, 24.0, [255, 255, 255]);
        assert!(w > 200 && h >= 36);
        let lit = bgra.chunks_exact(4).filter(|p| p[3] > 200).count();
        assert!(lit > 400);
        assert!(bgra.chunks_exact(4).all(|p| p[0] <= p[3] && p[1] <= p[3] && p[2] <= p[3]));
    }
}
