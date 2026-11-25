use kasa_castview::{contains, Layout, Rect};

#[derive(Default)]
pub(super) struct VerticalEdges {
    armed_image: Option<Rect>,
}

pub(super) struct EdgeHit {
    pub image_row: f32,
    pub remote_y: f64,
    pub local_row: f64,
    pub crossed: bool,
}

impl VerticalEdges {
    pub fn clear(&mut self) {
        self.armed_image = None;
    }

    pub fn motion(&mut self, layout: Layout, p: (f64, f64), scale: f64) -> Option<EdgeHit> {
        let (x, y, width, height) = layout.image;
        if height <= 0.0 || p.0 < f64::from(x) || p.0 >= f64::from(x + width) || !contains(layout.slot, p) {
            self.clear();
            return None;
        }

        if contains(layout.image, p) {
            self.armed_image = Some(layout.image);
        } else if self.armed_image != Some(layout.image) {
            self.clear();
            return None;
        }

        // AppKit may round a warp to whole points. Choose a backing pixel whose hotspot stays
        // inside the image after that rounding, and send the exact remote boundary separately.
        if y > layout.slot.1 && layout.uv.1 == 0.0 {
            let local_row = (f64::from(y).ceil() * scale).ceil();
            if p.1 <= local_row / scale {
                return Some(EdgeHit { image_row: y, remote_y: f64::from(y), local_row, crossed: (p.1 as f32) < y });
            }
        }
        let bottom = y + height;
        if bottom < layout.slot.1 + layout.slot.3 && layout.uv.3 == 1.0 {
            let image_row = bottom.next_down();
            let local_row = (f64::from(image_row).floor() * scale).floor();
            if p.1 >= local_row / scale {
                return Some(EdgeHit {
                    image_row,
                    remote_y: f64::from(y) + f64::from(height),
                    local_row,
                    crossed: p.1 as f32 >= bottom,
                });
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn layout(scale: f32) -> Layout {
        let slot = (0.0, 0.0, 1280.0, 800.0);
        let frame = (3840, 1600);
        let (image, uv) = kasa_castview::zoomed(slot, frame, scale, (0.5, 0.5));
        Layout {
            slot,
            image,
            uv,
            frame,
            toggle: (0.0, 0.0, 0.0, 0.0),
            zoom_chip: None,
            key_button: None,
            display_chip: None,
        }
    }

    #[test]
    fn downward_overshoot_and_horizontal_motion_reach_the_remote_last_row() {
        let l = layout(1.0);
        for scale in [1.0, 1.25, 1.5, 2.0] {
            let mut edge = VerticalEdges::default();
            assert!(edge.motion(l, (500.0, 780.0), scale).is_none());
            assert!(edge.motion(l, (500.0, 450.0), scale).is_none());
            let hit = edge.motion(l, (700.0, 789.0), scale).unwrap();
            assert!(hit.crossed);
            assert!(hit.local_row < hit.remote_y * scale);
            assert!(contains(l.image, (700.0, hit.local_row / scale)));
            assert_eq!((hit.remote_y - f64::from(l.image.1)) / f64::from(l.image.3), 1.0);
            let at_row = edge.motion(l, (750.0, hit.local_row / scale), scale).unwrap();
            assert!(!at_row.crossed);
            assert!(edge.motion(l, (750.0, hit.local_row / scale - 1.0), scale).is_none());
        }
    }

    #[test]
    fn upward_overshoot_sends_zero_and_releases_on_downward_motion() {
        let l = layout(1.0);
        for scale in [1.0, 1.25, 1.5, 2.0] {
            let mut edge = VerticalEdges::default();
            assert!(edge.motion(l, (150.0, 20.0), scale).is_none());
            assert!(edge.motion(l, (150.0, 350.0), scale).is_none());
            let hit = edge.motion(l, (150.0, 20.0), scale).unwrap();
            assert!(hit.crossed);
            assert_eq!(hit.remote_y, f64::from(l.image.1));
            assert!(contains(l.image, (150.0, hit.local_row / scale)));
            let at_row = edge.motion(l, (700.0, hit.local_row / scale), scale).unwrap();
            assert!(!at_row.crossed);
            assert_eq!(at_row.image_row, l.image.1);
            assert!(edge.motion(l, (700.0, (hit.local_row + 1.0) / scale), scale).is_none());
            edge.clear();
            assert!(edge.motion(l, (150.0, 20.0), scale).is_none());
        }
    }

    #[test]
    fn top_edge_only_applies_when_the_remote_first_row_and_upper_margin_are_visible() {
        let l = layout(1.0);
        for modified in [Layout { uv: (0.0, 0.1, 1.0, 0.9), ..l }, layout(2.0)] {
            let mut edge = VerticalEdges::default();
            edge.motion(modified, (120.0, 350.0), 2.0);
            assert!(edge.motion(modified, (120.0, 0.0), 2.0).is_none());
        }
    }

    #[test]
    fn menu_focus_exit_and_changed_geometry_cannot_capture_from_the_margin() {
        let l = layout(1.0);
        let mut edge = VerticalEdges::default();
        for exit in [None, Some((1300.0, 760.0)), Some((-1.0, 20.0))] {
            edge.motion(l, (400.0, 400.0), 2.0);
            if let Some(p) = exit {
                edge.motion(l, p, 2.0);
            } else {
                edge.clear();
            }
            assert!(edge.motion(l, (400.0, 750.0), 2.0).is_none());
        }
        edge.motion(l, (400.0, 400.0), 2.0);
        assert!(edge.motion(layout(1.2), (400.0, 750.0), 2.0).is_none());
        assert!(edge.motion(layout(2.0), (400.0, 799.0), 2.0).is_none());
    }
}
