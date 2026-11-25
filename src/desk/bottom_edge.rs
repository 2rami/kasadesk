use kasa_castview::{contains, Layout, Rect};

#[derive(Default)]
pub(super) struct BottomEdge {
    armed_image: Option<Rect>,
}

pub(super) struct EdgeHit {
    pub image_row: f32,
    pub remote_y: f64,
    pub local_row: f64,
    pub crossed: bool,
}

impl BottomEdge {
    pub fn clear(&mut self) {
        self.armed_image = None;
    }

    pub fn motion(&mut self, layout: Option<Layout>, p: (f64, f64), scale: f64) -> Option<EdgeHit> {
        let Some(layout) = layout else {
            self.clear();
            return None;
        };
        let (x, y, width, height) = layout.image;
        let bottom = y + height;
        let inside_x = p.0 >= f64::from(x) && p.0 < f64::from(x + width);
        if !inside_x
            || height <= 0.0
            || layout.uv.3 < 1.0
            || !contains(layout.slot, p)
            || bottom >= layout.slot.1 + layout.slot.3
        {
            self.clear();
            return None;
        }

        let crossed = p.1 as f32 >= bottom;
        if contains(layout.image, p) {
            self.armed_image = Some(layout.image);
        } else if !crossed || self.armed_image != Some(layout.image) {
            self.clear();
            return None;
        }
        // AppKit may round a warp to whole points, and the image hit test rounds coordinates to f32.
        // Land inside both grids so the resulting move still sends the final remote scanline.
        let image_row = bottom.next_down();
        let local_row = (f64::from(image_row).floor() * scale).floor();
        let remote_y = f64::from(y) + f64::from(height);
        (p.1 >= local_row / scale).then_some(EdgeHit { image_row, remote_y, local_row, crossed })
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
            let mut edge = BottomEdge::default();
            assert!(edge.motion(Some(l), (500.0, 780.0), scale).is_none());
            assert!(edge.motion(Some(l), (500.0, 450.0), scale).is_none());
            let hit = edge.motion(Some(l), (700.0, 789.0), scale).unwrap();
            assert!(hit.crossed);
            assert!(hit.local_row < hit.remote_y * scale);
            assert!(contains(l.image, (700.0, hit.local_row / scale)));
            assert_eq!((hit.remote_y - f64::from(l.image.1)) / f64::from(l.image.3), 1.0);
            let at_row = edge.motion(Some(l), (750.0, hit.local_row / scale), scale).unwrap();
            assert!(!at_row.crossed);
            assert!(edge.motion(Some(l), (750.0, hit.local_row / scale - 1.0), scale).is_none());
        }
    }

    #[test]
    fn menu_focus_exit_and_changed_geometry_cannot_capture_from_the_margin() {
        let l = layout(1.0);
        let mut edge = BottomEdge::default();
        for exit in [None, Some((1300.0, 760.0)), Some((200.0, 20.0))] {
            edge.motion(Some(l), (400.0, 400.0), 2.0);
            if let Some(p) = exit {
                edge.motion(Some(l), p, 2.0);
            } else {
                edge.motion(None, (400.0, 400.0), 2.0);
            }
            assert!(edge.motion(Some(l), (400.0, 750.0), 2.0).is_none());
        }
        edge.motion(Some(l), (400.0, 400.0), 2.0);
        assert!(edge.motion(Some(layout(1.2)), (400.0, 750.0), 2.0).is_none());
        assert!(edge.motion(Some(layout(2.0)), (400.0, 799.0), 2.0).is_none());
    }
}
