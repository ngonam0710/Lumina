#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sym {
    Music,
    Widgets,
    Sliders,
    Tools,
    Clipboard,
    Gear,
    Play,
    Pause,
    Next,
    Prev,
    Wifi,
    Bluetooth,
    Moon,
    Sun,
    Cloud,
    Mic,
    MicMute,
    Speaker,
    SpeakerMute,
    Power,
    Camera,
    Scissors,
    Record,
    Plus,
    Minus,
    Check,
    Close,
    ChevronLeft,
    ChevronRight,
    Folder,
    Document,
    ArrowUp,
    ArrowDown,
    Qr,
    Cpu,
    Clock,
    Calendar,
    Pin,
    Trash,
    PartlyCloudy,
    Drop,
    Eyedropper,
    Battery,
    Bolt,
    Sparkles,
    Search,
    Shield,
    Gamepad,
    Keyboard,
    Accessibility,
    Stop,
    Info,
}

type P = (f32, f32);

#[inline]
fn len(a: P, b: P) -> f32 {
    ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt()
}

#[inline]
fn seg(p: P, a: P, b: P) -> f32 {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let l2 = dx * dx + dy * dy;
    let t = if l2 <= 1e-8 {
        0.0
    } else {
        (((p.0 - a.0) * dx + (p.1 - a.1) * dy) / l2).clamp(0.0, 1.0)
    };
    len(p, (a.0 + t * dx, a.1 + t * dy))
}

fn poly(p: P, pts: &[P]) -> f32 {
    let mut d = f32::MAX;
    let mut inside = false;
    let mut j = pts.len() - 1;
    for i in 0..pts.len() {
        d = d.min(seg(p, pts[i], pts[j]));
        let (pi, pj) = (pts[i], pts[j]);
        if (pi.1 > p.1) != (pj.1 > p.1)
            && p.0 < (pj.0 - pi.0) * (p.1 - pi.1) / (pj.1 - pi.1) + pi.0
        {
            inside = !inside;
        }
        j = i;
    }
    if inside {
        -d
    } else {
        d
    }
}

#[inline]
fn circle(p: P, c: P, r: f32) -> f32 {
    len(p, c) - r
}

#[inline]
fn line(p: P, a: P, b: P, w: f32) -> f32 {
    seg(p, a, b) - w * 0.5
}

#[inline]
fn rrect(p: P, c: P, h: P, r: f32) -> f32 {
    let (qx, qy) = ((p.0 - c.0).abs() - (h.0 - r), (p.1 - c.1).abs() - (h.1 - r));
    (qx.max(0.0).powi(2) + qy.max(0.0).powi(2)).sqrt() + qx.max(qy).min(0.0) - r
}

#[inline]
fn ring(p: P, c: P, r: f32, w: f32) -> f32 {
    (len(p, c) - r).abs() - w * 0.5
}

#[inline]
fn arc(p: P, c: P, r: f32, w: f32, half: f32) -> f32 {
    let ang = (p.0 - c.0).atan2(-(p.1 - c.1));
    let a = ang.clamp(-half, half);
    let q = (c.0 + r * a.sin(), c.1 - r * a.cos());
    len(p, q) - w * 0.5
}

#[inline]
fn tri_right(p: P, tip_x: f32, base_x: f32) -> f32 {
    poly(
        p,
        &[(base_x, 0.25), (base_x, 0.75), (tip_x, 0.50)],
    ) - 0.045
}

fn sd(sym: Sym, p: P) -> f32 {
    match sym {
        Sym::Music => {
            let n1 = circle(p, (0.29, 0.72), 0.125);
            let n2 = circle(p, (0.69, 0.64), 0.125);
            let s1 = line(p, (0.38, 0.70), (0.38, 0.23), 0.075);
            let s2 = line(p, (0.78, 0.62), (0.78, 0.16), 0.075);
            let beam = line(p, (0.38, 0.23), (0.78, 0.16), 0.10);
            n1.min(n2).min(s1).min(s2).min(beam)
        }
        Sym::Widgets => {
            let mut d = f32::MAX;
            for c in [(0.30, 0.30), (0.70, 0.30), (0.30, 0.70), (0.70, 0.70)] {
                d = d.min(rrect(p, c, (0.145, 0.145), 0.055));
            }
            d
        }
        Sym::Sliders => {
            let l1 = line(p, (0.14, 0.30), (0.86, 0.30), 0.075);
            let l2 = line(p, (0.14, 0.70), (0.86, 0.70), 0.075);
            let k1 = circle(p, (0.66, 0.30), 0.115);
            let k2 = circle(p, (0.34, 0.70), 0.115);
            l1.min(l2).min(k1).min(k2)
        }
        Sym::Tools => {
            let box_r = rrect(p, (0.50, 0.56), (0.35, 0.26), 0.085).abs() - 0.038;
            let handle = rrect(p, (0.50, 0.26), (0.14, 0.065), 0.035).abs() - 0.034;
            let latch = rrect(p, (0.50, 0.50), (0.06, 0.045), 0.02);
            box_r.min(handle).min(latch)
        }
        Sym::Clipboard => {
            let board = rrect(p, (0.50, 0.56), (0.29, 0.33), 0.075).abs() - 0.038;
            let clip = rrect(p, (0.50, 0.22), (0.14, 0.075), 0.04);
            let l1 = line(p, (0.36, 0.49), (0.64, 0.49), 0.06);
            let l2 = line(p, (0.36, 0.66), (0.58, 0.66), 0.06);
            board.min(clip).min(l1).min(l2)
        }
        Sym::Gear => {
            let c = (0.5, 0.5);
            let mut d = ring(p, c, 0.25, 0.115);
            for i in 0..8 {
                let a = i as f32 * std::f32::consts::FRAC_PI_4;
                let t = line(
                    p,
                    (c.0 + 0.29 * a.cos(), c.1 + 0.29 * a.sin()),
                    (c.0 + 0.41 * a.cos(), c.1 + 0.41 * a.sin()),
                    0.125,
                );
                d = d.min(t);
            }
            d
        }
        Sym::Play => poly(p, &[(0.31, 0.23), (0.31, 0.77), (0.77, 0.50)]) - 0.055,
        Sym::Pause => rrect(p, (0.35, 0.50), (0.085, 0.28), 0.05)
            .min(rrect(p, (0.65, 0.50), (0.085, 0.28), 0.05)),
        Sym::Prev => {
            let bar = line(p, (0.22, 0.24), (0.22, 0.76), 0.085);
            let tri = tri_right((1.0 - p.0, p.1), 0.68, 0.26);
            bar.min(tri)
        }
        Sym::Next => {
            let bar = line(p, (0.78, 0.24), (0.78, 0.76), 0.085);
            let tri = tri_right(p, 0.68, 0.26);
            bar.min(tri)
        }
        Sym::Wifi => {
            let apex = (0.5, 0.79);
            let mut d = circle(p, apex, 0.078);
            for r in [0.255f32, 0.44, 0.625] {
                d = d.min(arc(p, apex, r, 0.088, 0.76));
            }
            d
        }
        Sym::Bluetooth => {
            let pts: [P; 6] = [
                (0.28, 0.34),
                (0.70, 0.66),
                (0.49, 0.85),
                (0.49, 0.15),
                (0.70, 0.34),
                (0.28, 0.66),
            ];
            let mut d = f32::MAX;
            for i in 0..5 {
                d = d.min(line(p, pts[i], pts[i + 1], 0.082));
            }
            d
        }
        Sym::Moon => circle(p, (0.47, 0.50), 0.36).max(-circle(p, (0.68, 0.36), 0.30)),
        Sym::Sun => {
            let c = (0.5, 0.5);
            let mut d = circle(p, c, 0.205);
            for i in 0..8 {
                let a = i as f32 * std::f32::consts::FRAC_PI_4;
                d = d.min(line(
                    p,
                    (c.0 + 0.32 * a.cos(), c.1 + 0.32 * a.sin()),
                    (c.0 + 0.44 * a.cos(), c.1 + 0.44 * a.sin()),
                    0.078,
                ));
            }
            d
        }
        Sym::Cloud => circle(p, (0.38, 0.54), 0.20)
            .min(circle(p, (0.58, 0.48), 0.23))
            .min(circle(p, (0.72, 0.62), 0.15))
            .min(rrect(p, (0.53, 0.66), (0.28, 0.12), 0.11)),
        Sym::Mic | Sym::MicMute => {
            let cap = rrect(p, (0.50, 0.35), (0.125, 0.21), 0.125);
            let cradle_arc = arc(p, (0.50, 0.46), 0.255, 0.072, std::f32::consts::PI);
            let cradle = cradle_arc.max(0.44 - p.1);
            let stem = line(p, (0.50, 0.72), (0.50, 0.87), 0.078)
                .min(line(p, (0.35, 0.87), (0.65, 0.87), 0.078));
            let base = cap.min(cradle).min(stem);
            if sym == Sym::MicMute {
                base.min(line(p, (0.18, 0.82), (0.82, 0.18), 0.088))
            } else {
                base
            }
        }
        Sym::Speaker | Sym::SpeakerMute => {
            let body = rrect(p, (0.21, 0.50), (0.09, 0.12), 0.03).min(
                poly(
                    p,
                    &[(0.26, 0.38), (0.51, 0.19), (0.51, 0.81), (0.26, 0.62)],
                ) - 0.025,
            );
            if sym == Sym::SpeakerMute {
                body.min(line(p, (0.62, 0.36), (0.85, 0.64), 0.082))
                    .min(line(p, (0.85, 0.36), (0.62, 0.64), 0.082))
            } else {
                let w1 = arc((p.1, 1.0 - p.0), (0.50, 0.52), 0.21, 0.072, 0.74);
                let w2 = arc((p.1, 1.0 - p.0), (0.50, 0.52), 0.35, 0.072, 0.72);
                body.min(w1).min(w2)
            }
        }
        Sym::Power => {
            let c = (0.50, 0.54);
            let ring_gap = ring(p, c, 0.32, 0.085);
            let cut = rrect(p, (0.50, 0.24), (0.14, 0.15), 0.02);
            ring_gap.max(-cut).min(line(p, (0.50, 0.13), (0.50, 0.46), 0.085))
        }
        Sym::Camera => {
            let body = rrect(p, (0.50, 0.54), (0.39, 0.26), 0.09).abs() - 0.038;
            let bump = rrect(p, (0.50, 0.24), (0.14, 0.055), 0.03);
            let lens = ring(p, (0.50, 0.55), 0.145, 0.065);
            let dot = circle(p, (0.74, 0.41), 0.035);
            body.min(bump).min(lens).min(dot)
        }
        Sym::Scissors => ring(p, (0.28, 0.72), 0.125, 0.068)
            .min(ring(p, (0.72, 0.72), 0.125, 0.068))
            .min(line(p, (0.34, 0.61), (0.72, 0.18), 0.078))
            .min(line(p, (0.66, 0.61), (0.28, 0.18), 0.078)),
        Sym::Record => ring(p, (0.5, 0.5), 0.36, 0.075).min(circle(p, (0.5, 0.5), 0.18)),
        Sym::Plus => line(p, (0.22, 0.50), (0.78, 0.50), 0.105)
            .min(line(p, (0.50, 0.22), (0.50, 0.78), 0.105)),
        Sym::Minus => line(p, (0.24, 0.50), (0.76, 0.50), 0.105),
        Sym::Check => line(p, (0.20, 0.53), (0.42, 0.75), 0.115)
            .min(line(p, (0.42, 0.75), (0.80, 0.28), 0.115)),
        Sym::Close => line(p, (0.25, 0.25), (0.75, 0.75), 0.105)
            .min(line(p, (0.75, 0.25), (0.25, 0.75), 0.105)),
        Sym::ChevronLeft => line(p, (0.62, 0.23), (0.36, 0.50), 0.095)
            .min(line(p, (0.36, 0.50), (0.62, 0.77), 0.095)),
        Sym::ChevronRight => line(p, (0.38, 0.23), (0.64, 0.50), 0.095)
            .min(line(p, (0.64, 0.50), (0.38, 0.77), 0.095)),
        Sym::Folder => {
            let body = rrect(p, (0.50, 0.57), (0.37, 0.24), 0.07).abs() - 0.04;
            let tab = rrect(p, (0.31, 0.30), (0.17, 0.07), 0.04).abs() - 0.038;
            body.min(tab.max(p.1 - 0.36))
        }
        Sym::Document => {
            let body = rrect(p, (0.50, 0.50), (0.28, 0.36), 0.06).abs() - 0.036;
            let l1 = line(p, (0.35, 0.42), (0.65, 0.42), 0.055);
            let l2 = line(p, (0.35, 0.57), (0.65, 0.57), 0.055);
            let l3 = line(p, (0.35, 0.71), (0.54, 0.71), 0.055);
            body.min(l1).min(l2).min(l3)
        }
        Sym::ArrowDown => line(p, (0.50, 0.18), (0.50, 0.78), 0.115)
            .min(line(p, (0.25, 0.54), (0.50, 0.80), 0.115))
            .min(line(p, (0.75, 0.54), (0.50, 0.80), 0.115)),
        Sym::ArrowUp => line(p, (0.50, 0.82), (0.50, 0.22), 0.115)
            .min(line(p, (0.25, 0.46), (0.50, 0.20), 0.115))
            .min(line(p, (0.75, 0.46), (0.50, 0.20), 0.115)),
        Sym::Qr => {
            let b1 = rrect(p, (0.28, 0.28), (0.15, 0.15), 0.03).abs() - 0.038;
            let b2 = rrect(p, (0.72, 0.28), (0.15, 0.15), 0.03).abs() - 0.038;
            let b3 = rrect(p, (0.28, 0.72), (0.15, 0.15), 0.03).abs() - 0.038;
            let c1 = rrect(p, (0.28, 0.28), (0.045, 0.045), 0.01);
            let c2 = rrect(p, (0.72, 0.28), (0.045, 0.045), 0.01);
            let c3 = rrect(p, (0.28, 0.72), (0.045, 0.045), 0.01);
            let m1 = rrect(p, (0.62, 0.62), (0.055, 0.055), 0.015);
            let m2 = rrect(p, (0.79, 0.79), (0.055, 0.055), 0.015);
            let m3 = rrect(p, (0.79, 0.60), (0.045, 0.045), 0.015);
            b1.min(b2).min(b3).min(c1).min(c2).min(c3).min(m1).min(m2).min(m3)
        }
        Sym::Cpu => {
            let mut d = (rrect(p, (0.5, 0.5), (0.25, 0.25), 0.05).abs() - 0.04)
                .min(rrect(p, (0.5, 0.5), (0.11, 0.11), 0.03));
            for k in [0.35f32, 0.65] {
                d = d
                    .min(line(p, (k, 0.12), (k, 0.24), 0.068))
                    .min(line(p, (k, 0.76), (k, 0.88), 0.068))
                    .min(line(p, (0.12, k), (0.24, k), 0.068))
                    .min(line(p, (0.76, k), (0.88, k), 0.068));
            }
            d
        }
        Sym::Clock => {
            let c = (0.5, 0.5);
            ring(p, c, 0.36, 0.082)
                .min(line(p, c, (0.50, 0.26), 0.078))
                .min(line(p, c, (0.66, 0.58), 0.078))
        }
        Sym::Calendar => {
            let frame = rrect(p, (0.50, 0.56), (0.37, 0.31), 0.095).abs() - 0.038;
            let header = rrect(p, (0.50, 0.35), (0.36, 0.045), 0.01);
            let rings = line(p, (0.33, 0.14), (0.33, 0.27), 0.078)
                .min(line(p, (0.67, 0.14), (0.67, 0.27), 0.078));
            frame.min(header).min(rings)
        }
        Sym::Pin => line(p, (0.50, 0.45), (0.50, 0.86), 0.085)
            .min(rrect(p, (0.50, 0.30), (0.17, 0.14), 0.07))
            .min(line(p, (0.32, 0.45), (0.68, 0.45), 0.075)),
        Sym::Trash => (rrect(p, (0.50, 0.60), (0.23, 0.25), 0.05).abs() - 0.038)
            .min(line(p, (0.21, 0.30), (0.79, 0.30), 0.075))
            .min(line(p, (0.40, 0.20), (0.60, 0.20), 0.075)),
        Sym::PartlyCloudy => circle(p, (0.33, 0.35), 0.165)
            .min(circle(p, (0.44, 0.58), 0.165))
            .min(circle(p, (0.64, 0.62), 0.145))
            .min(rrect(p, (0.54, 0.66), (0.24, 0.11), 0.10)),
        Sym::Drop => circle(p, (0.50, 0.63), 0.235)
            .min(poly(p, &[(0.50, 0.15), (0.28, 0.58), (0.72, 0.58)])),
        Sym::Eyedropper => {
            let stem = line(p, (0.23, 0.77), (0.61, 0.39), 0.13);
            let hollow = line(p, (0.29, 0.71), (0.55, 0.45), 0.055);
            let tip = line(p, (0.16, 0.84), (0.25, 0.75), 0.07);
            let collar = line(p, (0.50, 0.28), (0.72, 0.50), 0.09);
            let bulb = line(p, (0.65, 0.35), (0.80, 0.20), 0.19);
            stem.max(-hollow).min(tip).min(collar).min(bulb)
        }
        Sym::Battery => {
            let shell = rrect(p, (0.47, 0.50), (0.34, 0.21), 0.065).abs() - 0.036;
            let nub = rrect(p, (0.86, 0.50), (0.04, 0.09), 0.02);
            shell.min(nub)
        }
        Sym::Bolt => {
            let p1 = poly(p, &[(0.56, 0.12), (0.24, 0.54), (0.52, 0.54)]) - 0.025;
            let p2 = poly(p, &[(0.48, 0.46), (0.76, 0.46), (0.44, 0.88)]) - 0.025;
            p1.min(p2)
        }
        Sym::Sparkles => {
            let star = |c: P, s: f32| {
                let d1 = line(p, (c.0 - s, c.1), (c.0 + s, c.1), s * 0.28);
                let d2 = line(p, (c.0, c.1 - s), (c.0, c.1 + s), s * 0.28);
                let core = circle(p, c, s * 0.34);
                d1.min(d2).min(core)
            };
            star((0.46, 0.52), 0.32).min(star((0.78, 0.24), 0.13))
        }
        Sym::Search => ring(p, (0.44, 0.44), 0.24, 0.085).min(line(p, (0.61, 0.61), (0.82, 0.82), 0.095)),
        Sym::Shield => {
            let outline = poly(
                p,
                &[(0.50, 0.14), (0.82, 0.26), (0.76, 0.64), (0.50, 0.86), (0.24, 0.64), (0.18, 0.26)],
            );
            outline.abs() - 0.042
        }
        Sym::Gamepad => {
            let body = rrect(p, (0.50, 0.54), (0.38, 0.21), 0.18).abs() - 0.04;
            let dpad = line(p, (0.28, 0.54), (0.40, 0.54), 0.055)
                .min(line(p, (0.34, 0.48), (0.34, 0.60), 0.055));
            let b1 = circle(p, (0.66, 0.50), 0.045);
            let b2 = circle(p, (0.73, 0.58), 0.045);
            body.min(dpad).min(b1).min(b2)
        }
        Sym::Keyboard => {
            let frame = rrect(p, (0.50, 0.52), (0.40, 0.24), 0.065).abs() - 0.036;
            let space = line(p, (0.32, 0.65), (0.68, 0.65), 0.06);
            let row = line(p, (0.24, 0.44), (0.76, 0.44), 0.06);
            frame.min(space).min(row)
        }
        Sym::Accessibility => circle(p, (0.50, 0.22), 0.09)
            .min(line(p, (0.24, 0.40), (0.76, 0.40), 0.08))
            .min(line(p, (0.50, 0.36), (0.50, 0.62), 0.09))
            .min(line(p, (0.50, 0.62), (0.34, 0.84), 0.08))
            .min(line(p, (0.50, 0.62), (0.66, 0.84), 0.08)),
        Sym::Stop => rrect(p, (0.50, 0.50), (0.24, 0.24), 0.06),
        Sym::Info => ring(p, (0.50, 0.50), 0.36, 0.08)
            .min(circle(p, (0.50, 0.32), 0.055))
            .min(line(p, (0.50, 0.45), (0.50, 0.70), 0.085)),
    }
}

pub fn render(sym: Sym, px: u32, rgb: [u8; 3]) -> Vec<u8> {
    let n = px.max(1);
    let mut out = vec![0u8; (n * n * 4) as usize];
    let inner = (n as f32 - 2.0).max(1.0);
    let aa = (1.15 / inner).max(0.02);
    const SS: u32 = 2;
    for y in 0..n {
        for x in 0..n {
            let mut cov = 0.0f32;
            for sy in 0..SS {
                for sx in 0..SS {
                    let fx = (x as f32 + (sx as f32 + 0.5) / SS as f32 - 1.0) / inner;
                    let fy = (y as f32 + (sy as f32 + 0.5) / SS as f32 - 1.0) / inner;
                    let d = sd(sym, (fx, fy));
                    let c = ((aa * 0.5 - d) / aa).clamp(0.0, 1.0);
                    cov += c * c * (3.0 - 2.0 * c);
                }
            }
            cov /= (SS * SS) as f32;
            let a = (cov * 255.0).round() as u32;
            if a == 0 {
                continue;
            }
            let i = ((y * n + x) * 4) as usize;
            out[i] = (rgb[2] as u32 * a / 255) as u8;
            out[i + 1] = (rgb[1] as u32 * a / 255) as u8;
            out[i + 2] = (rgb[0] as u32 * a / 255) as u8;
            out[i + 3] = a as u8;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_symbols_render_premultiplied_non_empty() {
        for s in [
            Sym::Music,
            Sym::Widgets,
            Sym::Sliders,
            Sym::Tools,
            Sym::Clipboard,
            Sym::Gear,
            Sym::Play,
            Sym::Pause,
            Sym::Next,
            Sym::Prev,
            Sym::Wifi,
            Sym::Bluetooth,
            Sym::Cloud,
            Sym::PartlyCloudy,
            Sym::Qr,
            Sym::Eyedropper,
            Sym::Battery,
            Sym::Bolt,
            Sym::Sparkles,
            Sym::Document,
            Sym::Search,
            Sym::Shield,
            Sym::Gamepad,
            Sym::Keyboard,
            Sym::Accessibility,
            Sym::Stop,
            Sym::Info,
        ] {
            let b = render(s, 32, [255, 255, 255]);
            let filled = b.chunks_exact(4).filter(|p| p[3] > 100).count();
            let soft_aa = b.chunks_exact(4).filter(|p| p[3] > 0 && p[3] < 255).count();
            assert!(filled > 15, "{s:?} had too few pixels");
            assert!(soft_aa > 0, "{s:?} expected smooth anti-aliased edge pixels");
            assert!(b.chunks_exact(4).all(|p| p[0] <= p[3] && p[1] <= p[3] && p[2] <= p[3]));
        }
    }
}
