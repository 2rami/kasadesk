//! 계정 메뉴와 관문 로그인 화면.

use kasa_castview::{contains, Rect};
use kasa_gridview::GridRenderer;
use winit::event::{ElementState, KeyEvent};
use winit::keyboard::{Key, NamedKey};

use crate::account::{Credential, Profile, Ticket};
use crate::text::{fit, Ui};

use crate::theme::{CHARCOAL as TEXT, PAPER as CARD, RULE as EDGE, SECONDARY as MUTE, TEAL as ACCENT};
const FIELD: [u8; 4] = [255, 255, 253, 255];
const CHIP: [u8; 4] = [231, 237, 229, 255];
const WARN: [u8; 4] = [145, 59, 41, 255];
const PAD: f32 = 28.0;
const ROW: f32 = 36.0;
const CARD_W: f32 = 400.0;

#[derive(Clone, PartialEq)]
enum Hit {
    Chip,
    Body,
    Logout,
    Provider(String),
    Field(usize),
    Submit,
    Signup,
    Close,
}

/// 누름·키가 부른 일 — 관문 일은 부르는 쪽이 비동기로 한다.
pub enum Action {
    None,
    Redraw,
    /// 판을 열었다 — 켜진 로그인 방법을 물을 때.
    Opened,
    ProfileOpened,
    Logout,
    Provider(String),
    Password(String, String),
    /// 처음 보는 신원으로 새 계정을 만든다.
    Signup,
    /// 처음 보는 신원을 이 아이디 계정에 잇는다.
    Claim(String, String),
}

#[derive(Default, PartialEq)]
enum Pane {
    #[default]
    Closed,
    Login,
    Profile,
}

#[derive(Default)]
pub struct Login {
    pane: Pane,
    profile: Option<Profile>,
    focus_logout: bool,
    field: usize,
    id: String,
    pw: String,
    /// 관문 일을 기다리는 중 — 그동안 단추를 다시 안 받는다.
    pub busy: Option<&'static str>,
    pub error: Option<String>,
    pub providers: Vec<String>,
    /// 처음 보는 신원 — 있으면 판이 새 계정·기존 계정을 묻는다.
    pub choose: Option<Ticket>,
    hits: Vec<(Rect, Hit)>,
}

fn provider_name(p: &str) -> &str {
    match p {
        "google" => "Google",
        "github" => "GitHub",
        other => other,
    }
}

impl Login {
    pub fn is_open(&self) -> bool {
        self.pane != Pane::Closed
    }

    pub fn signing_in(&self) -> bool {
        self.pane == Pane::Login
    }

    pub fn open_login(&mut self) {
        self.pane = Pane::Login;
    }

    pub fn set_profile(&mut self, profile: Profile) {
        if self.pane == Pane::Profile {
            self.profile = Some(profile);
        }
    }

    pub fn clickable(&self, p: (f64, f64)) -> bool {
        self.hits.iter().any(|(r, hit)| *hit != Hit::Body && contains(*r, p))
    }

    pub fn paint_chip(
        &mut self,
        g: &mut GridRenderer,
        ui: &mut Ui,
        width: f32,
        cred: Option<&Credential>,
        visible: bool,
    ) {
        self.hits.clear();
        if !visible {
            return;
        }
        let name = cred.map_or("로그인", |c| if c.display.is_empty() { "내 계정" } else { c.display.as_str() });
        let label = fit(g, ui, name, 13.0, true, (width * 0.3 - 42.0).clamp(48.0, 148.0));
        let w = ui.measure(g, &label, 13.0, true) + 45.0;
        let r = (width - 16.0 - w, 16.0, w, 34.0);
        g.round_rect_fill(r.0, r.1 + 1.5, w, r.3, 17.0, [16, 26, 28, 65]);
        g.round_rect_fill(r.0, r.1, w, r.3, 17.0, CARD);
        g.round_rect_fill(r.0 + 6.0, r.1 + 5.0, 24.0, 24.0, 12.0, CHIP);
        g.round_rect_fill(r.0 + 15.0, r.1 + 9.0, 6.0, 6.0, 3.0, ACCENT);
        g.round_rect_fill(r.0 + 12.0, r.1 + 16.0, 12.0, 6.0, 3.0, ACCENT);
        ui.draw(g, r.0 + 34.0, r.1 + 10.0, &label, 13.0, TEXT, true);
        self.hits.push((r, Hit::Chip));
    }

    fn paint_profile(&mut self, g: &mut GridRenderer, ui: &mut Ui, width: f32, c: &Credential, devices: usize) {
        let w = (width - 32.0).min(320.0);
        let (x, y) = (width - 16.0 - w, 58.0);
        let rect = (x, y, w, 218.0);
        self.hits.retain(|(_, hit)| *hit == Hit::Chip);
        self.hits.push((rect, Hit::Body));
        g.round_rect_fill(x, y + 4.0, w, rect.3, 14.0, [16, 24, 24, 55]);
        g.round_rect_fill(x, y, w, rect.3, 14.0, CARD);
        let l = x + 19.0;
        ui.draw(g, l, y + 16.0, "내 계정", 13.0, MUTE, true);
        let close = (x + w - 42.0, y + 7.0, 34.0, 32.0);
        ui.draw(g, close.0 + 11.0, close.1 + 8.0, "×", 17.0, MUTE, false);
        self.hits.push((close, Hit::Close));
        let profile = self.profile.as_ref();
        let name = profile.and_then(|v| v.display.as_deref()).unwrap_or(&c.display);
        let name = fit(g, ui, if name.is_empty() { &c.account } else { name }, 23.0, true, w - 38.0);
        ui.draw(g, l, y + 42.0, &name, 23.0, TEXT, true);
        g.rect(l, y + 78.0, w - 38.0, 1.0, EDGE);
        let count = format!("{devices}대");
        let login = profile.and_then(|v| v.login.as_deref());
        let mut fields = vec![(if login.is_some() { "로그인 ID" } else { "계정 ID" }, login.unwrap_or(&c.account))];
        if let Some(device) = profile.and_then(|v| v.device.as_deref()) {
            fields.push(("이 기기", device));
        }
        fields.push(("온라인 기기", count.as_str()));
        for (i, (key, value)) in fields.into_iter().enumerate() {
            let yy = y + 92.0 + i as f32 * 24.0;
            ui.draw(g, l, yy, key, 12.0, MUTE, false);
            let value = fit(g, ui, value, 13.0, false, w - 140.0);
            let tw = ui.measure(g, &value, 13.0, false);
            ui.draw(g, x + w - 19.0 - tw, yy, &value, 13.0, TEXT, false);
        }
        g.rect(l, y + 168.0, w - 38.0, 1.0, EDGE);
        let row = (x + 8.0, y + 175.0, w - 16.0, 35.0);
        if self.focus_logout {
            g.round_rect_stroke(row.0, row.1, row.2, row.3, 7.0, 1.5, ACCENT);
        }
        ui.draw(g, l, y + 186.0, "이 기기에서 로그아웃", 13.0, WARN, false);
        self.hits.push((row, Hit::Logout));
    }

    pub fn paint_panel(
        &mut self,
        g: &mut GridRenderer,
        ui: &mut Ui,
        (w, h): (f32, f32),
        cred: Option<&Credential>,
        devices: usize,
    ) {
        match self.pane {
            Pane::Closed => return,
            Pane::Profile => {
                if let Some(c) = cred {
                    self.paint_profile(g, ui, w, c, devices);
                } else {
                    self.close();
                }
                return;
            }
            Pane::Login => {}
        }
        self.hits.clear();
        g.rect(0.0, 0.0, w, h, [0, 0, 0, 150]);
        let card_w = CARD_W.min((w - 16.0).max(1.0));
        let inner = card_w - PAD * 2.0;
        let choose = self.choose.clone();
        let signup = choose.as_ref().is_some_and(|t| t.signup);
        let compact = h < 440.0;
        let header_h = if compact { 56.0 } else { 80.0 } - if choose.is_none() { 13.0 } else { 0.0 };
        let gap = if compact { 6.0 } else { 10.0 };
        let row = if compact { 31.0 } else { ROW };
        let options = signup || (choose.is_none() && !self.providers.is_empty());
        let options_h = if options { row + gap + 22.0 } else { 0.0 };
        let card_h = header_h + options_h + 3.0 * row + 2.0 * gap + if self.error.is_some() { 30.0 } else { 14.0 };
        let (cx, cy) = (((w - card_w) / 2.0).max(8.0), ((h - card_h) / 2.0).max(8.0));
        g.round_rect_fill(cx, cy, card_w, card_h, 12.0, CARD);
        g.round_rect_stroke(cx, cy, card_w, card_h, 12.0, 1.0, EDGE);

        let (title, lead) = match &choose {
            Some(t) => (
                format!("{} 로 처음 왔어요", provider_name(&t.provider)),
                if t.display.is_empty() {
                    "처음 보는 계정이에요".to_string()
                } else {
                    format!("{} · 처음 보는 계정이에요", t.display)
                },
            ),
            None => ("카사계정 로그인".to_string(), String::new()),
        };
        let title_y = cy + if compact { 12.0 } else { 22.0 };
        ui.draw(g, cx + PAD, title_y, &title, 20.0, TEXT, true);
        if !lead.is_empty() {
            let lead = fit(g, ui, &lead, 12.0, false, inner);
            ui.draw(g, cx + PAD, title_y + 25.0, &lead, 12.0, MUTE, false);
        }
        let close = (cx + card_w - 42.0, cy + 6.0, 34.0, 34.0);
        ui.draw(g, close.0 + 11.0, close.1 + 9.0, "×", 17.0, MUTE, false);
        self.hits.push((close, Hit::Close));
        let mut y = cy + header_h;
        if options {
            let buttons: Vec<_> = if signup {
                vec![("새 계정으로 시작".to_string(), Hit::Signup)]
            } else {
                self.providers
                    .iter()
                    .map(|p| (format!("{}로 계속", provider_name(p)), Hit::Provider(p.clone())))
                    .collect()
            };
            let bw = (inner - gap * (buttons.len() - 1) as f32) / buttons.len() as f32;
            for (i, (label, hit)) in buttons.into_iter().enumerate() {
                let r = (cx + PAD + i as f32 * (bw + gap), y, bw, row);
                g.round_rect_fill(r.0, r.1, r.2, r.3, 8.0, CHIP);
                let tw = ui.measure(g, &label, 13.0, true);
                ui.draw(g, r.0 + (bw - tw) * 0.5, r.1 + (row - 13.0) * 0.5, &label, 13.0, TEXT, true);
                self.hits.push((r, hit));
            }
            y += row + gap;
            ui.draw(
                g,
                cx + PAD,
                y + 2.0,
                if signup { "또는 기존 계정에 연결" } else { "또는 아이디로 로그인" },
                11.5,
                MUTE,
                false,
            );
            y += 22.0;
        }
        let masked = "•".repeat(self.pw.chars().count());
        for (i, (hint, value)) in [("아이디", self.id.clone()), ("비밀번호", masked)].into_iter().enumerate() {
            let r = (cx + PAD, y, inner, row);
            g.round_rect_fill(r.0, r.1, r.2, r.3, 8.0, FIELD);
            g.round_rect_stroke(r.0, r.1, r.2, r.3, 8.0, 1.0, if self.field == i { ACCENT } else { EDGE });
            let value = fit(g, ui, &value, 13.0, false, inner - 28.0);
            if value.is_empty() {
                ui.draw(g, r.0 + 12.0, r.1 + 10.0, hint, 13.0, MUTE, false);
            } else {
                ui.draw(g, r.0 + 12.0, r.1 + 10.0, &value, 13.0, TEXT, false);
            }
            if self.field == i && self.busy.is_none() {
                let x = r.0 + 12.0 + if value.is_empty() { 0.0 } else { ui.measure(g, &value, 13.0, false) } + 1.0;
                g.rect(x, r.1 + 8.0, 1.0, 16.0, ACCENT);
            }
            self.hits.push((r, Hit::Field(i)));
            y += row + gap;
        }
        let r = (cx + PAD, y, inner, row);
        g.round_rect_fill(r.0, r.1, r.2, r.3, 8.0, if self.busy.is_some() || choose.is_some() { CHIP } else { ACCENT });
        let label = self.busy.unwrap_or(if choose.is_some() { "이 계정에 잇기" } else { "로그인" });
        let tw = ui.measure(g, label, 13.0, false);
        ui.draw(
            g,
            r.0 + (r.2 - tw) / 2.0,
            r.1 + 10.0,
            label,
            13.0,
            if self.busy.is_some() || choose.is_some() { TEXT } else { CARD },
            false,
        );
        self.hits.push((r, Hit::Submit));
        if let Some(e) = &self.error {
            ui.draw(g, cx + PAD, y + row + 8.0, e, 11.0, WARN, false);
        }
    }

    pub fn click(&mut self, p: (f64, f64), logged_in: bool) -> Action {
        let hit = self.hits.iter().rev().find(|(r, _)| contains(*r, p)).map(|(_, h)| h.clone());
        match hit {
            Some(Hit::Chip) if self.busy.is_some() => Action::None,
            Some(Hit::Chip) if self.is_open() => {
                self.close();
                Action::Redraw
            }
            Some(Hit::Chip) if logged_in => {
                self.pane = Pane::Profile;
                self.focus_logout = false;
                Action::ProfileOpened
            }
            Some(Hit::Chip) => {
                self.open_login();
                self.error = None;
                Action::Opened
            }
            _ if !self.is_open() || self.busy.is_some() => Action::None,
            Some(Hit::Logout) if self.pane == Pane::Profile && logged_in => {
                self.close();
                Action::Logout
            }
            Some(Hit::Close) | None if self.pane == Pane::Profile => {
                self.close();
                Action::Redraw
            }
            _ if self.pane == Pane::Profile => Action::None,
            Some(Hit::Close) => {
                self.close();
                Action::Redraw
            }
            Some(Hit::Field(i)) => {
                self.field = i;
                Action::Redraw
            }
            Some(Hit::Provider(p)) => self.start(Action::Provider(p)),
            Some(Hit::Signup) => self.start(Action::Signup),
            Some(Hit::Submit) => self.submit(),
            Some(Hit::Body | Hit::Logout) | None => Action::None,
        }
    }

    /// 판이 열려 있으면 키를 다 받는다(true). 칸 사이 Tab, Enter 로 로그인, Esc 로 닫기.
    pub fn key(&mut self, event: &KeyEvent) -> Option<Action> {
        if !self.is_open() {
            return None;
        }
        if event.state != ElementState::Pressed || self.busy.is_some() {
            return Some(Action::None);
        }
        if self.pane == Pane::Profile {
            let action = match &event.logical_key {
                Key::Named(NamedKey::Escape) => {
                    self.close();
                    Action::Redraw
                }
                Key::Named(NamedKey::Tab) => {
                    self.focus_logout = !self.focus_logout;
                    Action::Redraw
                }
                Key::Named(NamedKey::Enter | NamedKey::Space) => {
                    let logout = self.focus_logout;
                    self.close();
                    if logout {
                        Action::Logout
                    } else {
                        Action::Redraw
                    }
                }
                _ => Action::None,
            };
            return Some(action);
        }
        let value = if self.field == 0 { &mut self.id } else { &mut self.pw };
        Some(match &event.logical_key {
            Key::Named(NamedKey::Escape) => {
                self.close();
                Action::Redraw
            }
            Key::Named(NamedKey::Tab) => {
                self.field = 1 - self.field;
                Action::Redraw
            }
            Key::Named(NamedKey::Enter) => self.submit(),
            Key::Named(NamedKey::Backspace) => {
                value.pop();
                Action::Redraw
            }
            _ => match event.text.as_ref().filter(|t| !t.chars().any(char::is_control)) {
                Some(t) if value.chars().count() < 256 => {
                    value.push_str(t);
                    Action::Redraw
                }
                _ => Action::None,
            },
        })
    }

    /// 입력기를 거친 글(Windows)도 지금 칸으로.
    pub fn commit(&mut self, text: &str) -> bool {
        if self.pane != Pane::Login || self.busy.is_some() {
            return false;
        }
        let value = if self.field == 0 { &mut self.id } else { &mut self.pw };
        value.push_str(text);
        true
    }

    fn submit(&mut self) -> Action {
        if self.id.trim().is_empty() || self.pw.is_empty() {
            self.error = Some("아이디와 비밀번호를 넣어 주세요".into());
            self.field = if self.id.trim().is_empty() { 0 } else { 1 };
            return Action::Redraw;
        }
        let (id, pw) = (self.id.trim().to_string(), std::mem::take(&mut self.pw));
        let action = if self.choose.is_some() { Action::Claim(id, pw) } else { Action::Password(id, pw) };
        self.start(action)
    }

    /// 처음 보는 신원 — 판을 열어 새 계정·기존 계정을 묻는다.
    pub fn ask(&mut self, ticket: Ticket) {
        self.busy = None;
        self.error = None;
        self.open_login();
        self.field = 0;
        self.choose = Some(ticket);
    }

    fn start(&mut self, action: Action) -> Action {
        self.error = None;
        self.busy = Some(match action {
            Action::Provider(_) => "브라우저에서 로그인을 마쳐 주세요",
            Action::Signup => "계정을 만드는 중",
            Action::Claim(..) => "계정에 잇는 중",
            _ => "로그인하는 중",
        });
        action
    }

    pub fn done(&mut self, result: Result<(), String>) {
        self.busy = None;
        match result {
            Ok(()) => self.close(),
            Err(e) => self.error = Some(e),
        }
    }

    pub fn close(&mut self) {
        self.pane = Pane::Closed;
        self.profile = None;
        self.hits.retain(|(_, hit)| *hit == Hit::Chip);
        self.pw.clear();
        self.error = None;
        self.choose = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn account_button_and_dismissal_preserve_the_session() {
        let mut login = Login::default();
        let chip = (5.0, 5.0, 40.0, 25.0);
        login.hits.push((chip, Hit::Chip));
        for dismiss in [(10.0, 10.0), (90.0, 200.0)] {
            assert!(matches!(login.click((10.0, 10.0), true), Action::ProfileOpened));
            assert!(!login.commit("private"));
            assert!(matches!(login.click(dismiss, true), Action::Redraw));
            assert!(!login.is_open());
        }
        assert!(matches!(login.click((10.0, 10.0), true), Action::ProfileOpened));
        login.hits.push(((5.0, 60.0, 80.0, 30.0), Hit::Logout));
        assert!(matches!(login.click((10.0, 70.0), true), Action::Logout));
        assert!(matches!(login.click((10.0, 70.0), true), Action::None));
        assert!(matches!(login.click((10.0, 10.0), false), Action::Opened));
        assert!(login.signing_in());
    }
}
