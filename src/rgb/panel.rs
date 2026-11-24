use std::sync::Arc;
use std::time::{Duration, Instant};

use kasa_gridview::GridRenderer;
use winit::keyboard::KeyCode;

use super::{hex, parse_color, Color, Target, Fan, Session, Snapshot, Update, NAMES};
use crate::text::Ui;

type Rect = (f32, f32, f32, f32);
use crate::theme::{CLOUD as TEXT, EDGE, INK, MIST as MUTED, RAIN, SUN as AMBER, SURFACE};
const LCD: &str = "kasadesk:kraken-lcd";
const PALETTE: [Color; 8] = [
    [255, 244, 226],
    [255, 187, 112],
    [244, 119, 105],
    [222, 146, 223],
    [161, 149, 245],
    [103, 174, 254],
    [112, 227, 223],
    [147, 228, 168],
];
fn rgba(c: Color) -> [u8; 4] {
    [c[0], c[1], c[2], 255]
}
fn inside((x, y, w, h): Rect, (px, py): (f32, f32)) -> bool {
    px >= x && px < x + w && py >= y && py < y + h
}
fn fill(g: &mut GridRenderer, (x, y, w, h): Rect, r: f32, c: [u8; 4]) {
    g.round_rect_fill(x, y, w, h, r, c)
}
fn outline(g: &mut GridRenderer, (x, y, w, h): Rect, r: f32, c: [u8; 4]) {
    g.round_rect_stroke(x, y, w, h, r, 1.0, c)
}
fn center(g: &mut GridRenderer, ui: &mut Ui, r: Rect, t: &str, sz: f32, col: [u8; 4], bold: bool) {
    let w = ui.measure(g, t, sz, bold);
    ui.draw(g, r.0 + (r.2 - w) * 0.5, r.1 + (r.3 - sz) * 0.5, t, sz, col, bold);
}

#[derive(Clone, Copy, PartialEq)]
enum Action {
    Open,
    Close,
    Light,
    Cooling,
    Fan(usize),
    Swatch(usize),
    Hex,
    Apply,
    Release,
}
#[derive(Default)]
pub struct Panel {
    pub open: bool,
    pub data: Option<Arc<Snapshot>>,
    pub generation: u64,
    session: Option<Session>,
    retry: Option<Instant>,
    target: Option<Target>,
    error: Option<String>,
    cooling: bool,
    fan: usize,
    pending: bool,
    submitted: Option<(usize, String)>,
    draft: String,
    edited: bool,
    typing: bool,
    replace_hex: bool,
    hits: Vec<(Rect, Action)>,
    body: Option<Rect>,
    focus: Option<Action>,
    upload: bool,
}
impl Panel {
    fn set_target(&mut self, target: Option<Target>) -> bool {
        if self.target == target {
            return false;
        }
        self.session = None;
        self.target = target;
        self.generation = self.generation.wrapping_add(1);
        self.data = None;
        self.retry = None;
        self.error = None;
        self.pending = false;
        self.submitted = None;
        self.edited = false;
        self.typing = false;
        self.draft.clear();
        self.hits.clear();
        true
    }

    pub fn discard_stale(&mut self, route: Option<&Target>) {
        if self.target.is_some() && self.target.as_ref() != route {
            self.set_target(None);
        }
    }

    pub fn sync(
        &mut self,
        active: bool,
        rt: &tokio::runtime::Handle,
        target: Option<Target>,
        notify: impl Fn(u64, Update) + Send + Sync + 'static,
    ) -> bool {
        let changed = self.set_target(target.filter(|_| active));
        if self.target.is_none() { return changed; }
        if self.session.is_none() && self.retry.is_none_or(|t| t <= Instant::now()) {
            let Some(target) = self.target.clone() else { return false };
            static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
            self.generation = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let generation = self.generation;
            self.session = Some(Session::start(rt, target, move |u| notify(generation, u)));
            self.retry = None;
            return true;
        }
        changed
    }
    pub fn fan_index(&self) -> usize {
        self.fan
    }
    pub fn paste_hex(&mut self, text: &str) {
        if !self.typing {
            return;
        }
        let value = text.trim().trim_start_matches('#');
        if parse_color(value).is_some() {
            self.draft = value.to_ascii_uppercase();
            self.replace_hex = false;
            self.edited = true;
        }
    }
    pub fn editing_hex(&self) -> bool {
        self.typing
    }
    pub fn retry_at(&self) -> Option<Instant> {
        self.retry
    }
    pub fn receive(&mut self, generation: u64, update: Update) {
        if generation != self.generation || self.target.is_none() {
            return;
        }
        match update.result {
            Ok(data) => {
                self.error = None;
                if update.applied {
                    if self.submitted.take().is_some_and(|(fan, draft)| fan == self.fan && draft == self.draft) {
                        self.edited = false;
                    }
                    self.pending = false;
                }
                self.data = Some(Arc::new(data));
                self.upload = true;
                if !self.edited && !self.typing {
                    self.load_color();
                }
            }
            Err(err) => {
                self.data = None;
                self.pending = false;
                self.session = None;
                self.error = Some(err);
                self.retry = Some(Instant::now() + Duration::from_secs(5));
            }
        }
    }
    fn current(&self) -> Fan {
        self.data.as_ref().map_or_else(Fan::default, |s| s.fans[self.fan])
    }
    fn load_color(&mut self) {
        let f = self.current();
        self.draft = hex(f.fixed.or(f.inner).unwrap_or(PALETTE[6]));
    }
    pub fn choose(&mut self, fan: usize) {
        self.open = true;
        self.cooling = false;
        self.fan = fan;
        self.edited = false;
        self.typing = false;
        self.focus = None;
        self.load_color();
    }
    pub fn close(&mut self) {
        self.open = false;
        self.focus = None;
        self.typing = false;
        self.hits.clear();
        self.body = None;
    }
    pub fn contains(&self, p: (f32, f32)) -> bool {
        self.body.is_some_and(|r| inside(r, p)) || self.hits.iter().any(|(r, _)| inside(*r, p))
    }
    pub fn clickable(&self, p: (f32, f32)) -> bool {
        self.hits.iter().any(|(r, _)| inside(*r, p))
    }
    fn ready(&self) -> bool {
        self.data.as_ref().is_some_and(|s| s.connected && s.fans[self.fan].mapped) && !self.pending
    }
    fn activate(&mut self, act: Action) {
        if act != Action::Hex {
            self.typing = false;
        }
        match act {
            Action::Open => self.choose(self.fan),
            Action::Close => self.close(),
            Action::Light => self.cooling = false,
            Action::Cooling => self.cooling = true,
            Action::Fan(i) => self.choose(i),
            Action::Hex => {
                self.typing = true;
                self.replace_hex = true;
            }
            Action::Swatch(i) => {
                self.draft = hex(PALETTE[i]);
                self.edited = true;
            }
            Action::Apply | Action::Release if self.ready() => {
                let c = if act == Action::Apply {
                    let Some(v) = parse_color(&self.draft) else { return };
                    Some(v)
                } else {
                    None
                };
                self.pending = self.session.as_ref().is_some_and(|s| s.color(self.fan, c));
                if self.pending {
                    self.submitted = Some((self.fan, self.draft.clone()));
                }
            }
            _ => {}
        }
    }
    pub fn click(&mut self, p: (f32, f32)) -> bool {
        if let Some(a) = self.hits.iter().find(|(r, _)| inside(*r, p)).map(|(_, a)| *a) {
            self.focus = None;
            self.activate(a);
            true
        } else {
            self.body.is_some_and(|r| inside(r, p))
        }
    }
    pub fn key(&mut self, code: KeyCode, text: Option<&str>, shift: bool) {
        if code == KeyCode::Escape {
            if self.typing {
                self.typing = false;
                self.edited = false;
                self.load_color()
            } else {
                self.close()
            }
            return;
        }
        if code == KeyCode::Tab && !self.hits.is_empty() {
            self.typing = false;
            let n = self.hits.len();
            let i = self.focus.and_then(|a| self.hits.iter().position(|(_, v)| *v == a));
            let next = if shift { i.map_or(n - 1, |v| (v + n - 1) % n) } else { i.map_or(0, |v| (v + 1) % n) };
            self.focus = Some(self.hits[next].1);
            return;
        }
        if self.typing {
            if code == KeyCode::Enter {
                self.activate(Action::Apply);
                return;
            }
            if code == KeyCode::Backspace {
                if self.replace_hex {
                    self.draft.clear()
                } else {
                    self.draft.pop();
                }
                self.replace_hex = false;
                self.edited = true;
                return;
            }
            if let Some(t) = text.filter(|v| !v.is_empty() && v.bytes().all(|c| c.is_ascii_hexdigit())) {
                if self.replace_hex {
                    self.draft.clear();
                    self.replace_hex = false
                }
                if self.draft.len() + t.len() <= 6 {
                    self.draft.push_str(&t.to_ascii_uppercase());
                    self.edited = true;
                }
            }
        } else if matches!(code, KeyCode::Space | KeyCode::Enter) {
            if let Some(a) = self.focus {
                self.activate(a);
            }
        } else if !self.cooling && matches!(code, KeyCode::ArrowLeft | KeyCode::ArrowRight) {
            self.choose((self.fan + if code == KeyCode::ArrowRight { 1 } else { 5 }) % 6);
        }
    }
    fn hit(&mut self, g: &mut GridRenderer, r: Rect, a: Action) {
        self.hits.push((r, a));
        if self.focus == Some(a) {
            outline(g, (r.0 - 2.0, r.1 - 2.0, r.2 + 4.0, r.3 + 4.0), 7.0, RAIN)
        }
    }
    pub fn paint(&mut self, g: &mut GridRenderer, ui: &mut Ui, size: (f32, f32), available: bool, connection_hint: Option<&str>) {
        self.hits.clear();
        self.body = None;
        if !available {
            return;
        }
        if !self.open {
            let r = (24.0, 78.0, 112.0, 32.0);
            fill(g, r, 8.0, INK);
            outline(g, r, 8.0, EDGE);
            fill(
                g,
                (r.0 + 10.0, r.1 + 12.0, 8.0, 8.0),
                4.0,
                if self.data.as_ref().is_some_and(|s| s.connected) { RAIN } else { MUTED },
            );
            ui.draw(g, r.0 + 25.0, r.1 + 9.0, "조명 · LCD", 13.0, TEXT, true);
            self.hit(g, r, Action::Open);
            return;
        }
        let compact = size.1 < 500.0;
        let h = if compact { 276.0 } else { 427.0 };
        let w = 360.0_f32.min(size.0 - 20.0);
        let x = if size.0 < 550.0 { 10.0 } else { 24.0 };
        let y = ((size.1 - h) * 0.5).clamp(8.0, 140.0);
        let r = (x, y, w, h);
        self.body = Some(r);
        fill(g, (x + 3.0, y + 7.0, w, h), 14.0, [4, 9, 13, 104]);
        fill(g, r, 13.0, INK);
        outline(g, r, 13.0, EDGE);
        let pad = if compact { 13.0 } else { 20.0 };
        let l = x + pad;
        let iw = w - 2.0 * pad;
        g.round_rect_fill(l, y + 1.0, iw, 2.0, 1.0, EDGE);
        g.rect(l, y + 1.0, iw * 0.22, 2.0, AMBER);
        ui.draw(g, l, y + if compact { 9.0 } else { 18.0 }, "Y60", if compact { 22.0 } else { 29.0 }, TEXT, true);
        ui.draw(
            g,
            l + if compact { 51.0 } else { 66.0 },
            y + if compact { 15.0 } else { 29.0 },
            "RGB Control",
            13.0,
            MUTED,
            false,
        );
        let close = (x + w - pad - 25.0, y + if compact { 6.0 } else { 16.0 }, 26.0, 26.0);
        center(g, ui, close, "×", 19.0, MUTED, false);
        self.hit(g, close, Action::Close);
        if let Some(hint) = connection_hint {
            ui.draw(g,l,y+88.0,"RGB 연결",17.0,TEXT,true);
            ui.draw(g,l,y+126.0,hint,12.0,MUTED,false);
            return;
        }
        let status = match (&self.data, &self.error) {
            (Some(s), _) if s.connected => "연결됨",
            (Some(_), _) => "허브 대기",
            (None, Some(_)) => "연결 끊김",
            _ => "연결 중",
        };
        let stat_col = if self.data.as_ref().is_some_and(|s| s.connected) { RAIN } else { AMBER };
        fill(g, (close.0 - 65.0, close.1 + 10.0, 6.0, 6.0), 3.0, stat_col);
        ui.draw(g, close.0 - 53.0, close.1 + 6.0, status, 10.5, stat_col, true);
        let tab_y = y + if compact { 36.0 } else { 65.0 };
        let tab_h = if compact { 25.0 } else { 33.0 };
        fill(g, (l, tab_y, iw, tab_h), 6.0, [13, 30, 52, 255]);
        for (i, (label, a)) in [("팬 조명", Action::Light), ("냉각 · LCD", Action::Cooling)].into_iter().enumerate() {
            let tab = (l + 2.0 + i as f32 * iw / 2.0, tab_y + 2.0, iw / 2.0 - 4.0, tab_h - 4.0);
            if self.cooling == (i == 1) {
                fill(g, tab, 5.0, SURFACE);
                g.rect(tab.0 + 10.0, tab.1 + tab.3 - 2.0, tab.2 - 20.0, 2.0, RAIN);
            }
            center(g, ui, tab, label, 12.5, if self.cooling == (i == 1) { TEXT } else { MUTED }, true);
            self.hit(g, tab, a);
        }
        let top = tab_y + tab_h + if compact { 8.0 } else { 20.0 };
        if self.cooling {
            self.thermal(g, ui, (l, top, iw, h - (top - y) - pad), compact);
        } else {
            self.lights(g, ui, (l, top, iw, h - (top - y) - pad), compact);
        }
    }
    fn lights(&mut self, g: &mut GridRenderer, ui: &mut Ui, (x, y, w, h): Rect, compact: bool) {
        let gap = 5.0;
        let tw = (w - 2.0 * gap) / 3.0;
        let th = if compact { 29.0 } else { 48.0 };
        for i in 0..6 {
            let r = (x + (i % 3) as f32 * (tw + gap), y + (i / 3) as f32 * (th + gap), tw, th);
            let selected = i == self.fan;
            fill(g, r, 6.0, if selected { [43, 66, 66, 255] } else { SURFACE });
            if selected {
                outline(g, r, 6.0, RAIN)
            }
            let f = self.data.as_ref().map_or_else(Fan::default, |s| s.fans[i]);
            let d = if compact { 16.0 } else { 23.0 };
            let (cx, cy) = (r.0 + 8.0, r.1 + (th - d) * 0.5);
            fill(g, (cx, cy, d, d), d / 2.0, rgba(f.outer.unwrap_or([89, 104, 112])));
            fill(g, (cx + 2.0, cy + 2.0, d - 4.0, d - 4.0), d / 2.0, INK);
            fill(g, (cx + d * 0.28, cy + d * 0.28, d * 0.44, d * 0.44), d / 2.0, rgba(f.inner.unwrap_or([117, 131, 138])));
            ui.draw(g, cx + d + 5.0, r.1 + (th - 12.0) * 0.5, NAMES[i], if compact { 11.5 } else { 12.0 }, TEXT, selected);
            self.hit(g, r, Action::Fan(i));
        }
        let line = y + 2.0 * th + gap + if compact { 8.0 } else { 17.0 };
        g.rect(x, line, w, 1.0, EDGE);
        let f = self.current();
        let label = if self.pending {
            "적용 중"
        } else if f.fixed.is_some() {
            "고정 색"
        } else {
            "현재 색"
        };
        let meta = if let Some(rpm) = f.rpm { format!("{label}  ·  포트 {rpm} RPM") } else { label.into() };
        let ty = line + if compact { 6.0 } else { 13.0 };
        ui.draw(g, x, ty, NAMES[self.fan], if compact { 14.0 } else { 17.0 }, TEXT, true);
        let mw = ui.measure(g, &meta, 11.0, false);
        ui.draw(g, x + w - mw, ty + 2.0, &meta, 11.0, MUTED, false);
        let py = ty + if compact { 22.0 } else { 34.0 };
        let sw = (w - 7.0 * 5.0) / 8.0;
        for (i, c) in PALETTE.into_iter().enumerate() {
            let r = (x + i as f32 * (sw + 5.0), py, sw, if compact { 19.0 } else { 28.0 });
            fill(g, r, 5.0, rgba(c));
            if parse_color(&self.draft) == Some(c) {
                outline(g, (r.0 - 1.5, r.1 - 1.5, r.2 + 3.0, r.3 + 3.0), 6.0, TEXT);
            }
            self.hit(g, r, Action::Swatch(i));
        }
        let cy = py + if compact { 26.0 } else { 39.0 };
        let ch = if compact { 28.0 } else { 36.0 };
        let field = (x, cy, w * 0.50 - 4.0, ch);
        fill(g, field, 6.0, [11, 18, 22, 255]);
        outline(g, field, 6.0, if self.typing { RAIN } else { EDGE });
        let sample = parse_color(&self.draft).unwrap_or([70, 80, 86]);
        fill(g, (x + 9.0, cy + (ch - 14.0) * 0.5, 14.0, 14.0), 4.0, rgba(sample));
        ui.draw(g, x + 30.0, cy + (ch - 13.0) * 0.5, &format!("#{}", self.draft), 13.0, TEXT, true);
        self.hit(g, field, Action::Hex);
        let apply = (x + w * 0.50 + 4.0, cy, w * 0.50 - 4.0, ch);
        let ready = self.ready() && parse_color(&self.draft).is_some();
        fill(g, apply, 6.0, if ready { RAIN } else { SURFACE });
        center(g, ui, apply, if self.pending { "적용 중" } else { "색 적용" }, 13.0, if ready { INK } else { MUTED }, true);
        if ready {
            self.hit(g, apply, Action::Apply);
        }
        let reset = (x, cy + ch + 3.0, w, (y + h - cy - ch - 3.0).clamp(19.0, 32.0));
        if let Some(err) = &self.error {
            center(g, ui, reset, err, 11.0, AMBER, false);
        } else {
            center(g, ui, reset, "효과 따르기", 12.0, MUTED, false);
            if self.ready() {
                self.hit(g, reset, Action::Release);
            }
        }
    }
    fn thermal(&mut self, g: &mut GridRenderer, ui: &mut Ui, (x, y, w, h): Rect, compact: bool) {
        let d = if compact { 82.0 } else { 128.0 };
        let lcd = (x + 7.0, y + 4.0, d, d);
        fill(g, (lcd.0 - 4.0, lcd.1 - 4.0, d + 8.0, d + 8.0), d, EDGE);
        fill(g, lcd, d, [8, 15, 19, 255]);
        let snapshot = self.data.as_ref();
        if let Some(img) = snapshot.and_then(|s| s.lcd.as_ref()) {
            if self.upload || !g.has_image(LCD) {
                g.update_image(LCD, img.as_raw(), img.width(), img.height());
                self.upload = false;
            }
            g.queue_image_above(LCD, lcd.0, lcd.1, d, d);
        } else {
            center(g, ui, lcd, "LCD 대기", 12.0, MUTED, false);
        }
        let tx = x + d + if compact { 23.0 } else { 30.0 };
        ui.draw(g, tx, y + 6.0, "KRAKEN ELITE 360", 11.0, RAIN, true);
        ui.draw(g, tx, y + if compact { 24.0 } else { 40.0 }, "냉각수", 12.0, MUTED, false);
        let temp = snapshot.and_then(|s| s.liquid).map_or_else(|| "—".into(), |t| format!("{t:.1}°C"));
        ui.draw(g, tx, y + if compact { 41.0 } else { 63.0 }, &temp, if compact { 23.0 } else { 32.0 }, TEXT, true);
        let ry = y + d + if compact { 15.0 } else { 25.0 };
        let rh = if compact { 24.0 } else { 37.0 };
        for (i, (name, value)) in [
            ("펌프", snapshot.and_then(|s| s.pump).map(|n| format!("{n} RPM"))),
            ("GPU", snapshot.and_then(|s| s.gpu).map(|n| format!("{n:.0}°C"))),
            (NAMES[self.fan], self.current().rpm.map(|n| format!("{n} RPM"))),
        ]
        .into_iter()
        .enumerate()
        {
            let line = ry + i as f32 * rh;
            g.rect(x, line, w, 1.0, EDGE);
            let yy = line + (rh - 13.0) * 0.5;
            ui.draw(g, x, yy, name, 13.0, MUTED, false);
            let v = value.as_deref().unwrap_or("—");
            let vw = ui.measure(g, v, 13.0, true);
            ui.draw(g, x + w - vw, yy, v, 13.0, TEXT, true);
        }
        if let Some(e) = &self.error {
            ui.draw(g, x, y + h - 13.0, e, 11.0, AMBER, false);
        }
    }
}
