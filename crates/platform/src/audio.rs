use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc::{channel, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use windows::core::{implement, PCWSTR};
use windows::Win32::Foundation::PROPERTYKEY;
use windows::Win32::Media::Audio::Endpoints::{
    IAudioEndpointVolume, IAudioEndpointVolumeCallback, IAudioEndpointVolumeCallback_Impl,
};
use windows::Win32::Media::Audio::{
    eCapture, eConsole, eRender, EDataFlow, ERole, IAudioCaptureClient, IAudioClient, IMMDevice,
    IMMDeviceEnumerator, IMMNotificationClient, IMMNotificationClient_Impl, MMDeviceEnumerator,
    AUDCLNT_SHAREMODE_SHARED, AUDCLNT_STREAMFLAGS_LOOPBACK, AUDIO_VOLUME_NOTIFICATION_DATA,
    DEVICE_STATE, WAVEFORMATEX, WAVEFORMATEXTENSIBLE,
};
use windows::Win32::System::Com::StructuredStorage::{PropVariantClear, PropVariantToStringAlloc};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize, CLSCTX_ALL,
    COINIT_MULTITHREADED, STGM_READ,
};
use windows::Win32::UI::Shell::PropertiesSystem::IPropertyStore;

static OUT_VOL: AtomicU32 = AtomicU32::new(0);
static OUT_MUTE: AtomicBool = AtomicBool::new(false);
static OUT_OK: AtomicBool = AtomicBool::new(false);
static MIC_MUTE: AtomicBool = AtomicBool::new(false);
static MIC_OK: AtomicBool = AtomicBool::new(false);
static OUT_NAME: Mutex<String> = Mutex::new(String::new());

static SPEC: [AtomicU32; 16] = [const { AtomicU32::new(0) }; 16];
static BANDS: [AtomicU32; 3] = [const { AtomicU32::new(0) }; 3];

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AudioState {
    pub volume: f32,
    pub muted: bool,
    pub available: bool,
    pub mic_muted: bool,
    pub mic_available: bool,
}

pub fn latest() -> AudioState {
    AudioState {
        volume: f32::from_bits(OUT_VOL.load(Ordering::Relaxed)),
        muted: OUT_MUTE.load(Ordering::Relaxed),
        available: OUT_OK.load(Ordering::Relaxed),
        mic_muted: MIC_MUTE.load(Ordering::Relaxed),
        mic_available: MIC_OK.load(Ordering::Relaxed),
    }
}

pub fn output_name() -> String {
    OUT_NAME.lock().map(|g| g.clone()).unwrap_or_default()
}

pub fn spectrum16() -> [f32; 16] {
    let mut o = [0.0; 16];
    for (i, v) in SPEC.iter().enumerate() {
        o[i] = f32::from_bits(v.load(Ordering::Relaxed));
    }
    o
}

pub fn bands3() -> [f32; 3] {
    [
        f32::from_bits(BANDS[0].load(Ordering::Relaxed)),
        f32::from_bits(BANDS[1].load(Ordering::Relaxed)),
        f32::from_bits(BANDS[2].load(Ordering::Relaxed)),
    ]
}

pub enum Cmd {
    SetVolume(f32),
    SetMuted(bool),
    ToggleMute,
    ToggleMicMute,
    Refresh,
}

pub struct Audio {
    pub commands: Sender<Cmd>,
}

fn read_name(device: &IMMDevice) -> Option<String> {
    const KEY: PROPERTYKEY = PROPERTYKEY {
        fmtid: windows::core::GUID::from_u128(0xa45c254e_df1c_4efd_8020_67d146a850e0),
        pid: 14,
    };
    unsafe {
        let store: IPropertyStore = device.OpenPropertyStore(STGM_READ).ok()?;
        let mut pv = store.GetValue(&KEY).ok()?;
        let s = PropVariantToStringAlloc(&pv).ok().and_then(|p| {
            let t = p.to_string().ok();
            CoTaskMemFree(Some(p.0 as *const _));
            t
        });
        let _ = PropVariantClear(&mut pv);
        s.filter(|t| !t.is_empty())
    }
}

#[implement(IAudioEndpointVolumeCallback)]
struct VolCb {
    is_mic: bool,
    notify: Arc<dyn Fn() + Send + Sync>,
}

impl IAudioEndpointVolumeCallback_Impl for VolCb_Impl {
    fn OnNotify(&self, pnotify: *mut AUDIO_VOLUME_NOTIFICATION_DATA) -> windows::core::Result<()> {
        if !pnotify.is_null() {
            let d = unsafe { &*pnotify };
            if self.is_mic {
                MIC_MUTE.store(d.bMuted.as_bool(), Ordering::Relaxed);
            } else {
                OUT_VOL.store(d.fMasterVolume.to_bits(), Ordering::Relaxed);
                OUT_MUTE.store(d.bMuted.as_bool(), Ordering::Relaxed);
            }
            (self.notify)();
        }
        Ok(())
    }
}

#[implement(IMMNotificationClient)]
struct DevCb {
    tx: Sender<Cmd>,
}

impl IMMNotificationClient_Impl for DevCb_Impl {
    fn OnDeviceStateChanged(&self, _: &PCWSTR, _: DEVICE_STATE) -> windows::core::Result<()> {
        Ok(())
    }
    fn OnDeviceAdded(&self, _: &PCWSTR) -> windows::core::Result<()> {
        Ok(())
    }
    fn OnDeviceRemoved(&self, _: &PCWSTR) -> windows::core::Result<()> {
        Ok(())
    }
    fn OnDefaultDeviceChanged(&self, flow: EDataFlow, role: ERole, _: &PCWSTR) -> windows::core::Result<()> {
        if (flow == eRender || flow == eCapture) && role == eConsole {
            let _ = self.tx.send(Cmd::Refresh);
        }
        Ok(())
    }
    fn OnPropertyValueChanged(&self, _: &PCWSTR, _: &PROPERTYKEY) -> windows::core::Result<()> {
        Ok(())
    }
}

struct Attached {
    ep: IAudioEndpointVolume,
    cb: IAudioEndpointVolumeCallback,
}

impl Drop for Attached {
    fn drop(&mut self) {
        unsafe {
            let _ = self.ep.UnregisterControlChangeNotify(&self.cb);
        }
    }
}

fn attach_flow(
    enumerator: &IMMDeviceEnumerator,
    flow: EDataFlow,
    is_mic: bool,
    notify: &Arc<dyn Fn() + Send + Sync>,
) -> Option<Attached> {
    unsafe {
        let device = enumerator.GetDefaultAudioEndpoint(flow, eConsole).ok()?;
        if !is_mic {
            if let (Some(n), Ok(mut g)) = (read_name(&device), OUT_NAME.lock()) {
                *g = n;
            }
        }
        let ep: IAudioEndpointVolume = device.Activate(CLSCTX_ALL, None).ok()?;
        let cb: IAudioEndpointVolumeCallback = VolCb {
            is_mic,
            notify: notify.clone(),
        }
        .into();
        ep.RegisterControlChangeNotify(&cb).ok()?;
        if is_mic {
            if let Ok(m) = ep.GetMute() {
                MIC_MUTE.store(m.as_bool(), Ordering::Relaxed);
            }
        } else {
            if let Ok(v) = ep.GetMasterVolumeLevelScalar() {
                OUT_VOL.store(v.to_bits(), Ordering::Relaxed);
            }
            if let Ok(m) = ep.GetMute() {
                OUT_MUTE.store(m.as_bool(), Ordering::Relaxed);
            }
        }
        Some(Attached { ep, cb })
    }
}

pub fn spawn(notify: impl Fn() + Send + Sync + 'static) -> Audio {
    let (tx, rx) = channel::<Cmd>();
    let tx_dev = tx.clone();
    let notify: Arc<dyn Fn() + Send + Sync> = Arc::new(notify);
    std::thread::Builder::new()
        .name("np-audio-mta".into())
        .spawn(move || unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            let Ok(enumerator) = CoCreateInstance::<_, IMMDeviceEnumerator>(&MMDeviceEnumerator, None, CLSCTX_ALL) else {
                crate::log::line("audio", "enumerator failed");
                return;
            };
            let dev_cb: IMMNotificationClient = DevCb { tx: tx_dev }.into();
            let _ = enumerator.RegisterEndpointNotificationCallback(&dev_cb);
            let mut out_at = attach_flow(&enumerator, eRender, false, &notify);
            let mut mic_at = attach_flow(&enumerator, eCapture, true, &notify);
            OUT_OK.store(out_at.is_some(), Ordering::Relaxed);
            MIC_OK.store(mic_at.is_some(), Ordering::Relaxed);
            notify();
            while let Ok(cmd) = rx.recv() {
                match cmd {
                    Cmd::Refresh => {
                        drop(out_at.take());
                        drop(mic_at.take());
                        out_at = attach_flow(&enumerator, eRender, false, &notify);
                        mic_at = attach_flow(&enumerator, eCapture, true, &notify);
                        OUT_OK.store(out_at.is_some(), Ordering::Relaxed);
                        MIC_OK.store(mic_at.is_some(), Ordering::Relaxed);
                        notify();
                    }
                    Cmd::SetVolume(v) => {
                        if let Some(a) = &out_at {
                            let v = v.clamp(0.0, 1.0);
                            let _ = a.ep.SetMasterVolumeLevelScalar(v, std::ptr::null());
                            if v > 0.0 && OUT_MUTE.load(Ordering::Relaxed) {
                                let _ = a.ep.SetMute(false, std::ptr::null());
                            }
                        }
                    }
                    Cmd::SetMuted(m) => {
                        if let Some(a) = &out_at {
                            let _ = a.ep.SetMute(m, std::ptr::null());
                        }
                    }
                    Cmd::ToggleMute => {
                        if let Some(a) = &out_at {
                            let now = a.ep.GetMute().map(|m| m.as_bool()).unwrap_or(false);
                            let _ = a.ep.SetMute(!now, std::ptr::null());
                        }
                    }
                    Cmd::ToggleMicMute => {
                        if let Some(a) = &mic_at {
                            let now = a.ep.GetMute().map(|m| m.as_bool()).unwrap_or(false);
                            let _ = a.ep.SetMute(!now, std::ptr::null());
                            MIC_MUTE.store(!now, Ordering::Relaxed);
                            notify();
                        }
                    }
                }
            }
            drop(out_at);
            drop(mic_at);
            CoUninitialize();
        })
        .ok();
    Audio { commands: tx }
}

pub struct Loopback {
    stop: Arc<AtomicBool>,
    handle: Option<std::thread::JoinHandle<()>>,
}

impl Drop for Loopback {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        drop(self.handle.take());
    }
}

#[derive(Clone, Copy)]
enum SampleKind {
    F32,
    I16,
    I32,
}

unsafe fn sample_kind(fmt: *const WAVEFORMATEX) -> Option<SampleKind> {
    let f = &*fmt;
    let tag = f.wFormatTag;
    let bits = f.wBitsPerSample;
    match (tag, bits) {
        (3, 32) => Some(SampleKind::F32),
        (1, 16) => Some(SampleKind::I16),
        (1, 32) => Some(SampleKind::I32),
        (0xFFFE, _) if f.cbSize >= 22 => {
            let ext = &*(fmt as *const WAVEFORMATEXTENSIBLE);
            match (ext.SubFormat.data1, bits) {
                (3, 32) => Some(SampleKind::F32),
                (1, 16) => Some(SampleKind::I16),
                (1, 32) => Some(SampleKind::I32),
                _ => None,
            }
        }
        _ => None,
    }
}

fn device_id(d: &IMMDevice) -> String {
    unsafe {
        match d.GetId() {
            Ok(id) => {
                let s = PCWSTR(id.0).to_string().unwrap_or_default();
                CoTaskMemFree(Some(id.0 as *const _));
                s
            }
            Err(_) => String::new(),
        }
    }
}

pub fn start_loopback() -> Loopback {
    let stop = Arc::new(AtomicBool::new(false));
    let flag = stop.clone();
    let handle = std::thread::Builder::new()
        .name("np-loopback".into())
        .spawn(move || unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            let mut smooth = [0.0f32; 16];
            while !flag.load(Ordering::Relaxed) {
                let _ = run_loopback_session(&flag, &mut smooth);
                for _ in 0..20 {
                    if flag.load(Ordering::Relaxed) {
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(100));
                }
            }
            CoUninitialize();
        })
        .ok();
    Loopback { stop, handle }
}

unsafe fn run_loopback_session(stop: &AtomicBool, smooth: &mut [f32; 16]) -> Result<(), ()> {
    let enumerator: IMMDeviceEnumerator =
        CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).map_err(|_| ())?;
    let device = enumerator.GetDefaultAudioEndpoint(eRender, eConsole).map_err(|_| ())?;
    let id = device_id(&device);
    let client: IAudioClient = device.Activate(CLSCTX_ALL, None).map_err(|_| ())?;
    let fmt = client.GetMixFormat().map_err(|_| ())?;
    let channels = (*fmt).nChannels as usize;
    let kind = sample_kind(fmt);
    let init = client.Initialize(
        AUDCLNT_SHAREMODE_SHARED,
        AUDCLNT_STREAMFLAGS_LOOPBACK,
        10_000_000,
        0,
        fmt,
        None,
    );
    let bytes_per = ((*fmt).wBitsPerSample / 8) as usize;
    CoTaskMemFree(Some(fmt as *const _));
    init.map_err(|_| ())?;
    let (Some(kind), true) = (kind, channels > 0 && bytes_per > 0) else {
        return Err(());
    };
    let capture: IAudioCaptureClient = client.GetService().map_err(|_| ())?;
    client.Start().map_err(|_| ())?;

    let mut mono = Vec::with_capacity(4096);
    let mut last_check = Instant::now();
    while !stop.load(Ordering::Relaxed) {
        std::thread::sleep(Duration::from_millis(32));
        if last_check.elapsed() >= Duration::from_secs(2) {
            last_check = Instant::now();
            if let Ok(d) = enumerator.GetDefaultAudioEndpoint(eRender, eConsole) {
                if device_id(&d) != id {
                    let _ = client.Stop();
                    return Err(());
                }
            }
        }
        let mut got_audio = false;
        loop {
            let n = capture.GetNextPacketSize().map_err(|_| ())?;
            if n == 0 {
                break;
            }
            let (mut data, mut frames, mut flags) = (std::ptr::null_mut::<u8>(), 0u32, 0u32);
            capture
                .GetBuffer(&mut data, &mut frames, &mut flags, None, None)
                .map_err(|_| ())?;
            let silent = (flags & 0x2) != 0;
            mono.clear();
            for f in 0..frames as usize {
                let mut acc = 0.0f32;
                if !silent && !data.is_null() {
                    for ch in 0..channels {
                        let p = data.add((f * channels + ch) * bytes_per);
                        acc += match kind {
                            SampleKind::F32 => (p as *const f32).read_unaligned(),
                            SampleKind::I16 => (p as *const i16).read_unaligned() as f32 / 32768.0,
                            SampleKind::I32 => {
                                (p as *const i32).read_unaligned() as f32 / 2_147_483_648.0
                            }
                        };
                    }
                }
                mono.push(acc / channels as f32);
            }
            let _ = capture.ReleaseBuffer(frames);
            if !mono.is_empty() {
                got_audio = true;
                let chunk = (mono.len() / 16).max(1);
                for b in 0..16 {
                    let start = (b * chunk).min(mono.len());
                    let end = ((b + 1) * chunk).min(mono.len());
                    let rms = if end > start {
                        let sum: f32 = mono[start..end].iter().map(|s| s * s).sum();
                        (sum / (end - start) as f32).sqrt()
                    } else {
                        0.0
                    };
                    let val = (rms * 4.5).clamp(0.0, 1.0);
                    smooth[b] = smooth[b] * 0.55 + val * 0.45;
                    SPEC[b].store(smooth[b].to_bits(), Ordering::Relaxed);
                }
                let b0 = smooth[0..5].iter().sum::<f32>() / 5.0;
                let b1 = smooth[5..11].iter().sum::<f32>() / 6.0;
                let b2 = smooth[11..16].iter().sum::<f32>() / 5.0;
                BANDS[0].store(b0.to_bits(), Ordering::Relaxed);
                BANDS[1].store(b1.to_bits(), Ordering::Relaxed);
                BANDS[2].store(b2.to_bits(), Ordering::Relaxed);
            }
        }
        if !got_audio {
            for b in 0..16 {
                smooth[b] *= 0.82;
                SPEC[b].store(smooth[b].to_bits(), Ordering::Relaxed);
            }
            for k in 0..3 {
                let v = f32::from_bits(BANDS[k].load(Ordering::Relaxed)) * 0.82;
                BANDS[k].store(v.to_bits(), Ordering::Relaxed);
            }
        }
    }
    let _ = client.Stop();
    Ok(())
}
