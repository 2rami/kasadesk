use kasa_gridview::GridRenderer;

pub const INK: [u8; 4] = [20, 40, 67, 250];
pub const SURFACE: [u8; 4] = [44, 70, 104, 255];
pub const EDGE: [u8; 4] = [91, 123, 158, 255];
pub const CLOUD: [u8; 4] = [248, 246, 235, 255];
pub const MIST: [u8; 4] = [188, 212, 229, 255];
pub const RAIN: [u8; 4] = [138, 212, 234, 255];
pub const SUN: [u8; 4] = [255, 194, 123, 255];

pub fn sunshower(g: &mut GridRenderer, x: f32, y: f32, alpha: f32) {
    let ink = |c: [u8; 4]| [c[0], c[1], c[2], (f32::from(c[3]) * alpha) as u8];
    g.round_rect_fill(x + 13.0, y, 18.0, 18.0, 9.0, ink(SUN));
    g.round_rect_fill(x + 4.0, y + 8.0, 16.0, 16.0, 8.0, ink(CLOUD));
    g.round_rect_fill(x + 13.0, y + 13.0, 13.0, 11.0, 5.5, ink(CLOUD));
    g.round_rect_fill(x, y + 16.0, 26.0, 8.0, 4.0, ink(CLOUD));
    for (dx, dy) in [(5.0, 0.0), (12.0, 2.0), (19.0, 0.0)] {
        g.round_rect_fill(x + dx, y + 26.0 + dy, 2.0, 4.0, 1.0, ink(RAIN));
    }
}
