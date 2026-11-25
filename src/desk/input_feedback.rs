use std::time::{Duration, Instant};

use crate::{
    text::Ui,
    theme::{CLOUD, INK, RAIN},
};
use kasa_gridview::GridRenderer;
use winit::keyboard::KeyCode;

pub(super) struct InputFeedback {
    pub enabled: bool,
    last: Option<(String, Instant)>,
    marker: Option<((f32, f32), Instant)>,
    pressed_at: Option<(f32, f32)>,
}

impl Default for InputFeedback {
    fn default() -> Self {
        Self { enabled: true, last: None, marker: None, pressed_at: None }
    }
}

impl InputFeedback {
    pub fn clear(&mut self) {
        self.last = None;
        self.marker = None;
        self.pressed_at = None;
    }
    pub fn show(&mut self, label: impl Into<String>) {
        if self.enabled {
            self.last = Some((label.into(), Instant::now() + Duration::from_millis(1250)));
        }
    }
    pub fn click(&mut self, p: (f32, f32), label: &str) {
        if self.enabled {
            self.show(label);
            self.marker = Some((p, Instant::now() + Duration::from_millis(320)));
            self.pressed_at = Some(p);
        }
    }
    pub fn drag(&mut self, p: (f32, f32), buttons: u8) -> bool {
        if buttons == 0 {
            self.pressed_at = None;
            return false;
        }
        if self.pressed_at.is_some_and(|(x, y)| (p.0 - x).hypot(p.1 - y) > 5.0) {
            self.pressed_at = None;
            self.show("드래그");
            return true;
        }
        false
    }
    pub fn tick(&mut self, now: Instant) -> (bool, Option<Instant>) {
        let mut changed = false;
        if self.marker.is_some_and(|(_, end)| now >= end) {
            self.marker = None;
            changed = true;
        }
        if self.last.as_ref().is_some_and(|(_, end)| now >= *end) {
            self.last = None;
            changed = true;
        }
        let next = self.marker.map(|(_, end)| end).into_iter().chain(self.last.as_ref().map(|(_, end)| *end)).min();
        (changed, next)
    }
    pub fn paint(&self, g: &mut GridRenderer, ui: &mut Ui, height: f32) {
        if let Some(((x, y), _)) = self.marker {
            g.round_rect_stroke(x - 10.0, y - 10.0, 20.0, 20.0, 10.0, 2.0, RAIN);
        }
        if let Some((label, _)) = &self.last {
            let w = ui.measure(g, label, 13.0, true) + 38.0;
            let y = (height - 48.0).max(0.0);
            g.round_rect_fill(14.0, y, w, 32.0, 11.0, INK);
            g.round_rect_fill(25.0, y + 13.0, 6.0, 6.0, 3.0, RAIN);
            ui.draw(g, 39.0, y + 9.0, label, 13.0, CLOUD, true);
        }
    }
}

pub(super) fn key_label(code: KeyCode, mods: u8, windows: bool) -> Option<String> {
    use KeyCode::*;
    if matches!(code, ShiftLeft | ShiftRight | ControlLeft | ControlRight | AltLeft | AltRight | SuperLeft | SuperRight)
    {
        return None;
    }
    let key = if cfg!(target_os = "macos") && windows && code == F13 {
        "PrintScreen".to_string()
    } else {
        format!("{code:?}")
    };
    let printable = key.starts_with("Key")
        || key.starts_with("Digit")
        || key.starts_with("Numpad")
        || matches!(
            code,
            Space
                | Comma
                | Period
                | Slash
                | Semicolon
                | Quote
                | BracketLeft
                | BracketRight
                | Backslash
                | Backquote
                | Minus
                | Equal
                | IntlBackslash
                | IntlRo
                | IntlYen
        );
    if printable && mods & 14 == 0 {
        return Some("문자 입력".into());
    }
    let mut labels = Vec::new();
    if mods & 2 != 0 {
        labels.push("Ctrl");
    }
    if mods & 4 != 0 {
        labels.push(if windows { "Alt" } else { "Option" });
    }
    if mods & 8 != 0 {
        labels.push(if windows { "Win" } else { "Cmd" });
    }
    if mods & 1 != 0 {
        labels.push("Shift");
    }
    labels.push(key.strip_prefix("Key").or_else(|| key.strip_prefix("Digit")).unwrap_or(&key));
    Some(labels.join(" + "))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn content_is_hidden_and_shortcuts_use_remote_modifiers() {
        assert_eq!(key_label(KeyCode::KeyA, 1, false).as_deref(), Some("문자 입력"));
        assert_eq!(key_label(KeyCode::KeyC, 2, true).as_deref(), Some("Ctrl + C"));
        assert_eq!(key_label(KeyCode::SuperLeft, 8, true), None);
    }
}
