use super::mesh::Vertex;
use glam::{Vec2, Vec3, Vec4};

#[derive(Clone, Copy)]
pub(super) struct Panel {
    pub bounds: (Vec3, Vec3),
    pub horizontal: Vec4,
    pub vertical: Vec4,
    pub arc: [f32; 2],
    pub center: Vec3,
    pub normal: Vec3,
    pub size: Vec2,
}

impl Panel {
    pub fn measure(vertices: &[Vertex]) -> Option<Self> {
        let faces = || vertices.chunks_exact(3).filter(|tri| tri[0].material[3] >= 96.0);
        let mut lo = Vec3::splat(f32::INFINITY);
        let mut hi = Vec3::splat(f32::NEG_INFINITY);
        let mut normal = Vec3::ZERO;
        for tri in faces() {
            let point = |i: usize| Vec3::from_array(tri[i].point);
            let cross = (point(1) - point(0)).cross(point(2) - point(0));
            normal += if cross.z < 0.0 { -cross } else { cross };
            for v in tri {
                let p = Vec3::from_array(v.point);
                lo = lo.min(p);
                hi = hi.max(p);
            }
        }
        let normal = normal.try_normalize()?;
        let right = Vec3::Y.cross(normal).try_normalize()?;
        let up = normal.cross(right);
        let mut min = Vec3::splat(f32::INFINITY);
        let mut max = Vec3::splat(f32::NEG_INFINITY);
        for v in faces().flatten() {
            let p = Vec3::from_array(v.point);
            let local = Vec3::new(right.dot(p), up.dot(p), normal.dot(p));
            min = min.min(local);
            max = max.max(local);
        }
        let span = max - min;
        if span.x <= 0.0 || span.y <= 0.0 {
            return None;
        }
        // A cylindrical display has a sagitta along its normal. Unwrap its arc in the
        // vertex shader so the original curved triangles carry an evenly spaced picture.
        let (arc, width) = if span.z > span.x * 0.002 && span.z < span.x * 0.5 {
            let half_angle = 2.0 * (2.0 * span.z / span.x).atan();
            let sine = half_angle.sin();
            ([sine, 0.5 / half_angle], span.x * half_angle / sine)
        } else {
            ([0.0; 2], span.x)
        };
        Some(Self {
            bounds: (lo, hi),
            horizontal: right.extend(-min.x) / span.x,
            vertical: up.extend(-min.y) / span.y,
            arc,
            center: right * (min.x + span.x * 0.5) + up * (min.y + span.y * 0.5) + normal * min.z,
            normal,
            size: Vec2::new(width, span.y),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strip(columns: &[(Vec3, Vec3)]) -> Vec<Vertex> {
        let mut vertices = Vec::new();
        for pair in columns.windows(2) {
            let [(a, b), (c, d)] = [pair[0], pair[1]];
            for p in [a, c, b, b, c, d] {
                vertices.push(Vertex { point: p.to_array(), material: [1.0, 1.0, 1.0, 128.0], ..Vertex::default() });
            }
        }
        vertices
    }

    #[test]
    fn tilted_lid_uses_glass_height_and_ignores_emissive_trim() {
        let up = Vec3::new(0.0, 0.94, -0.342).normalize();
        let a = Vec3::new(-2.0, 1.0, -0.2);
        let b = a + Vec3::X * 4.0;
        let mut mesh = strip(&[(a, a + up * 2.5), (b, b + up * 2.5)]);
        mesh.extend([Vertex { point: [80.0, -12.0, 7.0], material: [1.0, 1.0, 1.0, 64.0], ..Vertex::default() }; 3]);
        let panel = Panel::measure(&mesh).unwrap();
        assert!((panel.size - Vec2::new(4.0, 2.5)).length() < 1e-4);
        assert!(panel.arc[0] == 0.0 && panel.vertical.dot((a + up * 2.5).extend(1.0)) > 0.9999);
        assert!((panel.center - (a + b + up * 2.5) * 0.5).length() < 1e-4);
    }

    #[test]
    fn curved_glass_uses_arc_length_and_maps_both_edges() {
        let columns: Vec<_> = (0..=8)
            .map(|i| {
                let t = -0.3 + i as f32 * 0.075;
                let p = Vec3::new(5.0 * t.sin(), 0.0, 5.0 * (1.0 - t.cos()));
                (p, p + Vec3::Y * 1.25)
            })
            .collect();
        let panel = Panel::measure(&strip(&columns)).unwrap();
        assert!((panel.size.x - 3.0).abs() < 1e-4 && (panel.size.y - 1.25).abs() < 1e-4);
        for (i, (p, _)) in columns.iter().enumerate() {
            let u = panel.horizontal.dot(p.extend(1.0));
            let arc_u = 0.5 + ((2.0 * u - 1.0) * panel.arc[0]).asin() * panel.arc[1];
            assert!((arc_u - i as f32 / 8.0).abs() < 1e-4);
        }
    }
}
