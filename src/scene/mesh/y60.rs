//! Original Y60 Snow White scene model, dimensioned in millimetres from HYTE's product specifications.

mod components;

use super::{mat, Mesh};
use crate::scene::catalog::Geometry;
use glam::{vec3, Quat, Vec3};
use std::f32::consts::{SQRT_2, TAU};

type Color = [u8; 3];
const WHITE: Color = [243, 245, 242];
const PORCELAIN: Color = [214, 222, 222];
const ALUMINIUM: Color = [165, 180, 182];
const SEAL: Color = [34, 40, 43];
const BOARD: Color = [66, 74, 77];
const SILVER: Color = [191, 201, 202];
const ICE: Color = [125, 228, 231];
const LILAC: Color = [187, 167, 235];
const DEPTH: f32 = 456.0;
const WIDTH: f32 = 285.0;
const HEIGHT: f32 = 462.0;
const CUT: f32 = 80.0;

// The inset follows the diagonal's own normal so its rim stays the same thickness as the straight sides.
fn outline(inset: f32, y: f32) -> [Vec3; 5] {
    let x = DEPTH * 0.5 - inset;
    let z = WIDTH * 0.5 - inset;
    let cut = CUT - (2.0 - SQRT_2) * inset;
    [vec3(-x, y, -z), vec3(-x, y, z), vec3(x - cut, y, z), vec3(x, y, z - cut), vec3(x, y, -z)]
}

fn join(m: &mut Mesh, lower: [Vec3; 5], upper: [Vec3; 5], color: Color) {
    for i in 0..5 {
        let j = (i + 1) % 5;
        m.quad([lower[i], lower[j], upper[j], upper[i]], color, mat::SOLID);
    }
}

fn shell_band(m: &mut Mesh, low: f32, high: f32) {
    let base = outline(1.4, low);
    let rim = outline(1.4, high);
    join(m, base, outline(0.0, low + 1.5), PORCELAIN);
    join(m, outline(0.0, low + 1.5), outline(0.0, high - 1.5), WHITE);
    join(m, outline(0.0, high - 1.5), rim, WHITE);
    let opening = outline(14.0, high);
    join(m, rim, opening, WHITE);
    join(m, opening, outline(14.0, high - 3.2), PORCELAIN);
    let well = outline(14.0, high - 3.2);
    for i in 0..5 {
        let j = (i + 1) % 5;
        m.face([vec3(0.0, high - 3.2, 0.0), well[i], well[j]], SEAL, mat::SOLID);
        m.face([vec3(0.0, low, 0.0), base[j], base[i]], PORCELAIN, mat::SOLID);
    }
    let left = -DEPTH * 0.5 + 13.8;
    for i in 0..34 {
        let z = -125.0 + i as f32 * 7.5;
        let right = (DEPTH * 0.5 - 14.0).min(DEPTH * 0.5 + WIDTH * 0.5 - CUT - 14.0 * SQRT_2 - z - 2.0);
        m.part([(left + right) * 0.5, high - 1.0, z], [right - left, 2.0, 4.6], 0.8, WHITE, mat::SOLID);
    }
}

fn axes(n: Vec3) -> (Vec3, Vec3) {
    let u = if n.y.abs() < 0.90 { Vec3::Y.cross(n).normalize() } else { Vec3::X };
    (u, n.cross(u).normalize())
}

fn annulus(m: &mut Mesh, center: Vec3, normal: Vec3, outer: f32, inner: f32, color: Color, material: f32) {
    let (u, v) = axes(normal);
    let edge = |i: usize| {
        let a = i as f32 * TAU / 40.0;
        u * a.cos() + v * a.sin()
    };
    for i in 0..40 {
        let (a, b) = (edge(i), edge(i + 1));
        if inner == 0.0 {
            m.face([center, center + a * outer, center + b * outer], color, material);
        } else {
            m.quad([center + a * inner, center + a * outer, center + b * outer, center + b * inner], color, material);
        }
    }
}

fn cylindrical_side(m: &mut Mesh, start: Vec3, end: Vec3, radius: f32, color: Color) {
    let axis = (end - start).normalize();
    let (u, v) = axes(axis);
    let edge = |i: usize| {
        let a = i as f32 * TAU / 28.0;
        u * a.cos() + v * a.sin()
    };
    for i in 0..28 {
        let (a, b) = (edge(i), edge(i + 1));
        for (p, n) in [
            (start + a * radius, a),
            (start + b * radius, b),
            (end + b * radius, b),
            (start + a * radius, a),
            (end + b * radius, b),
            (end + a * radius, a),
        ] {
            m.push(p, n, color, mat::SOLID);
        }
    }
}

// Carry each ring along the tangent to avoid cracks at the hose bends.
fn hose(m: &mut Mesh, controls: [Vec3; 4], radius: f32, color: Color) {
    let [a, b, c, d] = controls;
    let ring = |i: usize| {
        let t = i as f32 / 24.0;
        let s = 1.0 - t;
        let p = a * s * s * s + b * 3.0 * s * s * t + c * 3.0 * s * t * t + d * t * t * t;
        let tangent = ((b - a) * s * s + (c - b) * 2.0 * s * t + (d - c) * t * t).normalize();
        let u = tangent.cross(Vec3::Z).normalize();
        let v = tangent.cross(u);
        std::array::from_fn::<_, 12, _>(|k| {
            let angle = k as f32 * TAU / 12.0;
            let n = u * angle.cos() + v * angle.sin();
            (p + n * radius, n)
        })
    };
    let mut last = ring(0);
    for i in 1..=24 {
        let next = ring(i);
        for k in 0..12 {
            let j = (k + 1) % 12;
            for (p, n) in [last[k], last[j], next[j], last[k], next[j], next[k]] {
                m.push(p, n, color, mat::SOLID);
            }
        }
        last = next;
    }
}

fn panels(m: &mut Mesh) {
    for x in [-191.0, 180.0] {
        for z in [-112.0, 89.0] {
            m.part([x, 6.0, z], [43.0, 12.0, 36.0], 6.0, SEAL, mat::SOLID);
        }
    }
    shell_band(m, 11.0, 56.0);
    shell_band(m, 412.0, HEIGHT);
    m.part([-224.0, 234.0, 0.0], [7.0, 356.0, WIDTH - 5.0], 1.5, WHITE, mat::SOLID);
    m.part([0.0, 234.0, -139.0], [DEPTH - 7.0, 356.0, 7.0], 1.5, WHITE, mat::SOLID);
    m.part([-4.0, 234.0, -53.0], [DEPTH - 15.0, 356.0, 5.0], 2.0, PORCELAIN, mat::SOLID);
    for i in 0..29 {
        let x = -202.0 + i as f32 * 14.0;
        m.part([x, 241.0, -142.55], [6.0, 290.0, 0.65], 0.30, BOARD, mat::SOLID);
    }
    for i in 0..7 {
        m.part([-228.0, 89.0 + i as f32 * 17.0, 51.0], [0.8, 7.4, 90.0], 0.35, SEAL, mat::SOLID);
    }
    m.part([-228.0, 107.0, -83.0], [0.8, 78.0, 88.0], 0.4, BOARD, mat::SOLID);
    m.part([-228.6, 98.0, -83.0], [0.8, 22.0, 29.0], 0.35, SEAL, mat::SOLID);
    let lower = outline(0.6, 56.0);
    let upper = outline(0.6, 412.0);
    for side in 1..=3 {
        let j = (side + 1) % 5;
        m.quad([upper[side] - Vec3::Y * 13.0, upper[j] - Vec3::Y * 13.0, upper[j], upper[side]], SEAL, mat::SOLID);
        m.quad([lower[side], lower[j], lower[j] + Vec3::Y * 2.5, lower[side] + Vec3::Y * 2.5], SEAL, mat::SOLID);
    }
    // Exposed glass edges meet without a steel mullion at the two folds.
    for i in 2..=3 {
        m.slab((lower[i] + upper[i]) * 0.5, vec3(1.8, 343.0, 1.8), 0.5, [147, 175, 174], mat::SOLID);
    }
    let normal = vec3(1.0, 0.0, 1.0).normalize();
    let u = vec3(1.0, 0.0, -1.0).normalize();
    let center = (outline(0.0, 31.5)[2] + outline(0.0, 31.5)[3]) * 0.5 + normal * 0.3;
    let slot = |m: &mut Mesh, x: f32, w: f32, h: f32, color: Color| {
        let c = center + u * x + normal * if x == 0.0 && w < 7.0 { 0.18 } else { 0.0 };
        m.quad(
            [c - u * w - Vec3::Y * h, c + u * w - Vec3::Y * h, c + u * w + Vec3::Y * h, c - u * w + Vec3::Y * h],
            color,
            mat::SOLID,
        );
    };
    annulus(m, center - u * 35.5, normal, 3.5, 0.0, SEAL, mat::SOLID);
    for x in [-21.0, 18.0] {
        slot(m, x, 3.0, 6.1, SEAL);
    }
    slot(m, 0.0, 7.0, 7.0, ALUMINIUM);
    slot(m, 0.0, 5.4, 5.4, PORCELAIN);
    slot(m, 34.0, 2.1, 4.4, SEAL);
}

const CASE_YAW: f32 = -0.68;

pub(in crate::scene) fn fan_center(index: usize) -> Vec3 {
    Quat::from_rotation_y(CASE_YAW) * components::FANS[index].0 / 127.0
}

pub(in crate::scene) fn fan_hit(origin: Vec3, direction: Vec3) -> Option<usize> {
    let inverse = Quat::from_rotation_y(-CASE_YAW);
    let origin = (inverse * origin) * 127.0;
    let dir = inverse * direction;
    components::FANS
        .iter()
        .enumerate()
        .filter_map(|(i, &(center, normal))| {
            let dot = dir.dot(normal);
            if dot >= -0.015 {
                return None;
            }
            let t = (center - origin).dot(normal) / dot;
            (t >= 0.0 && (origin + t * dir - center).length_squared() <= 58.0 * 58.0).then_some((i, t))
        })
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(i, _)| i)
}

/// The three panes are the final triangles so the renderer can blend them after all opaque surfaces.
pub(in crate::scene) fn geometry() -> Geometry {
    let mut m = Mesh::default();
    panels(&mut m);
    components::n7_z790(&mut m);
    components::trident_z5(&mut m);
    components::kraken_elite(&mut m);
    components::neptune(&mut m);
    components::installed_fans(&mut m);
    let opaque_count = m.vertices.len();
    let lower = outline(0.0, 58.5);
    let upper = outline(0.0, 399.0);
    for i in 1..=3 {
        let j = (i + 1) % 5;
        let gap = (lower[j] - lower[i]).normalize() * 0.65;
        m.quad([lower[i] + gap, lower[j] - gap, upper[j] - gap, upper[i] + gap], SILVER, mat::GLASS);
    }
    let turn = Quat::from_rotation_y(CASE_YAW);
    let mut lo = Vec3::splat(f32::MAX);
    let mut hi = Vec3::splat(f32::MIN);
    for v in &mut m.vertices {
        let point = turn * Vec3::from_array(v.point) / 127.0;
        v.point = point.to_array();
        v.normal = (turn * Vec3::from_array(v.normal)).to_array();
        lo = lo.min(point);
        hi = hi.max(point);
    }
    Geometry {
        vertices: m.vertices,
        opaque_count,
        atlas: Vec::new(),
        center: (hi + lo) * 0.5,
        half: (hi - lo) * 0.5,
        screen: None,
    }
}

#[cfg(test)]
mod rgb_tests {
    use super::*;
    #[test]
    fn all_six_mounted_fans_can_be_picked_from_their_faces() {
        let turn = Quat::from_rotation_y(CASE_YAW);
        for (index, (_, normal)) in components::FANS.iter().enumerate() {
            let n = turn * *normal;
            assert_eq!(fan_hit(fan_center(index) + n * 0.12, -n), Some(index));
        }
    }
}
