mod panel;
pub use panel::Panel;

use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, bail, ensure, Context, Result};
use base64::{engine::general_purpose::STANDARD, Engine};
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::sync::mpsc;
use tokio::time::timeout;

use crate::roster::Machine;

pub const SPOTS: [&str; 6] = ["top1", "top2", "top3", "left1", "right1", "right2"];
pub const NAMES: [&str; 6] = ["상단 1", "상단 2", "상단 3", "후면", "측면 상단", "측면 하단"];
pub type Color = [u8; 3];

pub struct Config {
    device: String,
    host: String,
    token: String,
}

impl Config {
    pub fn load() -> Option<Arc<Self>> {
        let path = if let Some(p) = std::env::var_os("KASADESK_RGB_CONFIG") {
            PathBuf::from(p)
        } else {
            if std::env::var_os("KASATERM_VIEW_STATE").is_some() || std::env::var_os("KASATERM_VIEW_KEY").is_some() {
                return None;
            }
            #[cfg(target_os = "macos")]
            let base = PathBuf::from(std::env::var_os("HOME")?).join("Library/Application Support");
            #[cfg(windows)]
            let base = PathBuf::from(std::env::var_os("LOCALAPPDATA")?);
            #[cfg(not(any(target_os = "macos", windows)))]
            let base = std::env::var_os("XDG_DATA_HOME")
                .map(PathBuf::from)
                .or_else(|| Some(PathBuf::from(std::env::var_os("HOME")?).join(".local/share")))?;
            base.join("KasaDesk").join("rgb.json")
        };
        let config: Value = serde_json::from_reader(std::fs::File::open(path).ok()?).ok()?;
        let host = config["ssh_host"].as_str()?;
        let device = config["device_id"].as_str()?;
        let token = config["token"].as_str()?;
        if host.is_empty()
            || host.len() > 128
            || !host.as_bytes()[0].is_ascii_alphanumeric()
            || !host.bytes().all(|c| c.is_ascii_alphanumeric() || b"_.-".contains(&c))
            || device.len() != 64
            || !device.bytes().all(|c| c.is_ascii_hexdigit())
            || token.len() != 43
            || !token.bytes().all(|c| c.is_ascii_alphanumeric() || b"-_".contains(&c))
        {
            return None;
        }
        Some(Arc::new(Self { host: host.into(), device: device.to_ascii_lowercase(), token: token.into() }))
    }

    pub fn matches(&self, m: &Machine) -> bool {
        m.kasanet["id"].as_str() == Some(&self.device)
    }
}

pub fn parse_color(text: &str) -> Option<Color> {
    if text.len() != 6 || !text.bytes().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let v = u32::from_str_radix(text, 16).ok()?;
    Some([(v >> 16) as u8, (v >> 8) as u8, v as u8])
}
pub fn hex(c: Color) -> String {
    format!("{:02X}{:02X}{:02X}", c[0], c[1], c[2])
}

#[derive(Copy, Clone, Default)]
pub struct Fan {
    pub inner: Option<Color>,
    pub outer: Option<Color>,
    pub fixed: Option<Color>,
    pub rpm: Option<u32>,
    pub mapped: bool,
}

pub struct Snapshot {
    pub serial: u64,
    pub connected: bool,
    pub fans: [Fan; 6],
    pub liquid: Option<f64>,
    pub pump: Option<u32>,
    pub gpu: Option<f64>,
    pub lcd: Option<image::RgbaImage>,
}

impl Snapshot {
    fn parse(v: &Value) -> Result<Self> {
        let mut fans = [Fan::default(); 6];
        let rows = v["fans"].as_array().context("팬 목록 없음")?;
        ensure!(rows.len() == 6, "팬 목록 오류");
        for (i, row) in rows.iter().enumerate() {
            ensure!(row["spot"].as_str() == Some(SPOTS[i]), "팬 자리 불일치");
            let color = |k: &str| row[k].as_str().and_then(parse_color);
            fans[i] = Fan {
                inner: color("inner"),
                outer: color("outer"),
                fixed: color("fixed"),
                rpm: row["rpm"].as_u64().and_then(|n| n.try_into().ok()),
                mapped: row["mapped"] == true,
            };
        }
        let lcd = v["lcd"].as_str().and_then(|s| {
            let b = STANDARD.decode(s).ok()?;
            let reader = image::ImageReader::with_format(std::io::Cursor::new(&b), image::ImageFormat::Jpeg);
            let (w, h) = reader.into_dimensions().ok()?;
            if w == 0 || h == 0 || w > 1024 || h > 1024 {
                return None;
            }
            let image = image::load_from_memory_with_format(&b, image::ImageFormat::Jpeg).ok()?.resize_exact(
                192,
                192,
                image::imageops::FilterType::Triangle,
            );
            let mut thumb = image.to_rgba8();
            let (w, h) = thumb.dimensions();
            for (x, y, px) in thumb.enumerate_pixels_mut() {
                let a = (x as f32 + 0.5) * 2.0 / w as f32 - 1.0;
                let b = (y as f32 + 0.5) * 2.0 / h as f32 - 1.0;
                px[3] = (((1.0 - a.hypot(b)) * w.min(h) as f32 * 0.5 + 0.5).clamp(0.0, 1.0) * 255.0) as u8;
            }
            Some(thumb)
        });
        static SERIAL: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        Ok(Self {
            serial: SERIAL.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            connected: v["connected"] == true,
            fans,
            liquid: v["liquid"].as_f64(),
            pump: v["pump"].as_u64().and_then(|n| n.try_into().ok()),
            gpu: v["gpu"].as_f64(),
            lcd,
        })
    }

    pub fn lights(&self) -> [[f32; 4]; 12] {
        std::array::from_fn(|i| {
            let fan = self.fans[i / 2];
            let c = if i % 2 == 0 { fan.inner } else { fan.outer };
            if !self.connected {
                return [0.0; 4];
            }
            c.map_or([0.0; 4], |[r, g, b]| [r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, 1.0])
        })
    }
}

struct Pipe {
    _ssh: Child,
    input: ChildStdin,
    output: BufReader<ChildStdout>,
}

impl Pipe {
    fn open(cfg: &Config) -> Result<Self> {
        let mut command = Command::new("ssh");
        command
            .args([
                "-T",
                "-oBatchMode=yes",
                "-oStrictHostKeyChecking=yes",
                "-oForwardAgent=no",
                "-oClearAllForwardings=yes",
                "-oConnectTimeout=6",
                "-oServerAliveInterval=5",
                "-oServerAliveCountMax=1",
                &cfg.host,
                "python -I -u .kasadesk-rgb/bridge.py",
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true);
        #[cfg(windows)]
        command.creation_flags(0x08000000);
        let mut ssh = command.spawn().context("SSH 실행 실패")?;
        let input = ssh.stdin.take().context("SSH 입력 없음")?;
        let output = BufReader::new(ssh.stdout.take().context("SSH 출력 없음")?);
        Ok(Self { _ssh: ssh, input, output })
    }

    async fn request(&mut self, cfg: &Config, mut request: Value) -> Result<Value> {
        request["v"] = 1.into();
        request["token"] = cfg.token.as_str().into();
        let mut line = serde_json::to_vec(&request)?;
        line.push(b'\n');
        timeout(Duration::from_secs(12), async {
            self.input.write_all(&line).await?;
            self.input.flush().await?;
            let mut reply = Vec::new();
            let mut bounded = (&mut self.output).take(400_001);
            bounded.read_until(b'\n', &mut reply).await?;
            ensure!(reply.len() <= 400_000 && reply.last() == Some(&b'\n'), "SSH 연결 끊김");
            let mut v: Value = serde_json::from_slice(&reply).context("브리지 응답 오류")?;
            if v["ok"] != true {
                if v["error"] == "unpaired" {
                    bail!("페어링을 다시 해 주세요")
                }
                bail!("집 PC에서 RGB Control 실행 상태를 확인해 주세요")
            }
            Ok(v["data"].take())
        })
        .await
        .map_err(|_| anyhow!("응답 시간 초과"))?
    }
}

pub struct Update {
    pub result: Result<Snapshot, String>,
    pub applied: bool,
}
pub struct Session {
    tx: mpsc::Sender<(usize, Option<Color>)>,
    task: tokio::task::JoinHandle<()>,
}
impl Drop for Session {
    fn drop(&mut self) {
        self.task.abort()
    }
}
impl Session {
    pub fn start(rt: &tokio::runtime::Handle, cfg: Arc<Config>, notify: impl Fn(Update) + Send + Sync + 'static) -> Self {
        let (tx, mut rx) = mpsc::channel::<(usize, Option<Color>)>(1);
        let task = rt.spawn(async move {
            let run = async {
                let mut pipe = Pipe::open(&cfg)?;
                let mut applied = false;
                loop {
                    let data = pipe.request(&cfg, json!({"op":"snapshot"})).await?;
                    notify(Update { result: Ok(Snapshot::parse(&data)?), applied });
                    applied = false;
                    match timeout(Duration::from_millis(1600), rx.recv()).await {
                        Ok(Some((spot, color))) => {
                            pipe.request(&cfg, json!({"op":"color","spot":SPOTS[spot],"color":color.map(hex)})).await?;
                            applied = true;
                        }
                        Ok(None) => return Ok::<_, anyhow::Error>(()),
                        Err(_) => {}
                    }
                }
            }
            .await;
            if let Err(e) = run {
                notify(Update { result: Err(e.to_string()), applied: false });
            }
        });
        Self { tx, task }
    }
    pub fn color(&self, spot: usize, color: Option<Color>) -> bool {
        spot < 6 && self.tx.try_send((spot, color)).is_ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn color_values_and_port_samples_keep_inner_outer_order() {
        assert_eq!(parse_color("fF004A"), Some([255, 0, 74]));
        for value in ["abc", "#ff004a", "é0011", "00xx11"] {
            assert!(parse_color(value).is_none());
        }
        let fans: Vec<_> =
            SPOTS.iter().map(|spot| json!({"spot":spot,"inner":"000000","outer":"ff004a","mapped":true})).collect();
        let mut raw = json!({"fans":fans,"connected":true});
        let parsed = Snapshot::parse(&raw).unwrap();
        assert_eq!(parsed.lights()[0], [0.0, 0.0, 0.0, 1.0]);
        assert_eq!(parsed.lights()[1], [1.0, 0.0, 74.0 / 255.0, 1.0]);
        raw["fans"][1]["spot"] = "left1".into();
        assert!(Snapshot::parse(&raw).is_err());
    }
}
