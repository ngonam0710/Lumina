use np_core::settings::{surface_spec, SurfaceTheme};
use std::cell::{Cell, RefCell};
use windows::Foundation::{IPropertyValue, PropertyValue, TimeSpan};
use windows::Graphics::Effects::{
    IGraphicsEffect, IGraphicsEffectSource, IGraphicsEffectSource_Impl, IGraphicsEffect_Impl,
};
use windows::UI::Color;
use windows::UI::Composition::{
    CompositionBrush, CompositionClip, CompositionEffectBrush,
    CompositionEffectSourceParameter, CompositionLinearGradientBrush, CompositionObject,
    CompositionRoundedRectangleGeometry, CompositionSpriteShape, Compositor, ContainerVisual,
    ShapeVisual, SpriteVisual,
};
use windows::Win32::Foundation::{E_INVALIDARG, E_NOTIMPL};
use windows::Win32::Graphics::Direct2D::{CLSID_D2D1GaussianBlur, CLSID_D2D1Saturation};
use windows::Win32::System::WinRT::Graphics::Direct2D::{
    IGraphicsEffectD2D1Interop, IGraphicsEffectD2D1Interop_Impl, GRAPHICS_EFFECT_PROPERTY_MAPPING,
};
use windows_core::{implement, Interface, Result, GUID, HSTRING, PCWSTR};
use windows_numerics::{Vector2, Vector3};

#[implement(IGraphicsEffect, IGraphicsEffectSource, IGraphicsEffectD2D1Interop)]
struct D2dEffect {
    name: RefCell<HSTRING>,
    id: GUID,
    props: Vec<IPropertyValue>,
    sources: Vec<IGraphicsEffectSource>,
}

impl D2dEffect {
    fn make(id: GUID, props: Vec<IPropertyValue>, sources: Vec<IGraphicsEffectSource>) -> IGraphicsEffect {
        D2dEffect {
            name: RefCell::new(HSTRING::new()),
            id,
            props,
            sources,
        }
        .into()
    }
}

impl IGraphicsEffectSource_Impl for D2dEffect_Impl {}

impl IGraphicsEffect_Impl for D2dEffect_Impl {
    fn Name(&self) -> Result<HSTRING> {
        Ok(self.name.borrow().clone())
    }
    fn SetName(&self, name: &HSTRING) -> Result<()> {
        *self.name.borrow_mut() = name.clone();
        Ok(())
    }
}

impl IGraphicsEffectD2D1Interop_Impl for D2dEffect_Impl {
    fn GetEffectId(&self) -> Result<GUID> {
        Ok(self.id)
    }
    fn GetNamedPropertyMapping(
        &self,
        _: &PCWSTR,
        _: *mut u32,
        _: *mut GRAPHICS_EFFECT_PROPERTY_MAPPING,
    ) -> Result<()> {
        Err(E_NOTIMPL.into())
    }
    fn GetPropertyCount(&self) -> Result<u32> {
        Ok(self.props.len() as u32)
    }
    fn GetProperty(&self, i: u32) -> Result<IPropertyValue> {
        self.props.get(i as usize).cloned().ok_or(E_INVALIDARG.into())
    }
    fn GetSource(&self, i: u32) -> Result<IGraphicsEffectSource> {
        self.sources.get(i as usize).cloned().ok_or(E_INVALIDARG.into())
    }
    fn GetSourceCount(&self) -> Result<u32> {
        Ok(self.sources.len() as u32)
    }
}

fn prop_f32(v: f32) -> Result<IPropertyValue> {
    PropertyValue::CreateSingle(v)?.cast()
}
fn prop_u32(v: u32) -> Result<IPropertyValue> {
    PropertyValue::CreateUInt32(v)?.cast()
}

pub fn backdrop_blur_brush(
    comp: &Compositor,
    blur_sigma: f32,
    saturation: f32,
) -> Result<CompositionEffectBrush> {
    let blur_sigma = blur_sigma.clamp(0.5, 100.0);
    let saturation = saturation.clamp(0.0, 2.0);
    const SRC: &str = "backdrop";
    let source: IGraphicsEffectSource =
        CompositionEffectSourceParameter::Create(&HSTRING::from(SRC))?.cast()?;
    let blur = D2dEffect::make(
        CLSID_D2D1GaussianBlur,
        vec![prop_f32(blur_sigma)?, prop_u32(0)?, prop_u32(1)?],
        vec![source],
    );
    let sat = D2dEffect::make(
        CLSID_D2D1Saturation,
        vec![prop_f32(saturation)?],
        vec![blur.cast()?],
    );
    let brush = comp.CreateEffectFactory(&sat)?.CreateBrush()?;
    brush.SetSourceParameter(&HSTRING::from(SRC), &comp.CreateBackdropBrush()?)?;
    Ok(brush)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tier {
    Full,
    Solid,
}

#[derive(Clone, Copy)]
pub struct GlassParams {
    pub size: (f32, f32),
    pub radius: f32,
    pub blur_sigma: f32,
    pub saturation: f32,
    pub tint: Color,
    pub tint_bottom: Color,
    pub rim: Color,
    pub solid: Color,
    pub highlight: f32,
    pub liquify: bool,
}

fn col(c: [u8; 4]) -> Color {
    Color {
        A: c[0],
        R: c[1],
        G: c[2],
        B: c[3],
    }
}

impl GlassParams {
    pub fn from_theme(theme: SurfaceTheme, size: (f32, f32), radius: f32) -> Self {
        let s = surface_spec(theme);
        Self {
            size,
            radius,
            blur_sigma: s.blur_sigma,
            saturation: s.saturation,
            tint: col(s.tint),
            tint_bottom: col(s.tint_bottom),
            rim: col(s.rim),
            solid: col(s.solid),
            highlight: s.highlight,
            liquify: s.liquify,
        }
    }
}

const FRINGE_INSET: f32 = 1.6;

fn shape_geo_metrics(w: f32, h: f32, r: f32, inset: f32, flush: bool) -> (f32, f32, f32, f32) {
    let ext = if flush { r.max(2.0) } else { 0.0 };
    let y_off = inset - ext;
    let gw = (w - 2.0 * inset).max(0.0);
    let gh = (h + ext - inset - if flush { inset } else { inset }).max(0.0);
    let gr = (r - inset).max(0.0);
    (y_off, gw, gh, gr)
}

pub struct GlassPanel {
    comp: Compositor,
    p: Cell<GlassParams>,
    tier: Cell<Tier>,
    notch_flush: Cell<bool>,
    ambient: Cell<Option<[u8; 3]>>,
    glass: SpriteVisual,
    tint: SpriteVisual,
    tint_brush: CompositionLinearGradientBrush,
    glow: SpriteVisual,
    glow_brush: CompositionLinearGradientBrush,
    spec: SpriteVisual,
    rim: ShapeVisual,
    outer_rim_geo: CompositionRoundedRectangleGeometry,
    rim_brush: CompositionLinearGradientBrush,
    rim_shape: CompositionSpriteShape,
    fringe_brush: CompositionLinearGradientBrush,
    fringe_geo: CompositionRoundedRectangleGeometry,
    clip_geo: CompositionRoundedRectangleGeometry,
    rim_geo: CompositionRoundedRectangleGeometry,
    wh: Cell<(f32, f32)>,
}

impl GlassPanel {
    pub fn new(comp: &Compositor, parent: &ContainerVisual, p: GlassParams) -> Result<Self> {
        let clip_geo = comp.CreateRoundedRectangleGeometry()?;
        clip_geo.SetCornerRadius(Vector2 {
            X: p.radius,
            Y: p.radius,
        })?;
        let clip = comp.CreateGeometricClip()?;
        clip.SetGeometry(&clip_geo)?;
        let clip: CompositionClip = clip.cast()?;

        let glass = comp.CreateSpriteVisual()?;
        glass.SetClip(&clip)?;

        let tint = comp.CreateSpriteVisual()?;
        let tint_brush = comp.CreateLinearGradientBrush()?;
        tint_brush.SetStartPoint(Vector2 { X: 0.0, Y: 0.0 })?;
        tint_brush.SetEndPoint(Vector2 { X: 0.0, Y: 1.0 })?;
        tint.SetBrush(&tint_brush)?;
        tint.SetClip(&clip)?;

        let glow = comp.CreateSpriteVisual()?;
        let glow_brush = comp.CreateLinearGradientBrush()?;
        glow_brush.SetStartPoint(Vector2 { X: 0.0, Y: 0.0 })?;
        glow_brush.SetEndPoint(Vector2 { X: 0.85, Y: 0.90 })?;
        glow.SetBrush(&glow_brush)?;
        glow.SetOpacity(0.0)?;
        glow.SetClip(&clip)?;

        let spec = comp.CreateSpriteVisual()?;
        let sg = comp.CreateLinearGradientBrush()?;
        sg.SetStartPoint(Vector2 { X: 0.0, Y: 0.0 })?;
        sg.SetEndPoint(Vector2 { X: 0.0, Y: 1.0 })?;
        let stops = sg.ColorStops()?;
        for (off, a) in [(0.0f32, 92u8), (0.14, 24), (0.48, 0), (0.86, 0), (1.0, 28)] {
            stops.Append(&comp.CreateColorGradientStopWithOffsetAndColor(
                off,
                Color {
                    A: a,
                    R: 255,
                    G: 255,
                    B: 255,
                },
            )?)?;
        }
        spec.SetBrush(&sg)?;
        spec.SetClip(&clip)?;

        let outer_rim_geo = comp.CreateRoundedRectangleGeometry()?;
        outer_rim_geo.SetOffset(Vector2 { X: 0.0, Y: 0.0 })?;
        outer_rim_geo.SetCornerRadius(Vector2 {
            X: p.radius,
            Y: p.radius,
        })?;
        let outer_rim_shape = comp.CreateSpriteShapeWithGeometry(&outer_rim_geo)?;
        outer_rim_shape.SetStrokeBrush(&comp.CreateColorBrushWithColor(Color {
            A: 82,
            R: 0,
            G: 0,
            B: 0,
        })?)?;
        outer_rim_shape.SetStrokeThickness(1.0)?;

        let rim_geo = comp.CreateRoundedRectangleGeometry()?;
        rim_geo.SetOffset(Vector2 { X: 0.6, Y: 0.6 })?;
        rim_geo.SetCornerRadius(Vector2 {
            X: (p.radius - 0.6).max(0.0),
            Y: (p.radius - 0.6).max(0.0),
        })?;
        let rim_shape = comp.CreateSpriteShapeWithGeometry(&rim_geo)?;
        let rim_brush = comp.CreateLinearGradientBrush()?;
        rim_shape.SetStrokeBrush(&rim_brush)?;
        rim_shape.SetStrokeThickness(1.3)?;

        let fringe_geo = comp.CreateRoundedRectangleGeometry()?;
        fringe_geo.SetOffset(Vector2 {
            X: FRINGE_INSET,
            Y: FRINGE_INSET,
        })?;
        fringe_geo.SetCornerRadius(Vector2 {
            X: (p.radius - FRINGE_INSET).max(0.0),
            Y: (p.radius - FRINGE_INSET).max(0.0),
        })?;
        let fringe_shape = comp.CreateSpriteShapeWithGeometry(&fringe_geo)?;
        let fringe_brush = comp.CreateLinearGradientBrush()?;
        fringe_brush.SetStartPoint(Vector2 { X: 0.0, Y: 0.0 })?;
        fringe_brush.SetEndPoint(Vector2 { X: 1.0, Y: 0.0 })?;
        fringe_shape.SetStrokeBrush(&fringe_brush)?;
        fringe_shape.SetStrokeThickness(1.9)?;

        let rim = comp.CreateShapeVisual()?;
        rim.SetClip(&clip)?;
        rim.Shapes()?.Append(&outer_rim_shape)?;
        rim.Shapes()?.Append(&fringe_shape)?;
        rim.Shapes()?.Append(&rim_shape)?;

        let kids = parent.Children()?;
        kids.InsertAtTop(&glass)?;
        kids.InsertAtTop(&tint)?;
        kids.InsertAtTop(&glow)?;
        kids.InsertAtTop(&spec)?;
        kids.InsertAtTop(&rim)?;

        let panel = Self {
            comp: comp.clone(),
            p: Cell::new(p),
            tier: Cell::new(Tier::Solid),
            notch_flush: Cell::new(false),
            ambient: Cell::new(None),
            glass,
            tint,
            tint_brush,
            glow,
            glow_brush,
            spec,
            rim,
            outer_rim_geo,
            rim_brush,
            rim_shape,
            fringe_brush,
            fringe_geo,
            clip_geo,
            rim_geo,
            wh: Cell::new(p.size),
        };
        panel.paint_tint()?;
        panel.paint_rim(p.size.0, p.size.1)?;
        panel.apply_tier(Tier::Full)?;
        panel.set_rect(0.0, 0.0, p.size.0, p.size.1)?;
        Ok(panel)
    }

    pub fn set_notch_flush(&self, flush: bool) -> Result<()> {
        if self.notch_flush.replace(flush) != flush {
            let (w, h) = self.wh.get();
            self.apply_geometries(w, h, self.p.get().radius)?;
        }
        Ok(())
    }

    pub fn set_ambient(&self, accent: Option<[u8; 3]>) -> Result<()> {
        if self.ambient.get() == accent {
            return Ok(());
        }
        self.ambient.set(accent);
        let stops = self.glow_brush.ColorStops()?;
        stops.Clear()?;
        if let Some([r, g, b]) = accent {
            stops.Append(&self.comp.CreateColorGradientStopWithOffsetAndColor(
                0.0,
                Color { A: 72, R: r, G: g, B: b },
            )?)?;
            stops.Append(&self.comp.CreateColorGradientStopWithOffsetAndColor(
                0.55,
                Color { A: 24, R: r, G: g, B: b },
            )?)?;
            stops.Append(&self.comp.CreateColorGradientStopWithOffsetAndColor(
                1.0,
                Color { A: 0, R: r, G: g, B: b },
            )?)?;
            self.glow.SetOpacity(1.0)?;
        } else {
            self.glow.SetOpacity(0.0)?;
        }
        Ok(())
    }

    fn paint_tint(&self) -> Result<()> {
        let p = self.p.get();
        let stops = self.tint_brush.ColorStops()?;
        stops.Clear()?;
        stops.Append(&self.comp.CreateColorGradientStopWithOffsetAndColor(0.0, p.tint)?)?;
        stops.Append(&self.comp.CreateColorGradientStopWithOffsetAndColor(1.0, p.tint_bottom)?)?;
        Ok(())
    }

    pub fn set_params(&self, p: GlassParams, tier: Tier) -> Result<()> {
        let mut cur = self.p.get();
        cur.blur_sigma = p.blur_sigma;
        cur.saturation = p.saturation;
        cur.tint = p.tint;
        cur.tint_bottom = p.tint_bottom;
        cur.rim = p.rim;
        cur.solid = p.solid;
        cur.highlight = p.highlight;
        cur.liquify = p.liquify;
        self.p.set(cur);
        self.paint_tint()?;
        let (w, h) = self.wh.get();
        self.paint_rim(w, h)?;
        self.apply_tier(tier)
    }

    fn apply_tier(&self, tier: Tier) -> Result<()> {
        let p = self.p.get();
        let use_blur = tier == Tier::Full && p.blur_sigma >= 0.5;
        let brush: CompositionBrush = if use_blur {
            match backdrop_blur_brush(&self.comp, p.blur_sigma, p.saturation) {
                Ok(b) => {
                    self.tier.set(Tier::Full);
                    self.tint.SetOpacity(1.0)?;
                    b.cast()?
                }
                Err(_) => {
                    self.tier.set(Tier::Solid);
                    self.tint.SetOpacity(0.0)?;
                    self.comp.CreateColorBrushWithColor(p.solid)?.cast()?
                }
            }
        } else {
            self.tier.set(Tier::Solid);
            self.tint.SetOpacity(0.0)?;
            self.comp.CreateColorBrushWithColor(p.solid)?.cast()?
        };
        self.glass.SetBrush(&brush)?;
        self.spec
            .SetOpacity((p.highlight * if self.tier.get() == Tier::Solid { 0.45 } else { 1.0 }).clamp(0.0, 1.0))?;
        Ok(())
    }

    fn paint_rim(&self, w: f32, h: f32) -> Result<()> {
        let p = self.p.get();
        self.wh.set((w, h));
        let stops = self.rim_brush.ColorStops()?;
        stops.Clear()?;
        let with = |k: f32| Color {
            A: (p.rim.A as f32 * k).round().clamp(0.0, 255.0) as u8,
            ..p.rim
        };
        if p.liquify {
            let h = h.max(4.0);
            let t = ((p.radius * 0.45 + 2.0) / h).clamp(0.04, 0.45);
            let b = (2.5 / h).clamp(0.02, 0.30);
            self.rim_brush.SetStartPoint(Vector2 { X: 0.0, Y: 0.0 })?;
            self.rim_brush.SetEndPoint(Vector2 { X: 0.0, Y: 1.0 })?;
            stops.Append(&self.comp.CreateColorGradientStopWithOffsetAndColor(0.0, p.rim)?)?;
            stops.Append(&self.comp.CreateColorGradientStopWithOffsetAndColor(t, with(0.32))?)?;
            stops.Append(
                &self
                    .comp
                    .CreateColorGradientStopWithOffsetAndColor((1.0 - b).max(t), with(0.32))?,
            )?;
            stops.Append(&self.comp.CreateColorGradientStopWithOffsetAndColor(1.0, with(0.58))?)?;
            self.rim_shape.SetStrokeThickness(1.25)?;
        } else {
            self.rim_brush.SetStartPoint(Vector2 { X: 0.0, Y: 0.0 })?;
            self.rim_brush.SetEndPoint(Vector2 { X: 1.0, Y: 1.0 })?;
            stops.Append(&self.comp.CreateColorGradientStopWithOffsetAndColor(0.0, p.rim)?)?;
            stops.Append(&self.comp.CreateColorGradientStopWithOffsetAndColor(0.5, with(0.38))?)?;
            stops.Append(&self.comp.CreateColorGradientStopWithOffsetAndColor(1.0, with(0.75))?)?;
            self.rim_shape.SetStrokeThickness(1.35)?;
        }

        let fstops = self.fringe_brush.ColorStops()?;
        fstops.Clear()?;
        if p.liquify {
            let f = (28.0 / w.max(1.0)).clamp(0.04, 0.45);
            let left = Color {
                A: 72,
                R: 255,
                G: 94,
                B: 132,
            };
            let right = Color {
                A: 72,
                R: 84,
                G: 176,
                B: 255,
            };
            for (o, c) in [
                (0.0, left),
                (f, Color { A: 0, ..left }),
                (1.0 - f, Color { A: 0, ..right }),
                (1.0, right),
            ] {
                fstops.Append(&self.comp.CreateColorGradientStopWithOffsetAndColor(o, c)?)?;
            }
        }
        Ok(())
    }

    fn apply_geometries(&self, w: f32, h: f32, r: f32) -> Result<()> {
        let flush = self.notch_flush.get();
        let (cy, cw, ch, cr) = shape_geo_metrics(w, h, r, 0.0, flush);
        self.clip_geo.SetOffset(Vector2 { X: 0.0, Y: cy })?;
        self.clip_geo.SetSize(Vector2 { X: cw, Y: ch })?;
        self.clip_geo.SetCornerRadius(Vector2 { X: cr, Y: cr })?;

        let (oy, ow, oh, or) = shape_geo_metrics(w, h, r, 0.0, flush);
        self.outer_rim_geo.SetOffset(Vector2 { X: 0.0, Y: oy })?;
        self.outer_rim_geo.SetSize(Vector2 { X: ow, Y: oh })?;
        self.outer_rim_geo.SetCornerRadius(Vector2 { X: or, Y: or })?;

        let (ry, rw, rh, rr) = shape_geo_metrics(w, h, r, 0.6, flush);
        self.rim_geo.SetOffset(Vector2 { X: 0.6, Y: ry })?;
        self.rim_geo.SetSize(Vector2 { X: rw, Y: rh })?;
        self.rim_geo.SetCornerRadius(Vector2 { X: rr, Y: rr })?;

        let (fy, fw, fh, fr) = shape_geo_metrics(w, h, r, FRINGE_INSET, flush);
        self.fringe_geo.SetOffset(Vector2 { X: FRINGE_INSET, Y: fy })?;
        self.fringe_geo.SetSize(Vector2 { X: fw, Y: fh })?;
        self.fringe_geo.SetCornerRadius(Vector2 { X: fr, Y: fr })?;
        Ok(())
    }

    pub fn set_rect(&self, x: f32, y: f32, w: f32, h: f32) -> Result<()> {
        let size = Vector2 { X: w, Y: h };
        let at = Vector3 { X: x, Y: y, Z: 0.0 };
        for v in [&self.glass, &self.tint, &self.glow, &self.spec] {
            v.SetSize(size)?;
            v.SetOffset(at)?;
        }
        self.rim.SetSize(size)?;
        self.rim.SetOffset(at)?;
        self.apply_geometries(w, h, self.p.get().radius)?;
        self.paint_rim(w, h)
    }

    pub fn set_radius(&self, r: f32) -> Result<()> {
        let mut p = self.p.get();
        p.radius = r;
        self.p.set(p);
        let (w, h) = self.wh.get();
        self.apply_geometries(w, h, r)?;
        self.paint_rim(w, h)
    }

    pub fn animate_rect(
        &self,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        radius: f32,
        ratio: f32,
        period_ms: u32,
    ) -> Result<()> {
        if period_ms == 0 {
            self.set_rect(x, y, w, h)?;
            return self.set_radius(radius);
        }
        let flush = self.notch_flush.get();
        let comp = &self.comp;
        let go = |obj: &CompositionObject, prop: &str, v: f32| -> Result<()> {
            let a = comp.CreateSpringScalarAnimation()?;
            a.SetDampingRatio(ratio)?;
            a.SetPeriod(TimeSpan {
                Duration: period_ms as i64 * 10_000,
            })?;
            a.SetFinalValue(
                &PropertyValue::CreateSingle(v)?.cast::<windows::Foundation::IReference<f32>>()?,
            )?;
            obj.StartAnimation(&HSTRING::from(prop), &a)
        };
        for v in [&self.glass, &self.tint, &self.glow, &self.spec] {
            let o: CompositionObject = v.cast()?;
            go(&o, "Size.X", w)?;
            go(&o, "Size.Y", h)?;
            go(&o, "Offset.X", x)?;
            go(&o, "Offset.Y", y)?;
        }
        let o: CompositionObject = self.rim.cast()?;
        go(&o, "Size.X", w)?;
        go(&o, "Size.Y", h)?;
        go(&o, "Offset.X", x)?;
        go(&o, "Offset.Y", y)?;

        let (cy, cw, ch, cr) = shape_geo_metrics(w, h, radius, 0.0, flush);
        let g: CompositionObject = self.clip_geo.cast()?;
        go(&g, "Offset.Y", cy)?;
        go(&g, "Size.X", cw)?;
        go(&g, "Size.Y", ch)?;
        go(&g, "CornerRadius.X", cr)?;
        go(&g, "CornerRadius.Y", cr)?;

        let (oy, ow, oh, or) = shape_geo_metrics(w, h, radius, 0.0, flush);
        let og: CompositionObject = self.outer_rim_geo.cast()?;
        go(&og, "Offset.Y", oy)?;
        go(&og, "Size.X", ow)?;
        go(&og, "Size.Y", oh)?;
        go(&og, "CornerRadius.X", or)?;
        go(&og, "CornerRadius.Y", or)?;

        let (ry, rw, rh, rr) = shape_geo_metrics(w, h, radius, 0.6, flush);
        let r: CompositionObject = self.rim_geo.cast()?;
        go(&r, "Offset.Y", ry)?;
        go(&r, "Size.X", rw)?;
        go(&r, "Size.Y", rh)?;
        go(&r, "CornerRadius.X", rr)?;
        go(&r, "CornerRadius.Y", rr)?;

        let (fy, fw, fh, fr) = shape_geo_metrics(w, h, radius, FRINGE_INSET, flush);
        let f: CompositionObject = self.fringe_geo.cast()?;
        go(&f, "Offset.Y", fy)?;
        go(&f, "Size.X", fw)?;
        go(&f, "Size.Y", fh)?;
        go(&f, "CornerRadius.X", fr)?;
        go(&f, "CornerRadius.Y", fr)?;

        let mut p = self.p.get();
        p.radius = radius;
        self.p.set(p);
        self.paint_rim(w, h)
    }
}
