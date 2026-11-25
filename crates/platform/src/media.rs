use crate::icons::Bitmap;
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError, Sender};
use std::time::Duration;
use windows::Foundation::TypedEventHandler;
use windows::Media::Control::{
    CurrentSessionChangedEventArgs, GlobalSystemMediaTransportControlsSession,
    GlobalSystemMediaTransportControlsSessionManager,
    GlobalSystemMediaTransportControlsSessionPlaybackStatus, MediaPropertiesChangedEventArgs,
    PlaybackInfoChangedEventArgs, SessionsChangedEventArgs, TimelinePropertiesChangedEventArgs,
};
use windows::Storage::Streams::{DataReader, IRandomAccessStreamReference};
use windows::Win32::System::Com::{CoInitializeEx, COINIT_MULTITHREADED};

pub enum ArtUpdate {
    Unchanged,
    None,
    New(Bitmap),
}

pub struct Snapshot {
    pub has_media: bool,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub playing: bool,
    pub playback_rate: f64,
    pub position_ms: i64,
    pub duration_ms: i64,
    pub can_seek: bool,
    pub art: ArtUpdate,
    pub app: String,
}

pub enum Command {
    Refresh,
    PlayPause,
    Next,
    Prev,
    SeekMs(i64),
}

pub struct Media {
    pub commands: Sender<Command>,
    pub snapshots: Receiver<Snapshot>,
}

fn read_art(r: Option<IRandomAccessStreamReference>) -> Option<Vec<u8>> {
    let stream = r?.OpenReadAsync().ok()?.join().ok()?;
    let reader = DataReader::CreateDataReader(&stream).ok()?;
    let mut all = Vec::new();
    loop {
        let n = reader.LoadAsync(65536).ok()?.join().ok()?;
        if n == 0 {
            break;
        }
        let mut chunk = vec![0u8; n as usize];
        reader.ReadBytes(&mut chunk).ok()?;
        all.extend_from_slice(&chunk);
        if all.len() > 8 * 1024 * 1024 {
            return None;
        }
    }
    (!all.is_empty()).then_some(all)
}

fn fnv(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in bytes {
        h ^= *b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

struct ArtState {
    key: String,
    hash: u64,
    since: std::time::Instant,
}

struct Subscriptions {
    session: Option<GlobalSystemMediaTransportControlsSession>,
    tokens: (i64, i64, i64),
}

impl Subscriptions {
    fn drop_session(&mut self) {
        if let Some(s) = self.session.take() {
            let _ = s.RemoveMediaPropertiesChanged(self.tokens.0);
            let _ = s.RemovePlaybackInfoChanged(self.tokens.1);
            let _ = s.RemoveTimelinePropertiesChanged(self.tokens.2);
        }
    }

    fn follow(&mut self, session: Option<GlobalSystemMediaTransportControlsSession>, tx: &Sender<Command>) {
        let same = match (&self.session, &session) {
            (Some(a), Some(b)) => a.SourceAppUserModelId().ok() == b.SourceAppUserModelId().ok(),
            (None, None) => true,
            _ => false,
        };
        if same {
            return;
        }
        self.drop_session();
        let Some(s) = session else { return };
        let t1 = tx.clone();
        let a = s.MediaPropertiesChanged(&TypedEventHandler::<_, MediaPropertiesChangedEventArgs>::new(move |_, _| {
            let _ = t1.send(Command::Refresh);
            Ok(())
        }));
        let t2 = tx.clone();
        let b = s.PlaybackInfoChanged(&TypedEventHandler::<_, PlaybackInfoChangedEventArgs>::new(move |_, _| {
            let _ = t2.send(Command::Refresh);
            Ok(())
        }));
        let t3 = tx.clone();
        let c = s.TimelinePropertiesChanged(&TypedEventHandler::<_, TimelinePropertiesChangedEventArgs>::new(move |_, _| {
            let _ = t3.send(Command::Refresh);
            Ok(())
        }));
        self.tokens = (a.unwrap_or(0), b.unwrap_or(0), c.unwrap_or(0));
        self.session = Some(s);
    }
}

pub fn spawn(notify: impl Fn() + Send + 'static) -> Media {
    let (cmd_tx, cmd_rx) = channel::<Command>();
    let (snap_tx, snap_rx) = channel::<Snapshot>();
    let tx_for_events = cmd_tx.clone();
    std::thread::spawn(move || {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        }
        let Some(manager) = GlobalSystemMediaTransportControlsSessionManager::RequestAsync()
            .ok()
            .and_then(|o| o.join().ok())
        else {
            crate::log::line("media", "SMTC manager unavailable");
            return;
        };
        let t = tx_for_events.clone();
        let _ = manager.CurrentSessionChanged(&TypedEventHandler::<_, CurrentSessionChangedEventArgs>::new(move |_, _| {
            let _ = t.send(Command::Refresh);
            Ok(())
        }));
        let t = tx_for_events.clone();
        let _ = manager.SessionsChanged(&TypedEventHandler::<_, SessionsChangedEventArgs>::new(move |_, _| {
            let _ = t.send(Command::Refresh);
            Ok(())
        }));

        let mut subs = Subscriptions {
            session: None,
            tokens: (0, 0, 0),
        };
        let mut art_state = ArtState {
            key: String::new(),
            hash: 0,
            since: std::time::Instant::now(),
        };
        loop {
            let current = choose(&manager);
            subs.follow(current.clone(), &tx_for_events);
            let snap = build(current.as_ref(), &mut art_state);
            if snap_tx.send(snap).is_err() {
                return;
            }
            notify();
            match cmd_rx.recv_timeout(Duration::from_secs(2)) {
                Err(RecvTimeoutError::Timeout) => {}
                Ok(Command::Refresh) => {
                    loop {
                        match cmd_rx.recv_timeout(Duration::from_millis(60)) {
                            Ok(Command::Refresh) => continue,
                            Ok(cmd) => act(subs.session.as_ref(), cmd),
                            Err(RecvTimeoutError::Disconnected) => return,
                            Err(RecvTimeoutError::Timeout) => {}
                        }
                        break;
                    }
                }
                Ok(other) => {
                    act(subs.session.as_ref(), other);
                    std::thread::sleep(Duration::from_millis(100));
                }
                Err(RecvTimeoutError::Disconnected) => return,
            }
        }
    });
    Media {
        commands: cmd_tx,
        snapshots: snap_rx,
    }
}

fn is_playing(s: &GlobalSystemMediaTransportControlsSession) -> bool {
    s.GetPlaybackInfo()
        .ok()
        .and_then(|p| p.PlaybackStatus().ok())
        .is_some_and(|st| st == GlobalSystemMediaTransportControlsSessionPlaybackStatus::Playing)
}

fn has_title(s: &GlobalSystemMediaTransportControlsSession) -> bool {
    s.TryGetMediaPropertiesAsync()
        .ok()
        .and_then(|o| o.join().ok())
        .and_then(|p| p.Title().ok())
        .is_some_and(|t| !t.is_empty())
}

fn choose(manager: &GlobalSystemMediaTransportControlsSessionManager) -> Option<GlobalSystemMediaTransportControlsSession> {
    let current = manager.GetCurrentSession().ok();
    if current.as_ref().is_some_and(is_playing) {
        return current;
    }
    let all: Vec<_> = manager.GetSessions().ok().map(|v| v.into_iter().collect()).unwrap_or_default();
    if let Some(s) = all.iter().find(|s| is_playing(s) && has_title(s)) {
        return Some(s.clone());
    }
    if current.as_ref().is_some_and(has_title) {
        return current;
    }
    all.into_iter().find(has_title).or(current)
}

fn act(session: Option<&GlobalSystemMediaTransportControlsSession>, cmd: Command) {
    let Some(s) = session else { return };
    match cmd {
        Command::PlayPause => {
            let _ = s.TryTogglePlayPauseAsync();
        }
        Command::Next => {
            let _ = s.TrySkipNextAsync();
        }
        Command::Prev => {
            let _ = s.TrySkipPreviousAsync();
        }
        Command::SeekMs(ms) => {
            if let Ok(timeline) = s.GetTimelineProperties() {
                let start = timeline.StartTime().map(|t| t.Duration).unwrap_or(0);
                if let Ok(op) = s.TryChangePlaybackPositionAsync(start.saturating_add(ms.saturating_mul(10_000))) {
                    let _ = op.join();
                }
            }
        }
        Command::Refresh => {}
    }
}

fn build(session: Option<&GlobalSystemMediaTransportControlsSession>, art_state: &mut ArtState) -> Snapshot {
    let empty = |st: &mut ArtState| {
        st.key.clear();
        st.hash = 0;
        Snapshot {
            has_media: false,
            title: String::new(),
            artist: String::new(),
            album: String::new(),
            playing: false,
            playback_rate: 1.0,
            position_ms: 0,
            duration_ms: 0,
            can_seek: false,
            art: ArtUpdate::None,
            app: String::new(),
        }
    };
    let Some(s) = session else {
        return empty(art_state);
    };
    let Some(props) = s.TryGetMediaPropertiesAsync().ok().and_then(|o| o.join().ok()) else {
        return empty(art_state);
    };
    let title = props.Title().map(|t| t.to_string()).unwrap_or_default();
    if title.is_empty() {
        return empty(art_state);
    }
    let artist = props.Artist().map(|t| t.to_string()).unwrap_or_default();
    let album = props.AlbumTitle().map(|t| t.to_string()).unwrap_or_default();
    let key = format!("{title}\u{1}{artist}");
    let new_track = art_state.key != key;
    if new_track {
        art_state.key = key;
        art_state.hash = 0;
        art_state.since = std::time::Instant::now();
    }
    let art = if new_track || art_state.since.elapsed() < Duration::from_secs(6) {
        match read_art(props.Thumbnail().ok()) {
            Some(bytes) => {
                let h = fnv(&bytes);
                if h == art_state.hash && !new_track {
                    ArtUpdate::Unchanged
                } else {
                    art_state.hash = h;
                    match crate::icons::decode_square(&bytes, 128) {
                        Some(bmp) => ArtUpdate::New(bmp),
                        None if new_track => ArtUpdate::None,
                        None => ArtUpdate::Unchanged,
                    }
                }
            }
            None if new_track => ArtUpdate::None,
            None => ArtUpdate::Unchanged,
        }
    } else {
        ArtUpdate::Unchanged
    };

    let playing = is_playing(s);
    let can_seek = s
        .GetPlaybackInfo()
        .ok()
        .and_then(|p| p.Controls().ok())
        .and_then(|c| c.IsPlaybackPositionEnabled().ok())
        .unwrap_or(false);
    let playback_rate = s.GetPlaybackInfo().ok().and_then(|p| p.PlaybackRate().ok())
        .and_then(|rate| rate.Value().ok()).filter(|r| r.is_finite()).unwrap_or(1.0);
    let (position_ms, duration_ms) = match s.GetTimelineProperties() {
        Ok(t) => {
            let ms = |r: windows::core::Result<windows::Foundation::TimeSpan>| {
                r.map(|v| v.Duration / 10_000).unwrap_or(0)
            };
            let (start, end, pos) = (ms(t.StartTime()), ms(t.EndTime()), ms(t.Position()));
            let duration = (end - start).max(0);
            // WinRT DateTime uses 100 ns ticks since 1601.
            let age_ms = t.LastUpdatedTime().ok().and_then(|updated| {
                let stamp = (updated.UniversalTime / 10_000).checked_sub(11_644_473_600_000)?;
                let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).ok()?;
                (stamp > 0).then(|| (now.as_millis() as i64 - stamp).max(0))
            }).unwrap_or(0);
            (relative_position(pos - start, duration, playing, playback_rate, age_ms), duration)
        }
        Err(_) => (0, 0),
    };
    let app = s.SourceAppUserModelId().map(|h| h.to_string()).unwrap_or_default();
    Snapshot {
        has_media: true,
        title,
        artist,
        album,
        playing,
        playback_rate,
        position_ms,
        duration_ms,
        can_seek,
        art,
        app,
    }
}

fn relative_position(position: i64, duration: i64, playing: bool, rate: f64, age_ms: i64) -> i64 {
    let advance = if playing { (age_ms as f64 * rate) as i64 } else { 0 };
    position.saturating_add(advance).clamp(0, duration)
}

#[cfg(test)]
mod tests {
    use super::relative_position;

    #[test]
    fn stale_timeline_advances_only_while_playing_and_stops_at_end() {
        assert_eq!(relative_position(1000, 10_000, true, 1.5, 2000), 4000);
        assert_eq!(relative_position(1000, 10_000, false, 1.5, 2000), 1000);
        assert_eq!(relative_position(9000, 10_000, true, 1.0, 2000), 10_000);
    }
}
