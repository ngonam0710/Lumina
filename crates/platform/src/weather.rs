use np_core::widgets::WeatherData;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use windows::Foundation::Uri;
use windows::Web::Http::HttpClient;
use windows_core::HSTRING;

static LATEST: Mutex<Option<WeatherData>> = Mutex::new(None);
static STARTED: std::sync::OnceLock<()> = std::sync::OnceLock::new();

pub fn latest() -> WeatherData {
    LATEST
        .lock()
        .ok()
        .and_then(|g| g.clone())
        .unwrap_or_default()
}

pub fn start_weather_poller(lat: f64, lon: f64, notify: impl Fn() + Send + Sync + 'static) {
    if STARTED.set(()).is_err() {
        return;
    }
    std::thread::Builder::new()
        .name("np-weather".into())
        .spawn(move || loop {
            if let Some(w) = fetch_open_meteo(lat, lon) {
                if let Ok(mut g) = LATEST.lock() {
                    *g = Some(w);
                }
                notify();
            }
            std::thread::sleep(Duration::from_secs(15 * 60));
        })
        .ok();
}

fn http_get(url: &str, timeout: Duration) -> Option<String> {
    let client = HttpClient::new().ok()?;
    let uri = Uri::CreateUri(&HSTRING::from(url)).ok()?;
    let op = client.GetAsync(&uri).ok()?;
    let t0 = Instant::now();
    loop {
        match op.Status().ok().map(|s| s.0) {
            Some(1) => break,
            Some(0) => {
                if t0.elapsed() > timeout {
                    let _ = op.Cancel();
                    return None;
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            _ => return None,
        }
    }
    let resp = op.GetResults().ok()?;
    if resp.StatusCode().ok()?.0 != 200 {
        return None;
    }
    let read = resp.Content().ok()?.ReadAsStringAsync().ok()?;
    let t1 = Instant::now();
    loop {
        match read.Status().ok().map(|s| s.0) {
            Some(1) => break,
            Some(0) => {
                if t1.elapsed() > timeout {
                    let _ = read.Cancel();
                    return None;
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            _ => return None,
        }
    }
    read.GetResults().ok().map(|h| h.to_string())
}

fn wmo_condition(code: i64) -> &'static str {
    match code {
        0 => "Clear",
        1 | 2 => "Partly cloudy",
        3 => "Overcast",
        45 | 48 => "Foggy",
        51..=57 => "Drizzle",
        61..=67 | 80..=82 => "Rainy",
        71..=77 | 85 | 86 => "Snow",
        95..=99 => "Thunderstorm",
        _ => "Cloudy",
    }
}

pub fn fetch_open_meteo(lat: f64, lon: f64) -> Option<WeatherData> {
    let url = format!(
        "https://api.open-meteo.com/v1/forecast?latitude={lat:.4}&longitude={lon:.4}&current=temperature_2m,weather_code,is_day&daily=temperature_2m_max,temperature_2m_min&timezone=auto"
    );
    let body = http_get(&url, Duration::from_secs(6))?;
    let v: serde_json::Value = serde_json::from_str(&body).ok()?;
    let cur = v.get("current")?;
    let temp_c = cur.get("temperature_2m")?.as_f64()?.round() as i32;
    let code = cur.get("weather_code")?.as_i64().unwrap_or(2);
    let is_night = cur.get("is_day").and_then(|d| d.as_i64()).unwrap_or(1) == 0;
    let daily = v.get("daily")?;
    let high_c = daily
        .get("temperature_2m_max")?
        .get(0)?
        .as_f64()?
        .round() as i32;
    let low_c = daily
        .get("temperature_2m_min")?
        .get(0)?
        .as_f64()?
        .round() as i32;
    Some(WeatherData {
        temp_c,
        condition: wmo_condition(code).to_string(),
        high_c,
        low_c,
        is_night,
        valid: true,
    })
}
