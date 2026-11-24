use std::time::Duration;

use serde_json::Value;
use tokio::runtime::Handle;
use winit::event_loop::EventLoopProxy;

use crate::desk::Ev;

const SEOUL: &str = "https://api.open-meteo.com/v1/forecast?latitude=37.57&longitude=126.98&current=weather_code,wind_speed_10m,wind_direction_10m&wind_speed_unit=kmh";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum Sky {
    Clear = 0,
    Cloud = 1,
    Fog = 2,
    Rain = 3,
    Snow = 4,
    Storm = 5,
    Hail = 6,
    Sunshower = 7,
}

#[derive(Clone, Copy, Debug)]
pub struct Conditions {
    pub sky: Sky,
    amount: f32,
    wind: f32,
    gust: f32,
}

impl Conditions {
    fn decode(code: u64, speed: f32, direction: f32) -> Option<Self> {
        if !speed.is_finite() || speed < 0.0 || !(0.0..=360.0).contains(&direction) {
            return None;
        }
        let (sky, amount) = match code {
            0 => (Sky::Clear, 0.0),
            1 => (Sky::Clear, 0.20),
            2 => (Sky::Cloud, 0.46),
            3 => (Sky::Cloud, 0.94),
            45 | 48 => (Sky::Fog, 0.83),
            51 | 56 => (Sky::Rain, 0.20),
            53 | 55 | 57 | 61 | 66 | 80 => (Sky::Rain, 0.43),
            63 | 81 => (Sky::Rain, 0.73),
            65 | 67 | 82 => (Sky::Rain, 1.0),
            71 | 77 | 85 => (Sky::Snow, 0.40),
            73 => (Sky::Snow, 0.68),
            75 | 86 => (Sky::Snow, 1.0),
            95 | 97 => (Sky::Storm, 0.92),
            96 | 99 => (Sky::Hail, 1.0),
            _ => return None,
        };
        let wind = -direction.to_radians().sin() * (speed / 60.0).min(1.0);
        Some(Self { sky, amount, wind, gust: ((speed - 25.0) / 35.0).clamp(0.0, 1.0) })
    }

    fn from_response(body: &Value) -> Option<Self> {
        let current = &body["current"];
        Self::decode(
            current["weather_code"].as_u64()?,
            current["wind_speed_10m"].as_f64()? as f32,
            current["wind_direction_10m"].as_f64()? as f32,
        )
    }

    pub fn uniform(self) -> [f32; 4] {
        [self.sky as u8 as f32, self.amount, self.wind, self.gust]
    }

    fn label(self) -> &'static str {
        match self.sky {
            Sky::Clear => "맑음",
            Sky::Cloud if self.amount < 0.7 => "구름 조금",
            Sky::Cloud => "흐림",
            Sky::Fog => "안개",
            Sky::Rain => "비",
            Sky::Snow => "눈",
            Sky::Storm => "뇌우",
            Sky::Hail => "우박",
            Sky::Sunshower => "여우비",
        }
    }
}

#[derive(Clone, Copy, Default)]
pub enum Weather {
    #[default]
    Loading,
    Live(Conditions),
    Previous(Conditions),
    Offline,
    Preview(Conditions),
}

impl Weather {
    pub fn conditions(self) -> Conditions {
        match self {
            Self::Live(c) | Self::Previous(c) | Self::Preview(c) => c,
            Self::Loading | Self::Offline => Conditions { sky: Sky::Sunshower, amount: 0.42, wind: -0.18, gust: 0.0 },
        }
    }

    pub fn has_source(self) -> bool {
        matches!(self, Self::Live(_) | Self::Previous(_))
    }

    pub fn caption(self) -> String {
        match self {
            Self::Loading => "서울 · 날씨 확인 중".into(),
            Self::Offline => "서울 · 날씨 연결 대기".into(),
            Self::Live(c) => format!("서울 · {}", c.label()),
            Self::Previous(c) => format!("서울 · {} (지난 날씨)", c.label()),
            Self::Preview(c) => format!("효과 미리보기 · {}", c.label()),
        }
    }
}

pub fn start(handle: &Handle, proxy: EventLoopProxy<Ev>) {
    if std::env::var_os("KASATERM_VIEW_STATE").is_some() && std::env::var_os("KASATERM_VIEW_KEY").is_some() {
        if let Ok(code) = std::env::var("KASATERM_VIEW_TEST_WEATHER_CODE") {
            let weather = code
                .parse()
                .ok()
                .and_then(|n| Conditions::decode(n, 16.0, 295.0))
                .map_or(Weather::Offline, Weather::Preview);
            let _ = proxy.send_event(Ev::Weather(weather));
            return;
        }
    }
    handle.spawn(async move {
        let client = crate::account::client();
        let mut previous = None;
        loop {
            let current = async {
                let response =
                    client.get(SEOUL).timeout(Duration::from_secs(8)).send().await.ok()?.error_for_status().ok()?;
                Conditions::from_response(&response.json().await.ok()?)
            }
            .await;
            let (weather, minutes) = match current {
                Some(c) => {
                    previous = Some(c);
                    (Weather::Live(c), 15)
                }
                None => (previous.map_or(Weather::Offline, Weather::Previous), 2),
            };
            if proxy.send_event(Ev::Weather(weather)).is_err() {
                break;
            }
            tokio::time::sleep(Duration::from_secs(minutes * 60)).await;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn seoul_forecast_selects_window_effects() {
        for (code, expected) in [
            (0, Sky::Clear),
            (2, Sky::Cloud),
            (48, Sky::Fog),
            (57, Sky::Rain),
            (82, Sky::Rain),
            (85, Sky::Snow),
            (97, Sky::Storm),
            (99, Sky::Hail),
        ] {
            let response = json!({"current":{"weather_code": code,"wind_speed_10m":30.0,"wind_direction_10m":270}});
            let weather = Conditions::from_response(&response).unwrap();
            assert_eq!(weather.sky, expected);
            assert!(weather.wind > 0.0);
        }
        assert!(Conditions::decode(999, 0.0, 0.0).is_none());
        assert!(Conditions::from_response(&json!({"current":{"weather_code":0,"wind_speed_10m":null}})).is_none());
    }
}
