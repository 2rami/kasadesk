use std::sync::Arc;
use std::time::{Duration, Instant};

use kasa_gridview::GridRenderer;
use winit::keyboard::KeyCode;

use super::{hex, parse_color, Color, Target, Fan, LightPart, Session, Snapshot, Update, NAMES};
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
fn hue(t: f32) -> Color {
    let h = t.rem_euclid(1.0) * 6.0;
    let up = (h.fract() * 255.0).round() as u8;
    let down = 255 - up;
    match h as u8 {
        0 => [255, up, 0],
        1 => [down, 255, 0],
        2 => [0, 255, up],
        3 => [0, down, 255],
        4 => [up, 0, 255],
        _ => [255, 0, down],
    }
}

fn hue_position(c: Color) -> f32 {
    let [r, g, b] = c.map(f32::from);
    let peak = r.max(g).max(b);
    let span = peak - r.min(g).min(b);
    if span == 0.0 {
        return 0.0;
    }
    let sector = if peak == r {
        (g - b) / span
    } else if peak == g {
        2.0 + (b - r) / span
    } else {
        4.0 + (r - g) / span
    };
    sector.rem_euclid(6.0) / 6.0
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

#[derive(Clone, Copy, Default, PartialEq)]
enum Tab {
    #[default]
    Fans,
    Graphics,
    Cooling,
}

#[derive(Clone, Copy, PartialEq)]
enum Action {
    Open,
    Close,
    Tab(Tab),
    Revert,
    Fan(usize),
    Swatch(usize),
    Hue,
    Brightness,
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
    tab: Tab,
    fan: usize,
    pending: bool,
    submitted: Option<(LightPart, String)>,
    draft: String,
    edited: bool,
    typing: bool,
    replace_hex: bool,
    hits: Vec<(Rect, Action)>,
    body: Option<Rect>,
    focus: Option<Action>,
    upload: bool,
    drag: Option<(Rect, Action)>,
    bright_color: Option<Color>,
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
        self.drag = None;
        self.bright_color = None;
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
        let changed = self.set_target(target);
        // Keep the draft while another window has focus, so a HEX value can be copied from it.
        if !active {
            let suspended = self.session.take().is_some() || self.data.is_some();
            self.generation = 0;
            self.data = None;
            self.retry = None;
            self.pending = false;
            self.submitted = None;
            self.drag = None;
            return changed || suspended;
        }
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
    pub fn part(&self) -> LightPart {
        if self.tab == Tab::Graphics {
            LightPart::Gpu
        } else {
            LightPart::Fan(self.fan)
        }
    }

    pub fn gpu_color(&self) -> Option<Color> {
        if self.open && self.tab == Tab::Graphics && self.edited {
            if let Some(c) = parse_color(&self.draft) {
                return Some(c);
            }
        }
        let data = self.data.as_ref()?;
        let light = data.gpu_light.as_ref()?;
        if !data.connected || !light.controller || !light.sync {
            return None;
        }
        if light.asleep {
            Some([0; 3])
        } else {
            light.fixed.or(light.sample)
        }
    }

    pub fn paste_hex(&mut self, text: &str) {
        if !self.typing {
            return;
        }
        let value = text.trim().trim_start_matches('#');
        if parse_color(value).is_some() {
            self.draft = value.to_ascii_uppercase();
            self.replace_hex = false;
            self.bright_color = None;
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
                    if self.submitted.take().is_some_and(|(fan, draft)| fan == self.part() && draft == self.draft) {
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
        let color = match self.part() {
            LightPart::Gpu => self.data.as_ref().and_then(|s| s.gpu_light.as_ref().and_then(|g| g.fixed.or(g.sample))),
            LightPart::Fan(_) => self.current().fixed.or(self.current().inner),
        };
        self.draft = hex(color.unwrap_or(PALETTE[6]));
        self.bright_color = None;
    }
    pub fn choose(&mut self, part: LightPart) {
        self.open = true;
        self.drag = None;
        self.tab = match part {
            LightPart::Fan(i) => { self.fan = i; Tab::Fans }
            LightPart::Gpu => Tab::Graphics,
        };
        self.edited = false;
        self.typing = false;
        self.focus = None;
        self.load_color();
    }
    pub fn close(&mut self) {
        self.open = false;
        self.drag = None;
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
        self.data.as_ref().is_some_and(|s| s.connected && match self.part() {
            LightPart::Fan(i) => s.fans[i].mapped,
            LightPart::Gpu => s.gpu_light.as_ref().is_some_and(|g| g.controller),
        }) && !self.pending
    }
    fn activate(&mut self, act: Action) {
        if act != Action::Hex {
            self.typing = false;
        }
        match act {
            Action::Open => self.choose(self.part()),
            Action::Close => self.close(),
            Action::Tab(tab) => {
                if self.tab != tab {
                    self.tab = tab;
                    self.edited = false;
                    self.load_color();
                }
            }
            Action::Revert => { self.edited = false; self.load_color(); }
            Action::Fan(i) => self.choose(LightPart::Fan(i)),
            Action::Hex => {
                self.typing = true;
                self.replace_hex = true;
            }
            Action::Swatch(i) => {
                self.bright_color = None;
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
                self.pending = self.session.as_ref().is_some_and(|s| s.color(self.part(), c));
                if self.pending {
                    self.submitted = Some((self.part(), self.draft.clone()));
                }
            }
            _ => {}
        }
    }
    fn adjust(&mut self, action: Action, value: f32) {
        let t = value.clamp(0.0, 1.0);
        let color = match action {
            Action::Hue => hue(t),
            Action::Brightness => {
                let c = self
                    .bright_color
                    .unwrap_or_else(|| parse_color(&self.draft).unwrap_or([255; 3]));
                let max = *c.iter().max().unwrap();
                let base = if max == 0 { [255; 3] } else { c };
                self.bright_color = Some(base);
                let peak = f32::from(*base.iter().max().unwrap());
                base.map(|v| (f32::from(v) * 255.0 * t / peak).round() as u8)
            }
            _ => return,
        };
        if action == Action::Hue {
            self.bright_color = None;
        }
        self.draft = hex(color);
        self.edited = true;
        self.typing = false;
    }

    pub fn slide(&mut self, p: (f32, f32)) -> bool {
        let Some((rect, action)) = self.drag else { return false };
        self.adjust(action, (p.0 - rect.0) / rect.2);
        true
    }

    pub fn end_slide(&mut self) {
        self.drag = None;
    }

    pub fn click(&mut self, p: (f32, f32)) -> bool {
        self.drag = None;
        if let Some(&(r, a)) = self.hits.iter().find(|(r, _)| inside(*r, p)) {
            if matches!(a, Action::Hue | Action::Brightness) {
                self.focus = Some(a);
                self.drag = Some((r, a));
                if a == Action::Brightness && parse_color(&self.draft).is_some_and(|v| v != [0; 3]) {
                    self.bright_color = parse_color(&self.draft);
                }
                self.slide(p);
            } else {
                self.focus = None;
                self.activate(a);
            }
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
                    self.bright_color = None;
                    self.draft.push_str(&t.to_ascii_uppercase());
                    self.edited = true;
                }
            }
        } else if matches!(code, KeyCode::Space | KeyCode::Enter) {
            if let Some(a) = self.focus {
                self.activate(a);
            }
        } else if let Some(action @ (Action::Hue | Action::Brightness)) = self.focus {
            if matches!(code, KeyCode::ArrowLeft | KeyCode::ArrowRight) {
                let c = parse_color(&self.draft).unwrap_or(PALETTE[6]);
                let delta = if code == KeyCode::ArrowRight { 0.025 } else { -0.025 };
                let pos = if action == Action::Hue { (hue_position(c) + delta).rem_euclid(1.0) }
                    else { f32::from(*c.iter().max().unwrap())/255.0 + delta };
                self.adjust(action, pos);
            }
        } else if self.tab == Tab::Fans && matches!(code, KeyCode::ArrowLeft | KeyCode::ArrowRight) {
            self.choose(LightPart::Fan((self.fan + if code == KeyCode::ArrowRight { 1 } else { 5 }) % 6));
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
            let r = (size.0 - 194.0, size.1 - 50.0, 100.0, 34.0);
            fill(g, r, 12.0, INK);
            fill(
                g,
                (r.0 + 10.0, r.1 + 12.0, 8.0, 8.0),
                4.0,
                if self.data.as_ref().is_some_and(|s| s.connected) { RAIN } else { MUTED },
            );
            ui.draw(g, r.0 + 25.0, r.1 + 10.0, "조명 · LCD", 13.0, TEXT, true);
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
        ui.draw(g, l, y + if compact { 10.0 } else { 20.0 }, "본체 조명", if compact { 18.0 } else { 23.0 }, TEXT, true);
        let close = (x + w - pad - 25.0, y + if compact { 6.0 } else { 16.0 }, 26.0, 26.0);
        center(g, ui, close, "×", 19.0, MUTED, false);
        self.hit(g, close, Action::Close);
        let status = match (&self.data, &self.error) {
            (Some(s), _) if s.connected => "연결됨",
            (Some(_), _) => "허브 대기",
            (None, Some(_)) => "연결 끊김",
            _ if connection_hint.is_some() => "미연결",
            _ => "연결 중",
        };
        let stat_col = if self.data.as_ref().is_some_and(|s| s.connected) { RAIN } else { AMBER };
        fill(g, (close.0 - 65.0, close.1 + 10.0, 6.0, 6.0), 3.0, stat_col);
        ui.draw(g, close.0 - 53.0, close.1 + 6.0, status, 10.5, stat_col, true);
        let tab_y = y + if compact { 36.0 } else { 65.0 };
        let tab_h = if compact { 25.0 } else { 33.0 };
        fill(g, (l, tab_y, iw, tab_h), 6.0, [25, 31, 32, 255]);
        for (i, (label, page)) in [("팬 조명", Tab::Fans), ("그래픽카드", Tab::Graphics), ("냉각 · LCD", Tab::Cooling)].into_iter().enumerate() {
            let tab = (l + 2.0 + i as f32 * iw / 3.0, tab_y + 2.0, iw / 3.0 - 4.0, tab_h - 4.0);
            if self.tab == page {
                fill(g, tab, 5.0, SURFACE);
            }
            center(g, ui, tab, label, 12.5, if self.tab == page { TEXT } else { MUTED }, true);
            self.hit(g, tab, Action::Tab(page));
        }
        let top = tab_y + tab_h + if compact { 8.0 } else { 20.0 };
        let area = (l, top, iw, h - (top - y) - pad);
        match self.tab {
            Tab::Graphics => self.graphics(g, ui, area, compact, connection_hint),
            _ if connection_hint.is_some() => {
                ui.draw(g, l, top + 12.0, connection_hint.unwrap_or_default(), 12.0, MUTED, false);
            }
            Tab::Cooling => self.thermal(g, ui, area, compact),
            Tab::Fans => self.lights(g, ui, area, compact),
        }
    }

    fn graphics(&mut self, g: &mut GridRenderer, ui: &mut Ui, (x, y, w, h): Rect, compact: bool, hint: Option<&str>) {
        let data = self.data.as_deref();
        let light = data.and_then(|s| s.gpu_light.as_ref());
        let fixed = light.and_then(|s| s.fixed);
        let stats = [
            data.and_then(|s| s.gpu).map(|n| format!("{n:.0}°C")),
            light.and_then(|s| s.utilization).map(|n| format!("{n}%")),
        ];
        let stats = stats.into_iter().flatten().collect::<Vec<_>>().join("  ·  ");
        ui.draw(g, x, y, "NEPTUNE", if compact { 17.0 } else { 23.0 }, TEXT, true);
        let sw = ui.measure(g, &stats, 12.0, false);
        ui.draw(g, x + w - sw, y + 3.0, &stats, 12.0, RAIN, false);
        let info_y = y + if compact { 23.0 } else { 34.0 };
        ui.draw(g, x, info_y, "iGame GeForce RTX 4070 Ti", 12.0, MUTED, false);
        let mode = match (light, fixed) {
            (_, Some(c)) => format!("고정 색   #{}", hex(c)),
            (Some(v), None) => {
                let effect = if v.effect.is_empty() {
                    "효과 따르기"
                } else {
                    &v.effect
                };
                v.sample.map_or_else(
                    || effect.to_string(),
                    |c| format!("{effect}   ·   효과 대표색 #{}", hex(c)),
                )
            }
            _ => "설정 확인 대기".to_owned(),
        };
        let card_y = info_y + if compact { 18.0 } else { 27.0 };
        fill(g, (x, card_y, w, if compact { 27.0 } else { 40.0 }), 6.0, SURFACE);
        let yy = card_y + if compact { 7.0 } else { 13.0 };
        if let Some(c) = fixed.or_else(|| light.and_then(|v| v.sample)) {
            fill(g, (x + 9.0, yy, 14.0, 14.0), 4.0, rgba(c));
        } else {
            outline(g, (x + 9.0, yy, 14.0, 14.0), 4.0, EDGE);
        }
        ui.draw(g, x + 32.0, yy, &mode, 12.0, TEXT, true);
        let status = match light {
            _ if hint.is_some() => hint.unwrap_or_default(),
            None if data.is_some() => "이 연결은 GPU 조명을 지원하지 않아요",
            None => "GPU 조명 상태를 기다리고 있어요",
            Some(_) if !data.is_some_and(|s| s.connected) => "RGB Control 연결 끊김",
            Some(v) if !v.controller => "Razer 조명 컨트롤러 미연결",
            Some(v) if !v.sync => "RGB Control의 장치 동기화 꺼짐",
            Some(v) if v.asleep => "모니터 꺼짐 · 조명 소등 중",
            Some(_) => "Razer ARGB  ·  채널 5",
        };
        let status_y = card_y + if compact { 30.0 } else { 50.0 };
        ui.draw(g, x, status_y, status, 11.5, MUTED, false);
        let bar_y = status_y + if compact { 16.0 } else { 26.0 };
        let bar_h = if compact { 16.0 } else { 21.0 };
        let draft = parse_color(&self.draft).unwrap_or(PALETTE[6]);
        for (i, (label, action)) in [("색상", Action::Hue), ("밝기", Action::Brightness)]
            .into_iter()
            .enumerate()
        {
            let left = x + i as f32 * (w / 2.0 + 3.0);
            ui.draw(g, left, bar_y + 2.0, label, 11.0, MUTED, false);
            let bar = (left + 29.0, bar_y, w / 2.0 - 38.0, bar_h);
            let base = if draft == [0; 3] {
                self.bright_color.unwrap_or([255; 3])
            } else {
                draft
            };
            let peak = f32::from(*base.iter().max().unwrap()).max(1.0);
            for j in 0..32 {
                let t = j as f32 / 31.0;
                let c = if i == 0 {
                    hue(t)
                } else {
                    base.map(|v| (f32::from(v) * t * 255.0 / peak).round() as u8)
                };
                g.rect(
                    bar.0 + j as f32 * bar.2 / 32.0,
                    bar.1,
                    bar.2 / 32.0 + 0.3,
                    bar.3,
                    rgba(c),
                );
            }
            let t = if i == 0 {
                hue_position(draft)
            } else {
                f32::from(*draft.iter().max().unwrap()) / 255.0
            };
            let px = bar.0 + t * (bar.2 - 3.0);
            g.rect(px, bar.1 - 1.0, 3.0, bar.3 + 2.0, INK);
            g.rect(px + 1.0, bar.1 - 1.0, 1.0, bar.3 + 2.0, TEXT);
            self.hit(g, bar, action);
        }
        let py = bar_y + bar_h + if compact { 6.0 } else { 10.0 };
        self.editor(g, ui, (x, py, w, y + h - py), compact);
    }

    fn lights(&mut self, g: &mut GridRenderer, ui: &mut Ui, (x, y, w, h): Rect, compact: bool) {
        let gap = 5.0;
        let tw = (w - 2.0 * gap) / 3.0;
        let th = if compact { 29.0 } else { 48.0 };
        for i in 0..6 {
            let r = (x + (i % 3) as f32 * (tw + gap), y + (i / 3) as f32 * (th + gap), tw, th);
            let selected = i == self.fan;
            fill(g, r, 6.0, if selected { [41, 74, 64, 255] } else { SURFACE });
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
        self.editor(g, ui, (x, py, w, y + h - py), compact);
    }
    fn editor(&mut self, g: &mut GridRenderer, ui: &mut Ui, (x,py,w,h): Rect, compact: bool) {
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
        center(g, ui, apply, if self.pending { "적용 중" } else if self.tab == Tab::Graphics && !self.ready() { "미리보기 전용" } else if self.tab == Tab::Graphics { "GPU 색 저장" } else { "색 적용" }, 13.0, if ready { INK } else { MUTED }, true);
        if ready {
            self.hit(g, apply, Action::Apply);
        }
        let reset = (x, cy + ch + 3.0, w, (py + h - cy - ch - 3.0).clamp(19.0, 32.0));
        if let Some(err) = &self.error {
            center(g, ui, reset, err, 11.0, AMBER, false);
        } else {
            let preview = self.tab == Tab::Graphics && self.edited;
            let reset = if preview {
                let revert = (x, reset.1, w*0.5, reset.3);
                center(g, ui, revert, "미리보기 취소", 12.0, RAIN, false);
                self.hit(g, revert, Action::Revert);
                (x + w*0.5, reset.1, w*0.5, reset.3)
            } else { reset };
            center(g, ui, reset, "효과 따르기", 12.0, if self.ready() { TEXT } else { MUTED }, false);
            if self.ready() { self.hit(g, reset, Action::Release); }
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn gpu_preview_edits_do_not_overwrite_the_host_snapshot() {
        let raw = json!({"connected":true,"fans": super::super::SPOTS.map(|s| json!({"spot":s})),
            "gpu_light":{"controller":true,"sync":true,"asleep":true,"fixed":"993300","sample":"ccddee","effect":"ice"}});
        let mut panel = Panel { data: Some(Arc::new(Snapshot::parse(&raw).unwrap())), ..Default::default() };
        panel.choose(LightPart::Gpu);
        assert_eq!(panel.gpu_color(), Some([0;3]));
        panel.adjust(Action::Hue, 1.0/3.0);
        panel.adjust(Action::Brightness, 0.0);
        panel.adjust(Action::Brightness, 0.5);
        assert_eq!(panel.gpu_color(), Some([0,128,0]));
        panel.activate(Action::Revert);
        assert_eq!(panel.gpu_color(), Some([0;3]));
        assert_eq!(panel.draft, "993300");
        let mut old = raw;
        old.as_object_mut().unwrap().remove("gpu_light");
        panel.data = Some(Arc::new(Snapshot::parse(&old).unwrap()));
        assert!(!panel.ready());
        panel.activate(Action::Swatch(2));
        assert_eq!(panel.gpu_color(), Some(PALETTE[2]));
        panel.close();
        assert_eq!(panel.gpu_color(), None);
    }
}
