use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

use kasa_cast::{Config, Event, Frame, Watch};
use kasa_castview::Start;
use openh264::{decoder::Decoder, formats::YUVSource};

use crate::net::Net;
use crate::roster::Machine;

pub const WIDTH: u32 = 512;
pub const HEIGHT: u32 = 320;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Quality {
    Far,
    Middle,
    Near,
}

impl Quality {
    fn for_pixels(width: f32, current: Option<Self>) -> Self {
        let near = if current == Some(Self::Near) { 240.0 } else { 300.0 };
        let middle = if current == Some(Self::Far) { 155.0 } else { 125.0 };
        if width >= near {
            Self::Near
        } else if width >= middle {
            Self::Middle
        } else {
            Self::Far
        }
    }

    fn capture(self) -> Config {
        let (max_w, max_h, fps) = match self {
            Self::Far => (192, 120, 1),
            Self::Middle => (320, 200, 2),
            Self::Near => (WIDTH, HEIGHT, 3),
        };
        Config {
            codec: "h264".into(),
            max_w,
            max_h,
            fps,
            bitrate: 1_000_000,
            audio: false,
            refine: 0,
            ..Config::default()
        }
    }
}

pub struct Picture {
    pub serial: u64,
    pub size: (u32, u32),
    pub rgba: Vec<u8>,
}

struct Stream {
    watch: Watch,
    denied: Arc<AtomicBool>,
    latest: Arc<Mutex<Option<Arc<Picture>>>>,
}

impl Stream {
    fn open(start: Start, quality: Quality, redraw: impl Fn() + Send + Sync + 'static) -> Option<Self> {
        let Start::Connect(connect) = start else { return None };
        let cfg = quality.capture();
        let (tx, rx) = mpsc::sync_channel(4);
        let redraw = Arc::new(redraw);
        let wake_closed = redraw.clone();
        let denied = Arc::new(AtomicBool::new(false));
        let refusal = denied.clone();
        let watch = connect(
            cfg,
            Box::new(move |event| match event {
                Event::Frame { frame, .. } => {
                    let _ = tx.try_send(frame);
                }
                Event::Closed(_) => wake_closed(),
                Event::Denied(_) => {
                    refusal.store(true, Ordering::Release);
                    wake_closed();
                }
                _ => {}
            }),
        )?;
        let keys = watch.keyframes();
        let latest = Arc::new(Mutex::new(None));
        let dest = latest.clone();
        std::thread::Builder::new()
            .name("desk-preview".into())
            .spawn(move || {
                let Ok(mut decoder) = Decoder::new() else { return };
                let mut next = None;
                let mut last_request = None;
                for frame in rx {
                    let valid = frame.key || next == Some(frame.seq);
                    if valid {
                        if let Some(packet) = annex_b(&frame) {
                            if let Ok(decoded) = decoder.decode(&packet) {
                                next = Some(frame.seq.wrapping_add(1));
                                if let Some(yuv) = decoded {
                                    let (w, h) = yuv.dimensions();
                                    if w == 0 || h == 0 || w > WIDTH as usize || h > HEIGHT as usize {
                                        continue;
                                    }
                                    let mut rgba = vec![0; w * h * 4];
                                    yuv.write_rgba8(&mut rgba);
                                    static SERIAL: AtomicU64 = AtomicU64::new(1);
                                    let pic = Picture {
                                        serial: SERIAL.fetch_add(1, Ordering::Relaxed),
                                        size: (w as u32, h as u32),
                                        rgba,
                                    };
                                    *dest.lock().unwrap_or_else(|e| e.into_inner()) = Some(Arc::new(pic));
                                    redraw();
                                }
                                continue;
                            }
                        }
                    }
                    next = None;
                    if last_request.is_none_or(|t: Instant| t.elapsed() >= Duration::from_secs(1)) {
                        keys.request();
                        last_request = Some(Instant::now());
                    }
                }
            })
            .ok()?;
        Some(Self { watch, latest, denied })
    }
}

fn annex_b(frame: &Frame) -> Option<Vec<u8>> {
    if frame.data.len() > 4 << 20 || frame.width > WIDTH || frame.height > HEIGHT {
        return None;
    }
    let mut packet = Vec::with_capacity(frame.data.len() + 128);
    for bytes in &frame.param_sets {
        packet.extend_from_slice(&[0, 0, 0, 1]);
        packet.extend_from_slice(bytes);
    }
    let mut data = frame.data.as_slice();
    while !data.is_empty() {
        let (len, rest) = data.split_at_checked(4)?;
        let len = u32::from_be_bytes(len.try_into().ok()?) as usize;
        if len == 0 {
            return None;
        }
        let (nal, remain) = rest.split_at_checked(len)?;
        packet.extend_from_slice(&[0, 0, 0, 1]);
        packet.extend_from_slice(nal);
        data = remain;
    }
    Some(packet)
}

struct Feed {
    peer: Option<String>,
    service: Option<String>,
    quality: Quality,
    requested: Quality,
    due: Option<Instant>,
    stream: Option<Stream>,
    previous: Option<Arc<Picture>>,
    denied: bool,
}

impl Feed {
    fn matches(&self, m: &Machine) -> bool {
        self.peer.as_deref() == m.kasanet["id"].as_str()
            && self.service.as_deref() == m.kasanet["screen_node"]["id"].as_str()
    }

    fn picture(&self) -> Option<Arc<Picture>> {
        let stream = self.stream.as_ref()?;
        if stream.watch.is_closed() || stream.denied.load(Ordering::Acquire) {
            return None;
        }
        stream.latest.lock().unwrap_or_else(|e| e.into_inner()).clone().or_else(|| self.previous.clone())
    }

    fn update(&mut self, pixels: f32, start: impl FnOnce() -> Start, wake: impl Fn() + Send + Sync + 'static) -> bool {
        if self.denied {
            return false;
        }
        let now = Instant::now();
        let quality = Quality::for_pixels(pixels, Some(self.quality));
        if self.previous.is_some()
            && self.stream.as_ref().is_some_and(|s| s.latest.lock().unwrap_or_else(|e| e.into_inner()).is_some())
        {
            self.previous = None;
        }
        let closed = self.stream.as_ref().is_some_and(|s| s.watch.is_closed() || s.denied.load(Ordering::Acquire));
        if closed {
            let stream = self.stream.take().unwrap();
            self.previous = None;
            self.denied = stream.denied.load(Ordering::Acquire);
            self.due = (!self.denied).then_some(now + Duration::from_secs(4));
            return true;
        }
        if quality != self.requested {
            self.requested = quality;
            if self.stream.is_some() {
                self.due = (quality != self.quality).then_some(now + Duration::from_millis(650));
            }
        }
        if self.due.is_some_and(|t| now >= t) {
            // Keep the last decoded frame on the same screen while its encoder changes size.
            self.previous = self.picture();
            self.stream = None;
            self.quality = self.requested;
            self.stream = Stream::open(start(), self.quality, wake);
            self.due = self.stream.is_none().then_some(now + Duration::from_secs(4));
            return true;
        }
        false
    }
}

#[derive(Default)]
pub struct Previews {
    feeds: HashMap<String, Feed>,
}

impl Previews {
    pub fn sync(
        &mut self,
        visible: &[(usize, f32)],
        machines: &[Machine],
        live: &[bool],
        net: &Arc<Net>,
        wake: impl Fn() + Clone + Send + Sync + 'static,
    ) -> bool {
        let before = self.feeds.len();
        self.feeds.retain(|label, f| {
            if let Some(s) = &f.stream {
                if s.denied.load(Ordering::Acquire) {
                    f.denied = true;
                    f.stream = None;
                    f.previous = None;
                    f.due = None;
                }
            }
            // A camera turn must not re-request a capture the remote device has declined.
            if f.denied {
                return machines
                    .iter()
                    .enumerate()
                    .any(|(i, m)| &m.label == label && f.matches(m) && live.get(i) == Some(&true));
            }
            visible.iter().any(|&(i, _)| {
                live.get(i) == Some(&true) && machines.get(i).is_some_and(|m| &m.label == label && f.matches(m))
            })
        });
        let mut changed = self.feeds.len() != before;
        for &(i, pixels) in visible {
            let Some(m) = machines.get(i).filter(|_| live.get(i) == Some(&true)) else { continue };
            let feed = self.feeds.entry(m.label.clone()).or_insert_with(|| {
                let quality = Quality::for_pixels(pixels, None);
                Feed {
                    peer: m.kasanet["id"].as_str().map(str::to_owned),
                    service: m.kasanet["screen_node"]["id"].as_str().map(str::to_owned),
                    quality,
                    requested: quality,
                    due: Some(Instant::now() + Duration::from_millis(220)),
                    stream: None,
                    previous: None,
                    denied: false,
                }
            });
            changed |= feed.update(pixels, || net.start_view(&m.label), wake.clone());
        }
        changed
    }

    pub fn next_wake(&self) -> Option<Instant> {
        self.feeds.values().filter_map(|f| f.due).min()
    }

    pub fn picture(&self, m: &Machine) -> Option<Arc<Picture>> {
        self.feeds.get(&m.label).filter(|f| f.matches(m))?.picture()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stream_quality_has_separate_enter_and_exit_thresholds() {
        assert_eq!(Quality::for_pixels(50.0, None), Quality::Far);
        assert_eq!(Quality::for_pixels(140.0, Some(Quality::Far)), Quality::Far);
        assert_eq!(Quality::for_pixels(140.0, Some(Quality::Middle)), Quality::Middle);
        assert_eq!(Quality::for_pixels(270.0, Some(Quality::Near)), Quality::Near);
        assert_eq!(Quality::for_pixels(310.0, Some(Quality::Middle)), Quality::Near);
        assert!(!Quality::Far.capture().audio && Quality::Far.capture().fps < Quality::Near.capture().fps);
    }

    #[test]
    fn preview_preserves_parameter_sets_and_rejects_truncated_nals() {
        let mut frame = Frame {
            width: 320,
            height: 180,
            param_sets: vec![vec![0x67, 1], vec![0x68]],
            data: vec![0, 0, 0, 2, 0x65, 3, 0, 0, 0, 1, 0x06],
            ..Default::default()
        };
        assert_eq!(
            annex_b(&frame).unwrap(),
            [0, 0, 0, 1, 0x67, 1, 0, 0, 0, 1, 0x68, 0, 0, 0, 1, 0x65, 3, 0, 0, 0, 1, 0x06]
        );
        frame.data.pop();
        assert!(annex_b(&frame).is_none());
    }
}
