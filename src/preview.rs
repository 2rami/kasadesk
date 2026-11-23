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
    fn open(start: Start, redraw: impl Fn() + Send + Sync + 'static) -> Option<Self> {
        let Start::Connect(connect) = start else { return None };
        let cfg = Config {
            codec: "h264".into(),
            max_w: WIDTH,
            max_h: HEIGHT,
            fps: 3,
            bitrate: 1_000_000,
            audio: false,
            refine: 0,
            ..Config::default()
        };
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

#[derive(Default)]
pub struct Preview {
    target: Option<(String, String)>,
    due: Option<Instant>,
    stream: Option<Stream>,
}

impl Preview {
    pub fn sync(&mut self, target: Option<&Machine>, net: &Arc<Net>, wake: impl Fn() + Send + Sync + 'static) -> bool {
        let target = target.and_then(|m| Some((m.label.as_str(), m.kasanet.get("id")?.as_str()?)));
        let changed = self.target.as_ref().map(|(label, id)| (label.as_str(), id.as_str())) != target;
        if changed {
            self.stream = None;
            self.target = target.map(|(label, id)| (label.into(), id.into()));
            self.due = target.map(|_| Instant::now() + Duration::from_millis(380));
        }
        let closed = self.stream.as_ref().is_some_and(|s| s.watch.is_closed());
        if closed {
            let stream = self.stream.take().unwrap();
            if !stream.denied.load(Ordering::Acquire) {
                self.due = Some(Instant::now() + Duration::from_secs(4));
            }
        }
        if let Some((label, _)) = target {
            if self.due.is_some_and(|at| Instant::now() >= at) {
                self.due = None;
                self.stream = Stream::open(net.start_view(label), wake);
                if self.stream.is_none() {
                    self.due = Some(Instant::now() + Duration::from_secs(4));
                }
            }
        }
        changed || closed
    }

    pub fn next_wake(&self) -> Option<Instant> {
        self.due
    }

    pub fn picture(&self, machine: &Machine) -> Option<Arc<Picture>> {
        let (label, id) = self.target.as_ref()?;
        if label != &machine.label || machine.kasanet.get("id")?.as_str()? != id {
            return None;
        }
        let stream = self.stream.as_ref()?;
        if stream.watch.is_closed() || stream.denied.load(Ordering::Acquire) {
            return None;
        }
        stream.latest.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preview_preserves_parameter_sets_and_rejects_truncated_nals() {
        let mut frame = Frame {
            width: 320,
            height: 180,
            param_sets: vec![vec![0x67, 1], vec![0x68]],
            data: vec![0, 0, 0, 2, 0x65, 3, 0, 0, 0, 1, 0x06],
            ..Default::default()
        };
        assert_eq!(annex_b(&frame).unwrap(), [0, 0, 0, 1, 0x67, 1, 0, 0, 0, 1, 0x68, 0, 0, 0, 1, 0x65, 3, 0, 0, 0, 1, 0x06]);
        frame.data.pop();
        assert!(annex_b(&frame).is_none());
    }
}
