use crate::gfx::Gfx;
use np_core::symbols::{self, Sym};
use np_core::widgets::anurati;
use np_platform::text::{self, Style, TextAlign};
use std::collections::{HashMap, VecDeque};
use windows::UI::Color;
use windows::UI::Composition::{
    CompositionClip, CompositionDrawingSurface, CompositionStretch, Compositor, ContainerVisual,
    SpriteVisual,
};
use windows_numerics::{Vector2, Vector3};

pub const WHITE: [u8; 3] = [255, 255, 255];

#[derive(Clone, Copy)]
pub enum Anchor {
    Left,
    Center,
    Right,
}

pub struct Painter {
    pub comp: Compositor,
    pub gfx: Gfx,
    pub dpi: f32,
    cache: HashMap<String, (CompositionDrawingSurface, i32, i32, f32)>,
}

pub fn clip(comp: &Compositor, w: f32, h: f32, r: f32) -> windows_core::Result<CompositionClip> {
    let g = comp.CreateRoundedRectangleGeometry()?;
    g.SetSize(Vector2 { X: w, Y: h })?;
    g.SetCornerRadius(Vector2 { X: r, Y: r })?;
    let c = comp.CreateGeometricClip()?;
    c.SetGeometry(&g)?;
    windows_core::Interface::cast(&c)
}

fn recolor(b: &[u8], rgb: [u8; 3]) -> Vec<u8> {
    let mut out = b.to_vec();
    for px in out.chunks_exact_mut(4) {
        let a = px[3] as u32;
        px[0] = (rgb[2] as u32 * a / 255) as u8;
        px[1] = (rgb[1] as u32 * a / 255) as u8;
        px[2] = (rgb[0] as u32 * a / 255) as u8;
    }
    out
}

impl Painter {
    pub fn new(comp: Compositor, gfx: Gfx, dpi: f32) -> Self {
        Self {
            comp,
            gfx,
            dpi,
            cache: HashMap::new(),
        }
    }

    pub fn reset(&mut self, dpi: f32) {
        self.dpi = dpi;
    }

    pub fn clear_cache(&mut self) {
        self.cache.clear();
    }

    fn trim(&mut self) {
        if self.cache.len() > 280 {
            self.cache.clear();
        }
    }

    pub fn px(&self, dp: f32) -> f32 {
        dp * self.dpi
    }

    pub fn measure(&self, s: &str, size_dp: f32, weight: i32) -> f32 {
        text::measure(
            s,
            &Style {
                px: (size_dp * self.dpi).round() as i32,
                weight,
            },
        )
        .0 as f32
    }

    pub fn fit(&self, s: &str, size_dp: f32, weight: i32, max_w: f32) -> String {
        if self.measure(s, size_dp, weight) <= max_w {
            return s.to_string();
        }
        let mut chars: Vec<char> = s.chars().collect();
        while !chars.is_empty() {
            chars.pop();
            let t: String = chars.iter().collect::<String>().trim_end().to_string() + "\u{2026}";
            if self.measure(&t, size_dp, weight) <= max_w {
                return t;
            }
        }
        String::new()
    }

    pub fn place(
        &self,
        parent: &ContainerVisual,
        s: &CompositionDrawingSurface,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        opacity: f32,
    ) -> Option<SpriteVisual> {
        let go = || -> windows_core::Result<SpriteVisual> {
            let brush = self.comp.CreateSurfaceBrushWithSurface(s)?;
            brush.SetStretch(CompositionStretch::None)?;
            let v = self.comp.CreateSpriteVisual()?;
            v.SetBrush(&brush)?;
            v.SetSize(Vector2 { X: w, Y: h })?;
            v.SetOffset(Vector3 {
                X: x.round(),
                Y: y.round(),
                Z: 0.0,
            })?;
            v.SetOpacity(opacity)?;
            parent.Children()?.InsertAtTop(&v)?;
            Ok(v)
        };
        go().ok()
    }

    #[allow(clippy::too_many_arguments)]
    pub fn text(
        &mut self,
        parent: &ContainerVisual,
        s: &str,
        x: f32,
        y: f32,
        size_dp: f32,
        weight: i32,
        rgb: [u8; 3],
        opacity: f32,
        anchor: Anchor,
        max_w: f32,
    ) -> f32 {
        self.draw_text(parent, s, x, y, size_dp, weight, rgb, opacity, anchor, max_w, false)
    }

    /// Renders text centered both horizontally at `cx` and vertically by ink bounding box at `cy`.
    #[allow(clippy::too_many_arguments)]
    pub fn pill_text(
        &mut self,
        parent: &ContainerVisual,
        s: &str,
        cx: f32,
        cy: f32,
        size_dp: f32,
        weight: i32,
        rgb: [u8; 3],
        opacity: f32,
        max_w: f32,
    ) -> f32 {
        self.draw_text(parent, s, cx, cy, size_dp, weight, rgb, opacity, Anchor::Center, max_w, true)
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_text(
        &mut self,
        parent: &ContainerVisual,
        s: &str,
        x: f32,
        y: f32,
        size_dp: f32,
        weight: i32,
        rgb: [u8; 3],
        opacity: f32,
        anchor: Anchor,
        max_w: f32,
        center_ink_y: bool,
    ) -> f32 {
        if s.is_empty() {
            return 0.0;
        }
        let px = (size_dp * self.dpi).round().max(8.0) as i32;
        let st = Style { px, weight };
        let (tw, th) = text::measure(s, &st);
        let w = (tw + 6).min(max_w.ceil().max(8.0) as i32).max(1);
        let (align, atag) = match anchor {
            Anchor::Left => (TextAlign::Left, 'l'),
            Anchor::Center => (TextAlign::Center, 'c'),
            Anchor::Right => (TextAlign::Right, 'r'),
        };
        let key = format!("t|{atag}|{px}|{weight}|{rgb:?}|{w}|{s}");
        let (surf, w, h, ink_mid_y) = if let Some(hit) = self.cache.get(&key) {
            hit.clone()
        } else {
            let Some(bmp) = text::render_aligned(s, &st, w, th, align) else {
                return 0.0;
            };
            let ink_mid = text::ink_y_bounds(&bmp)
                .map(|(y0, y1)| (y0 + y1) * 0.5)
                .unwrap_or(bmp.h as f32 * 0.5);
            let Ok(sf) = self
                .gfx
                .surface_from_bgra(bmp.w, bmp.h, &recolor(&bmp.bgra, rgb))
            else {
                return 0.0;
            };
            self.trim();
            let v = (sf, bmp.w as i32, bmp.h as i32, ink_mid);
            self.cache.insert(key, v.clone());
            v
        };
        let left = match anchor {
            Anchor::Left => x,
            Anchor::Center => x - w as f32 / 2.0,
            Anchor::Right => x - w as f32,
        };
        let top = if center_ink_y { y - ink_mid_y } else { y };
        self.place(parent, &surf, left, top, w as f32, h as f32, opacity);
        w as f32
    }

    #[allow(clippy::too_many_arguments)]
    pub fn text_shadow(
        &mut self,
        parent: &ContainerVisual,
        s: &str,
        x: f32,
        y: f32,
        size_dp: f32,
        weight: i32,
        rgb: [u8; 3],
        opacity: f32,
        anchor: Anchor,
        max_w: f32,
    ) -> f32 {
        let off = self.px(1.2).max(1.0);
        self.text(
            parent,
            s,
            x,
            y + off,
            size_dp,
            weight,
            [0, 0, 0],
            opacity * 0.42,
            anchor,
            max_w,
        );
        self.text(parent, s, x, y, size_dp, weight, rgb, opacity, anchor, max_w)
    }

    pub fn anurati_centered(
        &mut self,
        parent: &ContainerVisual,
        word: &str,
        cx: f32,
        y: f32,
        glyph_h_px: f32,
        spacing_px: f32,
        rgb: [u8; 3],
    ) -> (f32, f32) {
        let gh = glyph_h_px.round().max(12.0) as i32;
        let sp = spacing_px.round().max(2.0) as i32;
        let render_word = |painter: &mut Self, col: [u8; 3]| -> Option<(CompositionDrawingSurface, i32, i32)> {
            let key = format!("anu|{gh}|{sp}|{col:?}|{word}");
            if let Some((sf, bw, bh, _)) = painter.cache.get(&key) {
                return Some((sf.clone(), *bw, *bh));
            }
            let (bw, bh, data) = anurati::render_word(word, gh as f32, sp as f32, col);
            let sf = painter.gfx.surface_from_bgra(bw, bh, &data).ok()?;
            painter.trim();
            let v = (sf.clone(), bw as i32, bh as i32, bh as f32 * 0.5);
            painter.cache.insert(key, v);
            Some((sf, bw as i32, bh as i32))
        };
        if let Some((sh, sw, sh_h)) = render_word(self, [0, 0, 0]) {
            let off = self.px(1.8).max(1.5);
            self.place(parent, &sh, cx - sw as f32 / 2.0, y + off, sw as f32, sh_h as f32, 0.36);
        }
        let Some((surf, w, h)) = render_word(self, rgb) else {
            return (0.0, 0.0);
        };
        self.place(parent, &surf, cx - w as f32 / 2.0, y, w as f32, h as f32, 1.0);
        (w as f32, h as f32)
    }

    pub fn sym(
        &mut self,
        parent: &ContainerVisual,
        s: Sym,
        cx: f32,
        cy: f32,
        size_dp: f32,
        rgb: [u8; 3],
        opacity: f32,
    ) {
        let n = (size_dp * self.dpi).round().max(10.0) as u32;
        let key = format!("s|{s:?}|{n}|{rgb:?}");
        let (surf, w, h) = if let Some((sf, w, h, _)) = self.cache.get(&key) {
            (sf.clone(), *w, *h)
        } else {
            let data = symbols::render(s, n, rgb);
            let Ok(sf) = self.gfx.surface_from_bgra(n, n, &data) else {
                return;
            };
            self.trim();
            let v = (sf.clone(), n as i32, n as i32, n as f32 * 0.5);
            self.cache.insert(key, v);
            (sf, n as i32, n as i32)
        };
        self.place(
            parent,
            &surf,
            cx - w as f32 / 2.0,
            cy - h as f32 / 2.0,
            w as f32,
            h as f32,
            opacity,
        );
    }

    pub fn layered_weather(&mut self, parent: &ContainerVisual, cx: f32, cy: f32, size_dp: f32) {
        let sun_off_x = self.px(size_dp * 0.14);
        let sun_off_y = self.px(size_dp * 0.14);
        self.sym(
            parent,
            Sym::Sun,
            cx - sun_off_x,
            cy - sun_off_y,
            size_dp * 0.72,
            [255, 206, 78],
            0.96,
        );
        self.sym(
            parent,
            Sym::Cloud,
            cx + self.px(size_dp * 0.05),
            cy + self.px(size_dp * 0.08),
            size_dp * 0.88,
            [244, 248, 255],
            0.96,
        );
    }

    pub fn rect(
        &self,
        parent: &ContainerVisual,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        radius: f32,
        c: Color,
    ) -> Option<SpriteVisual> {
        let go = || -> windows_core::Result<SpriteVisual> {
            let v = self.comp.CreateSpriteVisual()?;
            v.SetSize(Vector2 { X: w, Y: h })?;
            v.SetOffset(Vector3 { X: x, Y: y, Z: 0.0 })?;
            v.SetBrush(&self.comp.CreateColorBrushWithColor(c)?)?;
            if radius > 0.0 {
                v.SetClip(&clip(&self.comp, w, h, radius)?)?;
            }
            parent.Children()?.InsertAtTop(&v)?;
            Ok(v)
        };
        go().ok()
    }

    #[allow(clippy::too_many_arguments)]
    pub fn grad_rect(
        &self,
        parent: &ContainerVisual,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        radius: f32,
        c0: Color,
        c1: Color,
        horizontal: bool,
    ) -> Option<SpriteVisual> {
        let go = || -> windows_core::Result<SpriteVisual> {
            let b = self.comp.CreateLinearGradientBrush()?;
            b.SetStartPoint(Vector2 { X: 0.0, Y: 0.0 })?;
            b.SetEndPoint(if horizontal {
                Vector2 { X: 1.0, Y: 0.0 }
            } else {
                Vector2 { X: 0.0, Y: 1.0 }
            })?;
            let stops = b.ColorStops()?;
            stops.Append(&self.comp.CreateColorGradientStopWithOffsetAndColor(0.0, c0)?)?;
            stops.Append(&self.comp.CreateColorGradientStopWithOffsetAndColor(1.0, c1)?)?;
            let v = self.comp.CreateSpriteVisual()?;
            v.SetSize(Vector2 { X: w, Y: h })?;
            v.SetOffset(Vector3 { X: x, Y: y, Z: 0.0 })?;
            v.SetBrush(&b)?;
            if radius > 0.0 {
                v.SetClip(&clip(&self.comp, w, h, radius)?)?;
            }
            parent.Children()?.InsertAtTop(&v)?;
            Ok(v)
        };
        go().ok()
    }

    #[allow(clippy::too_many_arguments)]
    pub fn stroke_rect(
        &self,
        parent: &ContainerVisual,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        radius: f32,
        thickness: f32,
        top_c: Color,
        bot_c: Color,
    ) {
        let go = || -> windows_core::Result<()> {
            let half = thickness * 0.5;
            let g = self.comp.CreateRoundedRectangleGeometry()?;
            g.SetOffset(Vector2 { X: half, Y: half })?;
            g.SetSize(Vector2 {
                X: (w - thickness).max(0.0),
                Y: (h - thickness).max(0.0),
            })?;
            let r = (radius - half).max(0.0);
            g.SetCornerRadius(Vector2 { X: r, Y: r })?;
            let b = self.comp.CreateLinearGradientBrush()?;
            b.SetStartPoint(Vector2 { X: 0.0, Y: 0.0 })?;
            b.SetEndPoint(Vector2 { X: 0.0, Y: 1.0 })?;
            let stops = b.ColorStops()?;
            stops.Append(&self.comp.CreateColorGradientStopWithOffsetAndColor(0.0, top_c)?)?;
            stops.Append(&self.comp.CreateColorGradientStopWithOffsetAndColor(1.0, bot_c)?)?;
            let shape = self.comp.CreateSpriteShapeWithGeometry(&g)?;
            shape.SetStrokeBrush(&b)?;
            shape.SetStrokeThickness(thickness)?;
            let sv = self.comp.CreateShapeVisual()?;
            sv.SetSize(Vector2 { X: w, Y: h })?;
            sv.SetOffset(Vector3 { X: x, Y: y, Z: 0.0 })?;
            sv.Shapes()?.Append(&shape)?;
            parent.Children()?.InsertAtTop(&sv)?;
            Ok(())
        };
        let _ = go();
    }

    /// Draws a translucent glass inner card with subtle top-to-bottom fill gradient and specular rim border.
    #[allow(clippy::too_many_arguments)]
    pub fn card(
        &self,
        parent: &ContainerVisual,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        radius: f32,
        fill_rgb: [u8; 3],
        fill_alpha: u8,
        border_alpha: u8,
    ) {
        let top_a = fill_alpha.saturating_add((fill_alpha / 3).max(4));
        let bot_a = (fill_alpha as f32 * 0.62).round() as u8;
        self.grad_rect(
            parent,
            x,
            y,
            w,
            h,
            radius,
            rgba(fill_rgb, top_a),
            rgba(fill_rgb, bot_a),
            false,
        );
        if border_alpha > 0 {
            let bot_b = (border_alpha as f32 * 0.30).round() as u8;
            self.stroke_rect(
                parent,
                x,
                y,
                w,
                h,
                radius,
                self.px(0.95).clamp(0.8, 1.5),
                rgba(WHITE, border_alpha),
                rgba(WHITE, bot_b),
            );
        }
    }

    /// Soft analytical rounded glow/drop shadow rendered into premultiplied BGRA.
    #[allow(clippy::too_many_arguments)]
    pub fn soft_shadow(
        &mut self,
        parent: &ContainerVisual,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        radius: f32,
        spread: f32,
        rgb: [u8; 3],
        alpha: u8,
    ) {
        let bw = (w + 2.0 * spread).ceil().max(8.0) as u32;
        let bh = (h + 2.0 * spread).ceil().max(8.0) as u32;
        let ri = radius.round() as i32;
        let si = spread.round() as i32;
        let key = format!("sh|{bw}|{bh}|{ri}|{si}|{rgb:?}|{alpha}");
        let surf = if let Some((sf, _, _, _)) = self.cache.get(&key) {
            sf.clone()
        } else {
            let mut bgra = vec![0u8; (bw * bh * 4) as usize];
            let cx = bw as f32 * 0.5;
            let cy = bh as f32 * 0.5;
            let hx = w * 0.5 - radius;
            let hy = h * 0.5 - radius;
            let sp = spread.max(1.0);
            for py in 0..bh {
                for px in 0..bw {
                    let qx = (px as f32 + 0.5 - cx).abs() - hx;
                    let qy = (py as f32 + 0.5 - cy).abs() - hy;
                    let d = (qx.max(0.0).powi(2) + qy.max(0.0).powi(2)).sqrt() + qx.max(qy).min(0.0) - radius;
                    if d <= sp {
                        let k = if d <= 0.0 {
                            1.0
                        } else {
                            let t = 1.0 - (d / sp).clamp(0.0, 1.0);
                            t * t * (3.0 - 2.0 * t)
                        };
                        let a = (alpha as f32 * k).round() as u32;
                        let i = ((py * bw + px) * 4) as usize;
                        bgra[i] = (rgb[2] as u32 * a / 255) as u8;
                        bgra[i + 1] = (rgb[1] as u32 * a / 255) as u8;
                        bgra[i + 2] = (rgb[0] as u32 * a / 255) as u8;
                        bgra[i + 3] = a as u8;
                    }
                }
            }
            let Ok(sf) = self.gfx.surface_from_bgra(bw, bh, &bgra) else {
                return;
            };
            self.trim();
            self.cache.insert(key, (sf.clone(), bw as i32, bh as i32, bh as f32 * 0.5));
            sf
        };
        self.place(parent, &surf, x - spread, y - spread, bw as f32, bh as f32, 1.0);
    }

    pub fn image(
        &self,
        parent: &ContainerVisual,
        s: &CompositionDrawingSurface,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        radius: f32,
    ) -> Option<SpriteVisual> {
        let go = || -> windows_core::Result<SpriteVisual> {
            let b = self.comp.CreateSurfaceBrushWithSurface(s)?;
            b.SetStretch(CompositionStretch::UniformToFill)?;
            let v = self.comp.CreateSpriteVisual()?;
            v.SetBrush(&b)?;
            v.SetSize(Vector2 { X: w, Y: h })?;
            v.SetOffset(Vector3 { X: x, Y: y, Z: 0.0 })?;
            if radius > 0.0 {
                v.SetClip(&clip(&self.comp, w, h, radius)?)?;
            }
            parent.Children()?.InsertAtTop(&v)?;
            Ok(v)
        };
        go().ok()
    }

    /// SDF anti-aliased circular progress ring starting at 12 o'clock.
    #[allow(clippy::too_many_arguments)]
    pub fn ring_progress(
        &self,
        parent: &ContainerVisual,
        cx: f32,
        cy: f32,
        dia: f32,
        stroke: f32,
        frac: f32,
        track_rgb: [u8; 3],
        active_rgb: [u8; 3],
    ) {
        let n = dia.ceil().max(12.0) as u32;
        let mut bgra = vec![0u8; (n * n * 4) as usize];
        let mid = n as f32 * 0.5;
        let r_mid = (mid - stroke * 0.5 - 1.2).max(2.0);
        let half_w = stroke * 0.5;
        let sweep = frac.clamp(0.0, 1.0) * std::f32::consts::TAU;
        let end_x = mid + r_mid * sweep.sin();
        let end_y = mid - r_mid * sweep.cos();
        let start_x = mid;
        let start_y = mid - r_mid;

        for py in 0..n {
            for px in 0..n {
                let fx = px as f32 + 0.5;
                let fy = py as f32 + 0.5;
                let dist = ((fx - mid).powi(2) + (fy - mid).powi(2)).sqrt();
                let ring_d = (dist - r_mid).abs() - half_w;
                let track_cov = (0.75 - ring_d).clamp(0.0, 1.0);
                if track_cov <= 0.0 {
                    continue;
                }
                let mut ang = (fx - mid).atan2(-(fy - mid));
                if ang < 0.0 {
                    ang += std::f32::consts::TAU;
                }
                let active_cov = if sweep >= std::f32::consts::TAU - 0.01 {
                    track_cov
                } else if sweep <= 0.005 {
                    0.0
                } else {
                    let cap_s = ((fx - start_x).powi(2) + (fy - start_y).powi(2)).sqrt() - half_w;
                    let cap_e = ((fx - end_x).powi(2) + (fy - end_y).powi(2)).sqrt() - half_w;
                    let cap_cov = (0.75 - cap_s.min(cap_e)).clamp(0.0, 1.0);
                    let in_arc = if ang <= sweep { track_cov } else { 0.0 };
                    in_arc.max(cap_cov)
                };
                let tr_a = track_cov * 46.0;
                let ac_a = active_cov * 242.0;
                let a = ac_a.max(tr_a).round().clamp(0.0, 255.0);
                if a <= 0.0 {
                    continue;
                }
                let k = (ac_a / a).clamp(0.0, 1.0);
                let r = track_rgb[0] as f32 * (1.0 - k) + active_rgb[0] as f32 * k;
                let g = track_rgb[1] as f32 * (1.0 - k) + active_rgb[1] as f32 * k;
                let b = track_rgb[2] as f32 * (1.0 - k) + active_rgb[2] as f32 * k;
                let af = a / 255.0;
                let i = ((py * n + px) * 4) as usize;
                bgra[i] = (b * af).round() as u8;
                bgra[i + 1] = (g * af).round() as u8;
                bgra[i + 2] = (r * af).round() as u8;
                bgra[i + 3] = a as u8;
            }
        }
        if let Ok(sf) = self.gfx.surface_from_bgra(n, n, &bgra) {
            self.place(parent, &sf, cx - mid, cy - mid, n as f32, n as f32, 1.0);
        }
    }

    /// Anti-aliased live telemetry area+line sparkline chart.
    #[allow(clippy::too_many_arguments)]
    pub fn sparkline(
        &self,
        parent: &ContainerVisual,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        samples: &VecDeque<u8>,
        rgb: [u8; 3],
    ) {
        let bw = w.round().max(16.0) as u32;
        let bh = h.round().max(8.0) as u32;
        if samples.len() < 2 {
            return;
        }
        let mut bgra = vec![0u8; (bw * bh * 4) as usize];
        let n = samples.len() - 1;
        let line_t = self.px(1.3).max(1.1);
        for px in 0..bw {
            let t = (px as f32 + 0.5) / bw as f32 * n as f32;
            let i0 = (t.floor() as usize).min(n - 1);
            let frac = t - i0 as f32;
            let v0 = samples[i0] as f32 / 100.0;
            let v1 = samples[i0 + 1] as f32 / 100.0;
            let val = v0 + (v1 - v0) * frac;
            let curve_y = (bh as f32 - 2.0) - val * (bh as f32 - 4.0);
            for py in 0..bh {
                let fy = py as f32 + 0.5;
                let d_line = (fy - curve_y).abs();
                let line_a = ((line_t + 0.8 - d_line) / 1.2).clamp(0.0, 1.0) * 235.0;
                let fill_a = if fy >= curve_y {
                    let depth = 1.0 - ((fy - curve_y) / bh as f32).clamp(0.0, 1.0);
                    depth * 64.0
                } else {
                    0.0
                };
                let a = line_a.max(fill_a);
                if a <= 0.5 {
                    continue;
                }
                let af = a / 255.0;
                let idx = ((py * bw + px) * 4) as usize;
                bgra[idx] = (rgb[2] as f32 * af).round() as u8;
                bgra[idx + 1] = (rgb[1] as f32 * af).round() as u8;
                bgra[idx + 2] = (rgb[0] as f32 * af).round() as u8;
                bgra[idx + 3] = a.round() as u8;
            }
        }
        if let Ok(sf) = self.gfx.surface_from_bgra(bw, bh, &bgra) {
            self.place(parent, &sf, x, y, bw as f32, bh as f32, 1.0);
        }
    }

    /// Smooth wavy progress bar with ambient active glow and crisp capsule thumb.
    #[allow(clippy::too_many_arguments)]
    pub fn wavy_progress(
        &mut self,
        parent: &ContainerVisual,
        x: f32,
        cy: f32,
        w: f32,
        frac: f32,
        phase: f32,
        active_rgb: [u8; 3],
        playing: bool,
    ) {
        let bw = w.round().max(16.0) as u32;
        let bh = (self.px(16.0).round() as u32).max(12);
        let qphase = if playing {
            (phase * 12.0).round() / 12.0
        } else {
            0.0
        };
        let qfrac = (frac.clamp(0.0, 1.0) * 160.0).round() / 160.0;
        let mut bgra = vec![0u8; (bw * bh * 4) as usize];
        let mid = bh as f32 / 2.0;
        let split_x = bw as f32 * qfrac;
        let amp = if playing { self.px(2.6) } else { 0.0 };
        let freq = 0.21 / self.dpi.max(0.75);
        let thick = self.px(1.85).max(1.4);

        for py in 0..bh {
            for px in 0..bw {
                let fx = px as f32 + 0.5;
                let fy = py as f32 + 0.5;
                let (col, a) = if fx <= split_x {
                    let wave = (fx * freq + qphase).sin() * amp;
                    let d = (fy - (mid + wave)).abs();
                    let core = ((thick + 0.85 - d) / 1.15).clamp(0.0, 1.0) * 245.0;
                    let aura = ((thick * 2.5 - d) / (thick * 2.5)).clamp(0.0, 1.0) * 44.0;
                    (active_rgb, core.max(aura) as u32)
                } else {
                    let d = (fy - mid).abs();
                    let cov = ((thick * 0.72 + 0.7 - d) / 1.1).clamp(0.0, 1.0);
                    (WHITE, (72.0 * cov).round() as u32)
                };
                if a == 0 {
                    continue;
                }
                let idx = ((py * bw + px) * 4) as usize;
                bgra[idx] = (col[2] as u32 * a / 255) as u8;
                bgra[idx + 1] = (col[1] as u32 * a / 255) as u8;
                bgra[idx + 2] = (col[0] as u32 * a / 255) as u8;
                bgra[idx + 3] = a.min(255) as u8;
            }
        }
        if let Ok(surf) = self.gfx.surface_from_bgra(bw, bh, &bgra) {
            self.place(parent, &surf, x, cy - bh as f32 / 2.0, bw as f32, bh as f32, 1.0);
        }
        let thumb_w = self.px(4.5);
        let thumb_h = self.px(13.0);
        let tx = (x + split_x - thumb_w / 2.0).clamp(x, x + w - thumb_w);
        self.soft_shadow(
            parent,
            tx,
            cy - thumb_h / 2.0,
            thumb_w,
            thumb_h,
            thumb_w / 2.0,
            self.px(4.0),
            active_rgb,
            110,
        );
        self.rect(
            parent,
            tx,
            cy - thumb_h / 2.0,
            thumb_w,
            thumb_h,
            thumb_w / 2.0,
            rgba(WHITE, 252),
        );
    }
}

pub fn rgba(rgb: [u8; 3], a: u8) -> Color {
    Color {
        A: a,
        R: rgb[0],
        G: rgb[1],
        B: rgb[2],
    }
}
