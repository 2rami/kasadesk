use kasa_gridview::GridRenderer;

pub const INK: [u8; 4] = [20, 40, 67, 250];
pub const SURFACE: [u8; 4] = [44, 70, 104, 255];
pub const EDGE: [u8; 4] = [91, 123, 158, 255];
pub const CLOUD: [u8; 4] = [248, 246, 235, 255];
pub const MIST: [u8; 4] = [188, 212, 229, 255];
pub const RAIN: [u8; 4] = [138, 212, 234, 255];
pub const SUN: [u8; 4] = [255, 194, 123, 255];

pub fn weather_icon(g: &mut GridRenderer, x: f32, y: f32, alpha: f32, sky: crate::weather::Sky, night: bool) {
    use crate::weather::Sky;
    let ink = |c: [u8; 4]| [c[0], c[1], c[2], (f32::from(c[3]) * alpha) as u8];
    if sky == Sky::Clear {
        g.round_rect_fill(x + 3.0, y + 2.0, 24.0, 24.0, 12.0, ink(if night { CLOUD } else { SUN }));
        if night {
            g.round_rect_fill(x + 12.0, y, 19.0, 19.0, 9.5, ink(INK));
        }
        return;
    }
    if sky == Sky::Sunshower {
        g.round_rect_fill(x + 13.0, y, 18.0, 18.0, 9.0, ink(SUN));
    }
    let cloud = ink(if matches!(sky, Sky::Storm | Sky::Hail) { MIST } else { CLOUD });
    g.round_rect_fill(x + 4.0, y + 6.0, 16.0, 16.0, 8.0, cloud);
    g.round_rect_fill(x + 13.0, y + 11.0, 14.0, 11.0, 5.5, cloud);
    g.round_rect_fill(x, y + 14.0, 27.0, 8.0, 4.0, cloud);
    match sky {
        Sky::Fog => {
            for dy in [24.0, 28.0] { g.round_rect_fill(x + 2.0, y + dy, 23.0, 1.5, 0.75, ink(MIST)); }
        }
        Sky::Rain | Sky::Sunshower | Sky::Snow | Sky::Storm | Sky::Hail => {
            let snow = matches!(sky, Sky::Snow | Sky::Hail);
            for (dx, dy) in [(5.0, 0.0), (12.0, 2.0), (19.0, 0.0)] {
                g.round_rect_fill(x + dx, y + 25.0 + dy, if snow { 3.5 } else { 2.0 }, 4.0, 1.5,
                    ink(if snow { CLOUD } else if sky == Sky::Storm { SUN } else { RAIN }));
            }
        }
        _ => {}
    }
}
