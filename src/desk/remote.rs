use std::time::Instant;

use crate::{
    text::{fit, Ui},
    theme::{CHARCOAL, CLOUD, INK, MIST, RAIN, SURFACE},
};
use kasa_castview::{contains, zoomed, Layout, Rect, ScreenView, Surface};
use kasa_gridview::GridRenderer;

fn upload(g: &mut GridRenderer, v: &mut ScreenView, key: &str) {
    let next = v.new_surface().or_else(|| if g.has_image(key) { None } else { v.current_surface() });
    let Some((ptr, handle, width, height)) = next else { return };
    let start = Instant::now();
    #[cfg(target_os = "macos")]
    let shared = {
        let _ = handle;
        // ScreenView retains the decoded surface for the duration of the GPU upload.
        unsafe { g.set_image_iosurface(key, ptr, width, height) }
    };
    #[cfg(windows)]
    let shared = unsafe { g.set_image_d3d11(key, ptr, handle, width, height) };
    #[cfg(not(any(target_os = "macos", windows)))]
    let shared = {
        let _ = (ptr, handle, width, height);
        false
    };
    if !shared {
        v.copy_latest(|pixels, w, h, stride| g.update_image_bgra(key, pixels, w, h, stride));
    }
    v.note_upload(start.elapsed(), shared);
}

#[derive(Clone, Copy)]
enum Button {
    Back,
    Full,
    Feedback,
    Audio,
    Keys,
    Control,
    Display,
    Zoom,
}
pub(super) fn paint(
    g: &mut GridRenderer,
    ui: &mut Ui,
    v: &mut ScreenView,
    key: &str,
    slot: Rect,
    on: Surface,
    cursor: (f32, f32),
    fullscreen: bool,
    feedback: bool,
) {
    v.surface = on;
    upload(g, v, key);
    let (_, _, width, height) = slot;
    let live = v.has_frame();
    let frame = v.frame_size();
    let (image, uv) = zoomed(slot, frame, v.zoom, v.focus);
    if live {
        g.push_clip(0.0, 0.0, width, height);
        g.queue_image_region(key, image, uv);
        g.pop_clip();
    } else {
        let status = fit(g, ui, &v.status(), 14.0, false, width - 30.0).into_owned();
        let tw = ui.measure(g, &status, 14.0, false);
        ui.draw(g, (width - tw) * 0.5, height * 0.5, &status, 14.0, MIST, false);
    }
    let compact = width < 740.0;
    let short = |long, small| if compact { small } else { long };
    let mut buttons = vec![
        (Button::Back, short("내 기기", "기기").to_string(), false),
        (Button::Full, if fullscreen { "창으로" } else { short("전체 화면", "전체") }.into(), false),
        (Button::Feedback, short("입력 표시", "입력").into(), feedback),
    ];
    if live {
        if let Some((at, n, _)) = v.display_cycle() {
            buttons.push((Button::Display, format!("화면 {at}/{n}"), false));
        }
        if v.zoom > 1.001 {
            buttons.push((Button::Zoom, format!("{:.0}%", v.zoom * 100.0), false));
        }
        if v.audio.shown() {
            buttons.push((Button::Audio, v.audio.text().into(), v.audio.on));
        }
        if v.control {
            buttons.push((Button::Keys, short("특수 키", "키").into(), v.key_menu.is_some()));
        }
        buttons.push((Button::Control, if v.control { "조작 중" } else { "보기 전용" }.into(), v.control));
    }
    let pad = if compact { 7.0 } else { 12.0 };
    let mut right = width - 12.0;
    let mut y = 10.0;
    let mut boxes = Vec::new();
    for (act, label, selected) in buttons.into_iter().rev() {
        let w = ui.measure(g, &label, 12.0, true) + pad * 2.0;
        if right - w < 8.0 && !boxes.is_empty() {
            right = width - 12.0;
            y += 35.0;
        }
        let r = (right - w, y, w, 30.0);
        right = r.0 - 3.0;
        boxes.push((act, label, selected, r));
    }
    let toolbar_left = boxes.iter().map(|b| b.3 .0).fold(width, f32::min);
    if !compact && toolbar_left > 180.0 {
        let name = fit(g, ui, &v.label, 14.0, true, (toolbar_left - 48.0).min(210.0));
        let status = if live { v.hud() } else { String::new() };
        let tw = ui.measure(g, &name, 14.0, true).max(ui.measure(g, &status, 10.5, false));
        g.round_rect_fill(12.0, 10.0, tw + 27.0, if live { 42.0 } else { 30.0 }, 11.0, INK);
        g.round_rect_fill(21.0, 21.0, 5.0, 5.0, 2.5, if live { RAIN } else { MIST });
        ui.draw(g, 31.0, 16.0, &name, 14.0, CLOUD, true);
        if live {
            ui.draw(g, 31.0, 34.0, &status, 10.5, MIST, false);
        }
    }
    v.extra = vec![(0.0, 0.0, 0.0, 0.0); 3];
    v.audio.pill = None;
    let mut layout = Layout {
        slot,
        image,
        uv,
        frame,
        toggle: (0.0, 0.0, 0.0, 0.0),
        display_chip: None,
        key_button: None,
        zoom_chip: None,
    };
    let p = (f64::from(cursor.0), f64::from(cursor.1));
    for (act, label, selected, r) in boxes {
        let main = matches!(act, Button::Control) && selected;
        g.round_rect_fill(
            r.0,
            r.1,
            r.2,
            r.3,
            9.0,
            if main {
                RAIN
            } else if selected || contains(r, p) {
                SURFACE
            } else {
                INK
            },
        );
        ui.draw(g, r.0 + pad, r.1 + 9.0, &label, 12.0, if main { CHARCOAL } else { CLOUD }, true);
        match act {
            Button::Back => v.extra[0] = r,
            Button::Full => v.extra[1] = r,
            Button::Feedback => v.extra[2] = r,
            Button::Display => layout.display_chip = Some(r),
            Button::Zoom => layout.zoom_chip = Some(r),
            Button::Audio => v.audio.pill = Some(r),
            Button::Keys => layout.key_button = Some(r),
            Button::Control => layout.toggle = r,
        }
    }
    if !v.control || !live {
        v.key_menu = None;
    }
    if let (Some(key_rect), Some(_)) = (layout.key_button, v.key_menu.as_ref()) {
        let keys = v.special_keys();
        let row = 25.0;
        let h = row * keys.len() as f32 + 8.0;
        let w = keys.iter().map(|(t, _, _)| ui.measure(g, t, 12.0, false)).fold(0.0, f32::max) + 28.0;
        let x = (key_rect.0 + key_rect.2 - w).max(4.0);
        let y = (key_rect.1 + key_rect.3 + 4.0).min((height - h - 4.0).max(4.0));
        g.round_rect_fill(x, y, w, h, 10.0, INK);
        let mut rows = Vec::new();
        for (i, (t, _, _)) in keys.iter().enumerate() {
            let r = (x, y + 4.0 + i as f32 * row, w, row);
            if contains(r, p) {
                g.round_rect_fill(x + 3.0, r.1, w - 6.0, row, 6.0, SURFACE);
            }
            ui.draw(g, x + 12.0, r.1 + 6.0, t, 12.0, CLOUD, false);
            rows.push((r, i));
        }
        v.key_menu = Some(rows);
    }
    if live && v.control {
        let note = v.input_note();
        let msg = note.as_deref().unwrap_or(&v.preedit);
        if !msg.is_empty() {
            let msg = fit(g, ui, msg, 13.0, false, width - 50.0);
            let tw = ui.measure(g, &msg, 13.0, false);
            let x = (width - tw - 24.0) * 0.5;
            g.round_rect_fill(x, height - 83.0, tw + 24.0, 28.0, 8.0, INK);
            ui.draw(g, x + 12.0, height - 76.0, &msg, 13.0, CLOUD, false);
        }
    }
    v.layout = live.then_some(layout);
}
