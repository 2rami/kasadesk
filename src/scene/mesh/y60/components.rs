//! Component dimensions are in millimetres in the case coordinate system.
use super::{
    annulus, axes, cylindrical_side, hose, mat, Color, Mesh, ALUMINIUM, BOARD, ICE, LILAC, PORCELAIN, SEAL, SILVER, WHITE,
};
use glam::{vec2, vec3, Mat3, Vec3};
use std::f32::consts::TAU;

fn placed(m: &mut Mesh, part: &Mesh, origin: Vec3, basis: Mat3) {
    m.vertices.extend(part.vertices.iter().map(|v| {
        let mut v = *v;
        v.point = (origin + basis * Vec3::from_array(v.point)).to_array();
        v.normal = (basis * Vec3::from_array(v.normal)).to_array();
        v
    }));
}

fn stripe(m: &mut Mesh, a: Vec3, b: Vec3, normal: Vec3, width: f32, color: Color, material: f32) {
    let side = normal.cross(b - a).normalize_or_zero() * (width * 0.5);
    m.quad([a - side, b - side, b + side, a + side], color, material);
}

fn text(m: &mut Mesh, word: &str, origin: Vec3, height: f32, color: Color) {
    // Short product markings are geometry so they survive a close camera approach and mesh export.
    let strokes = |letter| -> &'static [&'static [(u8, u8)]] {
        match letter {
            'R' => &[&[(0, 0), (0, 6), (3, 6), (4, 5), (4, 4), (3, 3), (0, 3)], &[(2, 3), (4, 0)]],
            'I' => &[&[(2, 0), (2, 6)]],
            'D' => &[&[(0, 0), (0, 6), (2, 6), (4, 4), (4, 2), (2, 0), (0, 0)]],
            'N' => &[&[(0, 0), (0, 6), (4, 0), (4, 6)]],
            'E' => &[&[(4, 6), (0, 6), (0, 0), (4, 0)], &[(0, 3), (3, 3)]],
            'P' => &[&[(0, 0), (0, 6), (3, 6), (4, 5), (4, 4), (3, 3), (0, 3)]],
            'T' => &[&[(0, 6), (4, 6)], &[(2, 6), (2, 0)]],
            'U' => &[&[(0, 6), (0, 1), (1, 0), (3, 0), (4, 1), (4, 6)]],
            'Z' => &[&[(0, 6), (4, 6), (0, 0), (4, 0)]],
            'X' => &[&[(0, 6), (4, 0)], &[(0, 0), (4, 6)]],
            '7' => &[&[(0, 6), (4, 6), (1, 0)]],
            '5' => &[&[(4, 6), (0, 6), (0, 3), (3, 3), (4, 2), (4, 1), (3, 0), (0, 0)]],
            _ => &[],
        }
    };
    let unit = height / 6.0;
    for (idx, letter) in word.chars().enumerate() {
        let at = |&(x, y): &(u8, u8)| origin + vec3(idx as f32 * 5.7 + f32::from(x), f32::from(y), 0.0) * unit;
        for path in strokes(letter) {
            for pair in path.windows(2) {
                stripe(m, at(&pair[0]), at(&pair[1]), Vec3::Z, unit * 0.78, color, mat::SOLID);
            }
        }
    }
}

fn gradient(a: Color, b: Color, t: f32) -> Color {
    std::array::from_fn(|i| (f32::from(a[i]) * (1.0 - t) + f32::from(b[i]) * t).round() as u8)
}

fn radiator(m: &mut Mesh, origin: Vec3, basis: Mat3, size: Vec3) {
    // The fittings land on the end tanks; the fins occupy only the core between them.
    let mut part = Mesh::default();
    let (w, h, d) = (size.x, size.y, size.z);
    part.part([0.0, 0.0, -d * 0.5], [w, h, d], 3.0, WHITE, mat::SOLID);
    for sign in [-1.0, 1.0] {
        let z = if sign > 0.0 { 0.0 } else { -d };
        part.part([0.0, 0.0, z], [w - 9.0, h - 26.0, 1.0], 1.0, SEAL, mat::SOLID);
        let count = ((h - 30.0) / 4.0) as usize;
        for i in 0..count {
            let y = (i as f32 - (count - 1) as f32 * 0.5) * 4.0;
            part.part([0.0, y, z + sign * 0.7], [w - 12.0, 0.95, 0.7], 0.2, ALUMINIUM, mat::SOLID);
        }
    }
    placed(m, &part, origin, basis);
}

fn uni_fan() -> Mesh {
    let mut m = Mesh::default();
    let r = 55.7;
    // Project radii onto a clipped square to keep the intake open through the entire 25 mm frame.
    let edge = |a: f32| {
        let (y, x) = a.sin_cos();
        let reach = (60.0 / x.abs().max(y.abs())).min(114.0 / (x.abs() + y.abs()));
        vec2(x, y) * reach
    };
    for i in 0..64 {
        let a = TAU * i as f32 / 64.0;
        let b = TAU * (i + 1) as f32 / 64.0;
        let (oa, ob) = (edge(a), edge(b));
        let (ia, ib) = (vec2(a.cos(), a.sin()) * r, vec2(b.cos(), b.sin()) * r);
        m.quad([ia.extend(0.0), oa.extend(0.0), ob.extend(0.0), ib.extend(0.0)], WHITE, mat::SOLID);
        m.quad([oa.extend(-25.0), ob.extend(-25.0), ob.extend(0.0), oa.extend(0.0)], PORCELAIN, mat::SOLID);
        m.quad([ib.extend(-25.0), ob.extend(-25.0), oa.extend(-25.0), ia.extend(-25.0)], WHITE, mat::SOLID);
        m.quad([ib.extend(0.0), ib.extend(-25.0), ia.extend(-25.0), ia.extend(0.0)], ALUMINIUM, mat::SOLID);
    }
    for side in [-1.0, 1.0] {
        // SL-INF's side mirror sits inside a stepped capsule and the face has two separate angular light guides.
        let panel = [
            (-48.0, -7.0),
            (-41.0, -7.0),
            (-37.0, -5.0),
            (36.0, -5.0),
            (41.0, -7.0),
            (48.0, -7.0),
            (48.0, -18.0),
            (40.0, -18.0),
            (36.0, -20.0),
            (-36.0, -20.0),
            (-40.0, -18.0),
            (-48.0, -18.0),
        ];
        let p = |i: usize| vec3(panel[i].0, side * 60.1, panel[i].1);
        for i in 0..panel.len() {
            m.face([vec3(0.0, side * 60.1, -12.5), p(i), p((i + 1) % panel.len())], [57, 89, 104], mat::SOLID);
        }
        for depth in [0.0, 1.0, 2.0] {
            let project = |&(x, z): &(f32, f32)| {
                vec3(x * (1.0 - depth * 0.047), side * (60.2 + depth * 0.03), -12.5 + (z + 12.5) * (0.90 - depth * 0.22))
            };
            for i in 0..panel.len() {
                stripe(
                    &mut m,
                    project(&panel[i]),
                    project(&panel[(i + 1) % panel.len()]),
                    Vec3::Y * side,
                    0.65,
                    gradient(ICE, LILAC, depth * 0.36),
                    mat::RGB,
                );
            }
        }
        let marks = [(-46.0, 27.0), (-26.0, 52.7), (26.0, 52.7), (46.0, 27.0)];
        for w in marks.windows(2) {
            let point = |(x, y): (f32, f32)| vec3(x, side * y, 0.15);
            stripe(&mut m, point(w[0]), point(w[1]), Vec3::Z, 1.55, if side > 0.0 { ICE } else { LILAC }, mat::RGB);
        }
        for x in [-52.5, 52.5] {
            let pos = vec3(x, side * 52.5, 0.23);
            annulus(&mut m, pos, Vec3::Z, 3.4, 0.0, BOARD, mat::SOLID);
            stripe(
                &mut m,
                pos - Vec3::X * 1.8 + Vec3::Z * 0.1,
                pos + Vec3::X * 1.8 + Vec3::Z * 0.1,
                Vec3::Z,
                0.65,
                ALUMINIUM,
                mat::SOLID,
            );
        }
    }
    for blade in 0..7 {
        let phi = TAU * blade as f32 / 7.0;
        let row = |i: usize| {
            let t = i as f32 / 9.0;
            let rad = 17.0 + 37.5 * t;
            let point = |offset: f32, z: f32| vec3(rad * (phi + offset).cos(), rad * (phi + offset).sin(), z);
            [point(0.69 * t * t, -9.5 + 2.0 * t), point(0.69 * t * t + 0.42 + 0.34 * t, -4.2 + 2.7 * t)]
        };
        for i in 0..9 {
            let [a, b] = row(i);
            let [c, d] = row(i + 1);
            m.quad([a, c, d, b], gradient([177, 208, 215], [218, 230, 240], i as f32 / 8.0), mat::DIFFUSER);
        }
    }
    cylindrical_side(&mut m, vec3(0.0, 0.0, -20.0), Vec3::ZERO, 18.2, ALUMINIUM);
    annulus(&mut m, Vec3::ZERO, Vec3::Z, 18.2, 0.0, [42, 72, 83], mat::SOLID);
    for i in 0..6 {
        let radius = 17.3 - i as f32 * 1.7;
        let glow = gradient(ICE, [47, 78, 98], i as f32 / 5.5);
        annulus(&mut m, Vec3::Z * (0.15 + 0.015 * i as f32), Vec3::Z, radius, radius - 0.72, glow, mat::RGB);
    }
    for i in 0..4 {
        let angle = TAU * i as f32 / 4.0 + 0.65;
        let at = |radius: f32, a: f32| vec3(a.cos() * radius, a.sin() * radius, -24.1);
        stripe(&mut m, at(14.0, angle - 0.23), at(56.0, angle), Vec3::Z, 4.2, PORCELAIN, mat::SOLID);
    }
    m
}

pub(super) const FANS: [(Vec3, Vec3); 6] = [
    (vec3(-131.0, 360.0, 40.0), Vec3::NEG_Y),
    (vec3(-11.0, 360.0, 40.0), Vec3::NEG_Y),
    (vec3(109.0, 360.0, 40.0), Vec3::NEG_Y),
    (vec3(-195.0, 302.0, 29.0), Vec3::X),
    (vec3(141.0, 280.0, 8.0), Vec3::Z),
    (vec3(141.0, 160.0, 8.0), Vec3::Z),
];

pub(super) fn installed_fans(m: &mut Mesh) {
    let fan = uni_fan();
    for (index, (pos, normal)) in FANS.into_iter().enumerate() {
        let (u, v) = axes(normal);
        let start = m.vertices.len();
        placed(m, &fan, pos, Mat3::from_cols(u, v, normal));
        for (vertex, local) in m.vertices[start..].iter_mut().zip(&fan.vertices) {
            if vertex.material[3] == mat::DIFFUSER {
                vertex.material[3] = 17.0 + (2 * index) as f32;
                vertex.uv[0] = 1.0;
            } else if vertex.material[3] == mat::RGB {
                let outer = local.point[0].hypot(local.point[1]) > 20.0;
                vertex.material[3] = 17.0 + (2 * index + usize::from(outer)) as f32;
            }
        }
    }
}

fn perforations(m: &mut Mesh, corner: Vec3, rows: usize, columns: usize, pitch: f32) {
    for y in 0..rows {
        for x in 0..columns {
            let center = corner + vec3(x as f32 * pitch, y as f32 * pitch, 0.0);
            let r = 1.35;
            for i in 0..6 {
                let a = TAU * i as f32 / 6.0;
                let b = a + TAU / 6.0;
                m.face(
                    [center, center + vec3(r * a.cos(), r * a.sin(), 0.0), center + vec3(r * b.cos(), r * b.sin(), 0.0)],
                    SEAL,
                    mat::SOLID,
                );
            }
        }
    }
}

pub(super) fn n7_z790(m: &mut Mesh) {
    m.part([-83.0, 224.5, -47.5], [244.0, 305.0, 2.0], 1.5, BOARD, mat::SOLID);
    m.part([-83.0, 364.0, -35.0], [170.0, 24.0, 23.0], 2.7, WHITE, mat::SOLID);
    m.part([-183.0, 295.0, -27.0], [43.0, 151.0, 38.0], 3.0, WHITE, mat::SOLID);
    m.part([-201.0, 295.0, -23.0], [9.0, 153.0, 46.0], 2.0, BOARD, mat::SOLID);
    perforations(m, vec3(-173.0, 246.0, -7.7), 19, 2, 5.6);
    m.part([-84.0, 276.0, -43.0], [61.0, 78.0, 6.0], 2.5, SEAL, mat::SOLID);
    m.part([-84.0, 276.0, -38.9], [55.0, 69.0, 3.6], 2.0, SILVER, mat::SOLID);
    m.part([-84.0, 276.0, -36.7], [38.0, 47.0, 1.4], 1.0, [76, 79, 78], mat::SOLID);
    for i in 0..20 {
        let x = -100.0 + i as f32 * 1.65;
        stripe(m, vec3(x, 255.0, -35.9), vec3(x, 297.0, -35.9), Vec3::Z, 0.55, [155, 145, 111], mat::SOLID);
    }
    cylindrical_side(m, vec3(-52.0, 249.0, -35.3), vec3(-52.0, 311.0, -35.3), 1.5, SILVER);
    cylindrical_side(m, vec3(-52.0, 249.0, -35.3), vec3(-59.0, 245.0, -35.3), 1.5, SILVER);
    m.part([-84.0, 312.5, -36.2], [39.0, 5.5, 3.0], 1.0, ALUMINIUM, mat::SOLID);
    text(m, "NZXT", vec3(-125.0, 359.0, -23.2), 6.3, [147, 152, 156]);
    for y in [230.0, 316.0] {
        for x in [-122.0, -50.0] {
            annulus(m, vec3(x, y, -32.9), Vec3::Z, 3.3, 0.0, SEAL, mat::SOLID);
        }
    }
    for y in [97.0, 153.0, 198.0] {
        let width = if y > 190.0 { 182.0 } else { 161.0 };
        m.part([-116.0, y, -40.0], [width, 29.0, 12.0], 1.4, WHITE, mat::SOLID);
        m.part([-126.0, y + 18.5, -40.3], [86.0, 6.5, 12.0], 0.7, SEAL, mat::SOLID);
    }
    m.part([9.0, 159.0, -39.0], [50.0, 119.0, 14.0], 1.8, WHITE, mat::SOLID);
    perforations(m, vec3(-8.0, 154.0, -31.8), 10, 7, 5.1);
    text(m, "N7", vec3(3.0, 130.0, -31.7), 9.5, [101, 108, 111]);
    for i in 0..4 {
        let x = -9.5 + i as f32 * 7.65;
        m.part([x, 278.0, -41.5], [5.8, 141.0, 10.0], 0.7, SEAL, mat::SOLID);
        for y in [208.0, 348.0] {
            m.part([x, y, -36.5], [5.6, 5.4, 5.0], 0.55, BOARD, mat::SOLID);
        }
    }
    for x in [-140.0, -118.0] {
        m.part([x, 369.0, -33.0], [19.4, 14.0, 22.0], 1.1, SEAL, mat::SOLID);
        for row in 0..2 {
            for col in 0..4 {
                m.part(
                    [x - 6.6 + col as f32 * 4.4, 365.9 + row as f32 * 5.8, -21.6],
                    [2.7, 3.1, 0.4],
                    0.14,
                    ALUMINIUM,
                    mat::SOLID,
                );
            }
        }
    }
    for y in [88.0, 120.0, 150.0, 184.0] {
        m.part([-199.0, y, -37.0], [9.5, 12.0, 14.0], 1.5, SEAL, mat::SOLID);
    }
    m.part([30.0, 341.0, -42.0], [17.0, 68.0, 8.0], 1.2, WHITE, mat::SOLID);
    m.part([32.0, 280.0, -39.0], [9.5, 53.0, 17.0], 1.0, SEAL, mat::SOLID);
    for y in [83.0, 214.0, 371.0] {
        for x in [-195.0, 32.0] {
            annulus(m, vec3(x, y, -45.9), Vec3::Z, 3.0, 1.35, SILVER, mat::SOLID);
        }
    }
}

pub(super) fn trident_z5(m: &mut Mesh) {
    let mut dimm = Mesh::default();
    dimm.part([0.0, 16.6, 0.0], [132.0, 33.2, 1.5], 0.6, SEAL, mat::SOLID);
    for (cx, length) in [(-32.5, 61.5), (32.9, 63.7)] {
        dimm.part([cx, 2.0, 0.0], [length, 4.0, 1.7], 0.3, [176, 145, 82], mat::SOLID);
    }
    dimm.part([0.0, 20.0, 0.0], [133.0, 29.5, 7.5], 1.4, ALUMINIUM, mat::SOLID);
    let profile = |x: f32| 35.0 + 8.0 * ((x / 66.5 + 1.0) * 0.5).powf(1.7);
    for i in 0..24 {
        let a = -66.0 + 132.0 * i as f32 / 24.0;
        let b = a + 5.5;
        let (ha, hb) = (profile(a), profile(b));
        let led = gradient(ICE, LILAC, (i as f32 + 0.5) / 24.0);
        diffuser_segment(&mut dimm, a, b, ha, hb, led);
        for side in [-1.0, 1.0] {
            let z = side * 3.90;
            dimm.quad([vec3(a, 27.0, z), vec3(b, 27.0, z), vec3(b, hb - 1.2, z), vec3(a, ha - 1.2, z)], SILVER, mat::SOLID);
            let y = 19.0 + 0.043 * a;
            dimm.quad([vec3(a, y - 5.6, z), vec3(b, y - 5.36, z), vec3(b, y + 5.84, z), vec3(a, y + 5.6, z)], BOARD, mat::SOLID);
        }
    }
    for i in 0..31 {
        let x = 19.0 + i as f32 * 1.35;
        stripe(&mut dimm, vec3(x, 28.0, 3.96), vec3(x + 4.3, profile(x) - 1.7, 3.96), Vec3::Z, 0.25, ALUMINIUM, mat::SOLID);
    }
    for i in 0..127 {
        let x = -64.5 + i as f32 * 1.0;
        if (x - 0.4).abs() > 2.0 {
            stripe(&mut dimm, vec3(x, 0.40, 0.88), vec3(x, 3.65, 0.88), Vec3::Z, 0.14, BOARD, mat::SOLID);
        }
    }
    text(&mut dimm, "TRIDENT Z5", vec3(14.5, 17.8, 4.01), 4.8, PORCELAIN);
    let basis = Mat3::from_cols(Vec3::Y, Vec3::Z, Vec3::X);
    for x in [-1.85, 13.45] {
        placed(m, &dimm, vec3(x, 278.0, -45.0), basis);
    }
}

fn diffuser_segment(m: &mut Mesh, a: f32, b: f32, ha: f32, hb: f32, led: Color) {
    m.quad([vec3(a, ha, -2.6), vec3(a, ha, 2.6), vec3(b, hb, 2.6), vec3(b, hb, -2.6)], led, mat::RGB);
    for side in [-1.0, 1.0] {
        let z = side * 2.6;
        m.quad([vec3(a, ha - 3.5, z), vec3(b, hb - 3.5, z), vec3(b, hb, z), vec3(a, ha, z)], led, mat::RGB);
    }
}

pub(super) fn kraken_elite(m: &mut Mesh) {
    let roof = Mat3::from_cols(Vec3::Z, Vec3::NEG_X, Vec3::NEG_Y);
    radiator(m, vec3(-10.0, 385.0, 40.0), roof, vec3(121.0, 394.0, 27.0));
    let base = vec3(-84.0, 276.0, -36.0);
    let face = base + Vec3::Z * 59.5;
    cylindrical_side(m, base, base + Vec3::Z * 12.0, 38.2, ALUMINIUM);
    cylindrical_side(m, base + Vec3::Z * 10.5, face, 40.95, WHITE);
    annulus(m, face, Vec3::Z, 40.95, 0.0, PORCELAIN, mat::SOLID);
    annulus(m, face + Vec3::Z * 0.05, Vec3::Z, 37.8, 0.0, [24, 27, 35], mat::SOLID);
    let lcd = face + Vec3::Z * 0.15;
    annulus(m, lcd, Vec3::Z, 30.0, 0.0, [15, 23, 42], mat::SOLID);
    for i in 0..48 {
        let arc = |k: usize, r: f32| {
            let a = 2.42 + (k as f32 / 48.0) * 4.45;
            lcd + vec3(r * a.cos(), r * a.sin(), 0.12)
        };
        m.quad([arc(i, 25.0), arc(i, 27.2), arc(i + 1, 27.2), arc(i + 1, 25.0)], gradient(ICE, LILAC, i as f32 / 47.0), mat::RGB);
    }
    text(m, "NZXT", lcd + vec3(-14.4, -3.6, 0.3), 8.0, [235, 242, 249]);
    let first = m.vertices.len();
    let screen = lcd + Vec3::Z * 0.55;
    annulus(m, screen, Vec3::Z, 30.0, 0.0, WHITE, 29.0);
    for v in &mut m.vertices[first..] {
        v.uv = [0.5 + (v.point[0] - screen.x) / 60.0, 0.5 - (v.point[1] - screen.y) / 60.0];
    }
    for i in 0..2 {
        let d = i as f32;
        let start = base + vec3(39.0, 8.0 - 15.0 * d, 25.0);
        let socket = start + Vec3::X * 12.0;
        cylindrical_side(m, start, socket, 6.2, PORCELAIN);
        let end = vec3(178.0, 385.0, 18.0 + 20.0 * d);
        hose(m, [socket, socket + vec3(72.0, -8.0 + 11.0 * d, 52.0), end + vec3(-84.0, -60.0 + 8.0 * d, 38.0), end], 4.8, WHITE);
        cylindrical_side(m, end, end + Vec3::Y * 9.0, 6.4, ALUMINIUM);
    }
}

pub(super) fn neptune(m: &mut Mesh) {
    radiator(m, vec3(141.0, 220.0, -17.0), Mat3::IDENTITY, vec3(119.2, 277.0, 29.4));
    // On the riser the cold-plate cover faces the glass and the PCIe contacts remain below it.
    let front = 82.0;
    m.part([-85.8, 128.0, 43.0], [243.0, 119.0, 4.7], 2.2, ALUMINIUM, mat::SOLID);
    m.part([-85.8, 128.5, 49.0], [242.0, 120.0, 7.5], 2.2, BOARD, mat::SOLID);
    m.part([-85.8, 129.0, 64.0], [246.0, 124.0, 23.0], 4.8, SILVER, mat::SOLID);
    m.part([-85.8, 129.0, front - 3.8], [247.0, 124.5, 7.6], 3.2, [210, 213, 222], mat::SOLID);
    m.part([-213.0, 128.0, 61.0], [2.6, 153.0, 41.5], 1.3, SILVER, mat::SOLID);
    m.part([-212.0, 203.6, 61.0], [7.8, 2.2, 41.5], 0.65, BOARD, mat::SOLID);
    for i in 0..4 {
        m.part([-214.5, 89.0 + i as f32 * 24.0, 67.0], [0.9, 15.5, 7.5], 0.38, SEAL, mat::SOLID);
    }
    m.part([-104.0, 64.0, 46.0], [93.0, 9.0, 2.4], 0.7, [174, 139, 78], mat::SOLID);
    m.part([-104.0, 59.5, 46.0], [105.0, 7.0, 12.0], 1.7, BOARD, mat::SOLID);
    for row in 0..2 {
        let path = |t: f32| {
            let ease = t * t * (3.0 - 2.0 * t);
            vec3(-207.3 + 243.0 * t, 186.2 - row as f32 * 17.0 - 35.0 * ease, front + 0.14)
        };
        for i in 0..36 {
            let (a, b) = (path(i as f32 / 36.0), path((i + 1) as f32 / 36.0));
            stripe(m, a, b, Vec3::Z, 3.6, [119, 139, 157], mat::SOLID);
            stripe(
                m,
                a + Vec3::Z * 0.12,
                b + Vec3::Z * 0.12,
                Vec3::Z,
                1.65,
                gradient([67, 215, 226], [188, 138, 238], i as f32 / 35.0),
                mat::GPU_STRIP,
            );
        }
    }
    let line = |m: &mut Mesh, points: &[(f32, f32)]| {
        for q in points.windows(2) {
            stripe(
                m,
                vec3(q[0].0, q[0].1, front + 0.07),
                vec3(q[1].0, q[1].1, front + 0.07),
                Vec3::Z,
                1.35,
                [225, 227, 231],
                mat::SOLID,
            );
        }
    };
    line(m, &[(-190.0, 75.0), (-199.0, 104.0), (-171.0, 161.0)]);
    line(m, &[(-130.0, 70.5), (-105.0, 76.0), (-102.0, 113.0), (-113.0, 126.0), (-47.0, 149.0)]);
    line(m, &[(-30.0, 71.2), (-51.0, 84.0), (-35.0, 115.0), (-45.0, 121.0), (-32.0, 139.0)]);
    let badge = vec3(-163.0, 103.0, front + 0.25);
    for (triangle, tone) in [
        ([(-15.0, -9.0), (-11.0, 12.0), (13.0, 19.0)], BOARD),
        ([(-15.0, -9.0), (13.0, 19.0), (-6.0, 7.0)], SILVER),
        ([(-14.0, -12.0), (-6.0, 7.0), (-5.0, -6.0)], ALUMINIUM),
        ([(-14.0, -12.0), (8.0, 0.0), (0.0, -10.0)], SEAL),
        ([(0.0, 1.0), (15.0, 10.0), (9.0, -1.0)], BOARD),
    ] {
        m.face(triangle.map(|(x, y)| badge + vec3(x, y, 0.0)), tone, mat::SOLID);
    }
    text(m, "NEPTUNE", vec3(-4.0, 178.0, front + 0.20), 5.3, [100, 109, 125]);
    for x in [-192.0, 23.0] {
        for y in [78.0, 180.0] {
            annulus(m, vec3(x, y, 40.5), Vec3::NEG_Z, 2.7, 0.0, BOARD, mat::SOLID);
        }
    }
    for i in 0..2 {
        let d = i as f32;
        let connector = vec3(-112.0 + 25.0 * d, 191.5, 63.0);
        let start = connector + Vec3::Y * 12.5;
        cylindrical_side(m, connector, start, 7.1, SILVER);
        let mid = vec3(28.0 + 5.0 * d, 233.0 + 10.0 * d, 75.0 + 8.0 * d);
        let end = vec3(128.0 + 23.0 * d, 344.0, -15.0);
        hose(m, [start, start + vec3(0.0, 61.0, 31.0), mid - vec3(61.0, 0.0, -8.0), mid], 5.6, PORCELAIN);
        hose(m, [mid, mid + vec3(61.0, 0.0, -8.0), end + vec3(0.0, -17.0, 66.0), end], 5.6, PORCELAIN);
        cylindrical_side(m, end - Vec3::Z * 4.3, end + Vec3::Z * 5.4, 7.1, SILVER);
    }
    for i in 0..5 {
        let start = vec3(-21.0 + i as f32 * 2.75, 190.0, 47.0);
        hose(
            m,
            [start, start + vec3(1.0, 25.0, 2.0), vec3(56.0, 211.0, 1.0), vec3(68.0, 212.0 + i as f32 * 2.5, -48.0)],
            1.45,
            SEAL,
        );
    }
}
