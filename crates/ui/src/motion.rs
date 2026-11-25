use windows::Foundation::{PropertyValue, TimeSpan};
use windows::UI::Composition::{
    CompositionEasingFunction, CompositionObject, Compositor,
};
use windows_core::{Interface, Result, HSTRING};
use windows_numerics::Vector2;

pub fn dur(ms: u32) -> TimeSpan {
    TimeSpan {
        Duration: ms as i64 * 10_000,
    }
}

pub fn ease(comp: &Compositor) -> Result<CompositionEasingFunction> {
    comp.CreateCubicBezierEasingFunction(Vector2 { X: 0.2, Y: 0.0 }, Vector2 { X: 0.0, Y: 1.0 })?
        .cast()
}

pub fn spring(comp: &Compositor, obj: &impl Interface, prop: &str, to: f32, ratio: f32, period_ms: u32) -> Result<()> {
    let o: CompositionObject = obj.cast()?;
    let a = comp.CreateSpringScalarAnimation()?;
    a.SetDampingRatio(ratio)?;
    a.SetPeriod(dur(period_ms.max(12)))?;
    a.SetFinalValue(&PropertyValue::CreateSingle(to)?.cast::<windows::Foundation::IReference<f32>>()?)?;
    o.StartAnimation(&HSTRING::from(prop), &a)
}

pub fn scalar(comp: &Compositor, obj: &impl Interface, prop: &str, from: f32, to: f32, ms: u32) -> Result<()> {
    let e = ease(comp)?;
    let a = comp.CreateScalarKeyFrameAnimation()?;
    a.InsertKeyFrame(0.0, from)?;
    a.InsertKeyFrameWithEasingFunction(1.0, to, &e)?;
    a.SetDuration(dur(ms))?;
    obj.cast::<CompositionObject>()?
        .StartAnimation(&HSTRING::from(prop), &a)
}
