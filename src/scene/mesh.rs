//! 작업실의 책상와 기기 모형은 둥근 상자(`slab`)를 쌓아 짓는다. 길이 1 은 12.7cm 쯤이다(맥미니 19.7cm 가 1.55).
//! 방은 배치(`Plan`)가 바뀔 때만 다시 짓고, 기기 모형은 종류마다 한 벌을 두고 자리(인스턴스)만 바꾼다.

pub(super) mod y60;

use std::f32::consts::{FRAC_PI_2, TAU};

use bytemuck::{Pod, Zeroable};
use glam::{ Vec2, Vec3, vec2, vec3};

/// 재질 — 셰이더 `shade` 의 갈래와 같은 번호다.
mod mat {
    pub const SOLID: f32 = 0.0;
    pub const DECK: f32 = 2.0;
    pub const PAPER_TAIL: f32 = 3.0;
    pub const CHART: f32 = 4.0;
    pub const CONTACT: f32 = 6.0;
    pub const LAMP: f32 = 9.0;
    pub const STRIP: f32 = 12.0;
    pub const WOOD: f32 = 13.0;
    pub const FABRIC: f32 = 14.0;
    pub const GLASS: f32 = 15.0;
    pub const RGB: f32 = 16.0;
    pub const DIFFUSER: f32 = 31.0;
}

const GRAPHITE: [u8; 3] = [61, 74, 84];
const RECESS: [u8; 3] = [26, 36, 43];

pub const DESK_Y: f32 = 5.8;
const MINI_SLOT: f32 = 7.8;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct Vertex {
    pub point: [f32; 3],
    pub normal: [f32; 3],
    pub material: [f32; 4],
    pub uv: [f32; 2],
}

/// 기기 모형 — 맥북·맥미니·모니터(Windows 책상 위)·타워(Windows 본체).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Model {
    Laptop,
    Air,
    Pro,
    Mini,
    MiniM4,
    Monitor,
    Tower,
}

pub const MODELS: [Model; 7] = [Model::Laptop, Model::Air, Model::Pro, Model::Mini, Model::MiniM4, Model::Monitor, Model::Tower];

const LAPTOP_TILT: f32 = 0.22;

impl Model {
    pub fn key(self) -> &'static str {
        match self {
            Self::Laptop => "laptop", Self::Air => "air", Self::Pro => "pro", Self::Mini => "mini",
            Self::MiniM4 => "mini-m4", Self::Monitor => "display", Self::Tower => "case",
        }
    }

    pub fn parse(key: &str) -> Option<Self> {
        MODELS.into_iter().find(|m| m.key() == key)
    }

    /// 누름 판정·테두리 빛에 쓰는 겉 상자(모형 원점 기준 가운데와 반 크기).
    pub fn hull(self) -> (Vec3, Vec3) {
        match self {
            Model::Laptop | Model::Air | Model::Pro => (vec3(0.0, 0.88, -0.17), vec3(1.25, 0.9, 1.05)),
            Model::MiniM4 => (vec3(0.0, 0.20, 0.0), vec3(0.51, 0.21, 0.51)),
            Model::Mini => (vec3(0.0, 0.2, 0.0), vec3(0.8, 0.22, 0.8)),
            Model::Monitor => (vec3(0.0, 2.0, -0.3), vec3(3.52, 2.0, 0.55)),
            Model::Tower => (vec3(0.0, 1.8, 0.0), vec3(1.62, 1.8, 0.88)),
        }
    }

    /// 누르면 카메라가 바로 보며 다가갈 자리 — (가운데, 화면에 담을 너비·높이, 바라보는 방위각, 내려다보는 각).
    pub fn face(self) -> (Vec3, Vec2, f32, f32) {
        match self {
            Model::Laptop | Model::Air | Model::Pro => (vec3(0.0, 0.98, -0.98), vec2(2.3, 1.45), 0.0, LAPTOP_TILT),
            Model::Mini | Model::MiniM4 => (vec3(0.0, 0.2, 0.32), vec2(2.4, 1.5), 0.0, 0.42),
            Model::Monitor => (vec3(0.0, 2.58, -0.4), vec2(6.7, 2.86), 0.0, 0.06),
            Model::Tower => (vec3(0.0, 1.8, 1.72), vec2(1.8, 3.6), 0.0, 0.2),
        }
    }


}

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Plan {
    pub desk: (f32, f32),
    pub minis: usize,
}

impl Plan {
    pub fn new(desk_len: f32, minis: usize) -> Self {
        Self { desk: (-desk_len * 0.5, desk_len * 0.5), minis }
    }

    pub fn desk_start(&self) -> f32 {
        self.desk.0 + 1.3
    }

    pub fn rack_width(&self) -> f32 {
        self.minis as f32 * MINI_SLOT
    }

    pub fn mini_x(&self, i: usize) -> f32 {
        self.desk.0 - self.rack_width() + (i as f32 + 0.5) * MINI_SLOT
    }

    pub fn limits(&self) -> (Vec3, Vec3) {
        let left = self.desk.0 - self.rack_width();
        let span = self.desk.1 - left;
        (vec3(left - 9.0, 0.0, -9.0), vec3(self.desk.1 + 9.0, 19.0 + span * 0.5, 34.0 + span * 1.4))
    }

    pub fn side_window_end(&self) -> f32 {
        self.limits().0.z + 17.0
    }

}

#[derive(Default)]
struct Mesh {
    vertices: Vec<Vertex>,
}

impl Mesh {
    fn push(&mut self, p: Vec3, n: Vec3, color: [u8; 3], mode: f32) {
        self.push_uv(p, n, color, mode, [0.0; 2]);
    }

    fn push_uv(&mut self, p: Vec3, n: Vec3, color: [u8; 3], mode: f32, uv: [f32; 2]) {
        self.vertices.push(Vertex {
            point: p.to_array(),
            normal: n.to_array(),
            uv,
            material: [f32::from(color[0]) / 255.0, f32::from(color[1]) / 255.0, f32::from(color[2]) / 255.0, mode],
        });
    }

    fn face(&mut self, [a, b, c]: [Vec3; 3], color: [u8; 3], mode: f32) {
        let normal = (b - a).cross(c - a).normalize_or_zero();
        for p in [a, b, c] {
            self.push(p, normal, color, mode);
        }
    }

    fn quad(&mut self, [a, b, c, d]: [Vec3; 4], color: [u8; 3], mode: f32) {
        self.face([a, b, c], color, mode);
        self.face([a, c, d], color, mode);
    }

    /// 둥근 상자 — 위아래 모서리를 조금 깎은 둥근 사각 기둥.
    fn slab(&mut self, center: Vec3, size: Vec3, corner: f32, color: [u8; 3], mode: f32) {
        const STEPS: usize = 3;
        const RING: usize = STEPS * 4;
        let half = size * 0.5;
        let corner = corner.min(half.x.min(half.z) * 0.98).max(0.001);
        let bevel = (corner * 0.4).min(half.y * 0.5);
        let ring = |height: f32, inset: f32| -> [Vec3; RING] {
            std::array::from_fn(|i| {
                let quadrant = i / STEPS;
                let a = (quadrant as f32 + (i % STEPS) as f32 / (STEPS - 1) as f32) * FRAC_PI_2;
                let (sin, cos) = a.sin_cos();
                let (sx, sz) = [(1.0, 1.0), (-1.0, 1.0), (-1.0, -1.0), (1.0, -1.0)][quadrant];
                center + vec3(sx * (half.x - corner) + cos * (corner - inset), height, sz * (half.z - corner) + sin * (corner - inset))
            })
        };
        let rings = [ring(-half.y, bevel), ring(-half.y + bevel, 0.0), ring(half.y - bevel, 0.0), ring(half.y, bevel)];
        for i in 0..RING {
            let j = (i + 1) % RING;
            self.face([center - Vec3::Y * half.y, rings[0][i], rings[0][j]], color, mode);
            self.face([center + Vec3::Y * half.y, rings[3][j], rings[3][i]], color, mode);
            for pair in rings.windows(2) {
                let [lower, upper] = [&pair[0], &pair[1]];
                self.quad([lower[i], upper[i], upper[j], lower[j]], color, mode);
            }
        }
    }

    fn part(&mut self, center: [f32; 3], size: [f32; 3], corner: f32, color: [u8; 3], mode: f32) {
        self.slab(Vec3::from_array(center), Vec3::from_array(size), corner, color, mode);
    }

    /// 접지 그림자와 선택 조명이 같은 자리를 쓰도록 반경 좌표를 함께 싣는다.
    fn halo(&mut self, center: Vec3, half: Vec3) {
        let (x, z) = (half.x * 1.5 + 0.4, half.z * 1.5 + 0.4);
        let y = center.y - half.y + 0.03;
        let corner = |u: f32, v: f32| (vec3(center.x + u * x, y, center.z + v * z), vec3(u, 0.0, v));
        for (p, uv) in [corner(-1.0, -1.0), corner(-1.0, 1.0), corner(1.0, 1.0), corner(-1.0, -1.0), corner(1.0, 1.0), corner(1.0, -1.0)] {
            self.vertices.push(Vertex { point: p.to_array(), normal: uv.to_array(), material: [1.0, 0.82, 0.58, mat::CONTACT], uv: [0.0; 2] });
        }
    }
}

fn axes(n: Vec3) -> (Vec3, Vec3) {
    let u = if n.y.abs() < 0.90 { Vec3::Y.cross(n).normalize() } else { Vec3::X };
    (u, n.cross(u).normalize())
}

fn cylindrical_side(m: &mut Mesh, start: Vec3, end: Vec3, radius: f32, color: [u8; 3]) {
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

pub fn room_vertices(plan: &Plan) -> Vec<Vertex> {
    let mut m = Mesh::default();
    room(&mut m, plan);
    m.vertices
}

pub fn contact(center: Vec3, half: Vec3) -> Vec<Vertex> {
    let mut m = Mesh::default();
    m.halo(center, half);
    m.vertices
}

const BIRCH: [u8; 3] = [215, 174, 126];
const ENAMEL: [u8; 3] = [62, 114, 159];
const INDIGO: [u8; 3] = [62, 84, 125];
const CERAMIC: [u8; 3] = [229, 227, 205];

fn planter(m: &mut Mesh, x: f32, y: f32, z: f32, size: f32) {
    let at = vec3(x, y, z);
    m.slab(at + Vec3::Y * (0.57 * size), vec3(1.28, 1.14, 1.28) * size, size * 0.44, CERAMIC, mat::SOLID);
    m.slab(at + Vec3::Y * (1.14 * size), vec3(1.03, 0.04, 1.03) * size, size * 0.33, RECESS, mat::SOLID);
    for i in 0..15 {
        let a = i as f32 * 2.39996;
        let spread = 0.65 + (i % 4) as f32 * 0.25;
        let height = 1.45 + (i % 5) as f32 * 0.36;
        let color = [[55, 109, 78], [83, 137, 90], [100, 154, 99], [67, 127, 88]][i % 4];
        let dir = vec3(a.cos(), 0.0, a.sin());
        let side = vec3(-dir.z, 0.0, dir.x);
        let row = |j: usize| {
            let t = j as f32 / 8.0;
            let bend = (std::f32::consts::PI * t).sin();
            let center = at + (dir * (0.16 + spread * t * t) + Vec3::Y * (1.12 + height * t + bend * 0.70)) * size;
            let wing = side * (0.28 * bend * size);
            [center - wing, center + Vec3::Y * (0.10 * bend * size), center + wing]
        };
        for j in 0..8 {
            let b = row(j);
            let t = row(j + 1);
            for k in 0..2 { m.quad([b[k], t[k], t[k+1], b[k+1]], color, mat::SOLID); }
        }
    }
}

fn pendant(m: &mut Mesh, x: f32, z: f32, height: f32, roof: f32, radius: f32) {
    let top = vec3(x, roof, z);
    cylindrical_side(m, vec3(x, height, z), top, 0.035, GRAPHITE);
    let at = |angle: f32, r: f32, dy: f32| vec3(x + r * angle.cos(), height + dy, z + r * angle.sin());
    for i in 0..40 {
        let a = TAU * i as f32 / 40.0;
        let b = TAU * (i + 1) as f32 / 40.0;
        for ((r0, h0), (r1, h1), color) in [
            ((0.16, 0.90), (0.33, 0.69), GRAPHITE),
            ((0.33, 0.69), (0.61, 0.37), ENAMEL),
            ((0.61, 0.37), (1.0, 0.0), ENAMEL),
            ((1.0, 0.0), (1.0, -0.055), CERAMIC),
        ] {
            m.quad(
                [at(a, r1 * radius, h1), at(a, r0 * radius, h0), at(b, r0 * radius, h0), at(b, r1 * radius, h1)],
                color,
                mat::SOLID,
            );
        }
        m.face([at(a, radius, -0.055), at(b, radius, -0.055), at(a, 0.0, 0.14)], CERAMIC, mat::LAMP);
    }
}

fn parasol(m: &mut Mesh, x: f32, z: f32, height: f32, radius: f32) {
    let crown = vec3(x, height, z);
    let pole = vec3(x, 0.20, z);
    cylindrical_side(m, pole, crown + Vec3::Y * 0.13, 0.075, BIRCH);
    m.slab(pole, vec3(2.0, 0.40, 2.0), 0.70, CERAMIC, mat::SOLID);
    let rim = |angle: f32, fraction: f32, drop: f32| {
        let (sin, cos) = angle.sin_cos();
        crown + vec3(cos * fraction, -drop - 0.12 * fraction * cos, sin * fraction) * radius
    };
    for i in 0..8 {
        let a = TAU * i as f32 / 8.0;
        let b = a + TAU / 8.0;
        let cloth = if i % 2 == 0 { [240, 147, 83] } else { [251, 185, 113] };
        let mid = (a + b) * 0.5;
        let shoulder = rim(mid, 0.61, 0.085);
        let edge = rim(mid, 0.92, 0.27);
        let left = rim(a, 1.0, 0.34);
        let right = rim(b, 1.0, 0.34);
        let l_rib = rim(a, 0.64, 0.14);
        let r_rib = rim(b, 0.64, 0.14);
        m.face([crown, shoulder, l_rib], cloth, mat::FABRIC);
        m.face([crown, r_rib, shoulder], cloth, mat::FABRIC);
        m.quad([shoulder, edge, left, l_rib], cloth, mat::FABRIC);
        m.quad([r_rib, right, edge, shoulder], cloth, mat::FABRIC);
        cylindrical_side(m, crown, rim(a, 0.64, 0.14), 0.022, BIRCH);
        cylindrical_side(m, rim(a, 0.64, 0.14), left, 0.022, BIRCH);
        cylindrical_side(m, crown - Vec3::Y * 1.15, rim(a, 0.58, 0.14), 0.036, BIRCH);
    }
}

fn wind_bell(m: &mut Mesh, x: f32, y: f32, z: f32, roof: f32) {
    let at = vec3(x, y, z);
    cylindrical_side(m, at, vec3(x, roof, z), 0.024, GRAPHITE);
    let profile = [(0.06, 0.50), (0.31, 0.45), (0.53, 0.29), (0.63, 0.00), (0.62, -0.15), (0.54, -0.27), (0.49, -0.30)];
    for i in 0..32 {
        let edge = |j: usize, (r, h): (f32, f32)| {
            let a = TAU * j as f32 / 32.0;
            at + vec3(r * a.cos(), h, r * a.sin())
        };
        for (band, rings) in profile.windows(2).enumerate() {
            let color = match band {
                5 => ENAMEL,
                3 if i % 8 < 3 => [98, 158, 195],
                _ => CERAMIC,
            };
            m.quad([edge(i, rings[1]), edge(i, rings[0]), edge(i + 1, rings[0]), edge(i + 1, rings[1])], color, mat::SOLID);
        }
    }
    cylindrical_side(m, at, at - Vec3::Y * 0.69, 0.029, [111, 75, 67]);
    let root = at - Vec3::Y * 0.67;
    for i in 0..8 {
        let top = i as f32 / 8.0;
        let bottom = (i + 1) as f32 / 8.0;
        for (u, t) in [(0.0, top), (0.0, bottom), (1.0, bottom), (0.0, top), (1.0, bottom), (1.0, top)] {
            let point = root + vec3((u - 0.5) * 0.39 - 0.18 * t * t, -1.50 * t, t * t * 0.20);
            m.push_uv(point, Vec3::Z, CERAMIC, mat::PAPER_TAIL, [u, t]);
        }
    }
}

fn field_book(m: &mut Mesh, x: f32) {
    let z = 2.37;
    m.part([x, DESK_Y + 0.070, z], [1.63, 0.13, 1.06], 0.09, [132, 68, 69], mat::SOLID);
    m.part([x, DESK_Y + 0.143, z], [1.56, 0.065, 1.01], 0.05, CERAMIC, mat::WOOD);
    for (u, v) in [(0.0, 0.0), (0.0, 1.0), (1.0, 1.0), (0.0, 0.0), (1.0, 1.0), (1.0, 0.0)] {
        m.push_uv(vec3(x + (u - 0.5) * 1.52, DESK_Y + 0.178, z + (v - 0.5) * 0.96), Vec3::Y, CERAMIC, mat::CHART, [u, v]);
    }
    cylindrical_side(
        m,
        vec3(x + 0.23, DESK_Y + 0.062, z + 0.65),
        vec3(x + 1.32, DESK_Y + 0.062, z + 0.52),
        0.035,
        [206, 123, 59],
    );
}

fn hydrangeas(m: &mut Mesh, x: f32, z: f32, size: f32) {
    planter(m, x, 0.0, z, size);
    let palette = [[122, 149, 227], [155, 158, 224], [167, 145, 208], [124, 177, 220]];
    for i in 0..6 {
        let yaw = i as f32 * 2.4;
        let bloom = vec3(x, 0.0, z) + vec3(yaw.cos() * 1.03, 3.20 + yaw.sin() * 0.48, yaw.sin() * 0.66) * size;
        cylindrical_side(m, bloom - Vec3::Y * 1.25 * size, bloom, 0.055 * size, [73, 109, 92]);
        for j in 0..26 {
            let h = (j as f32 + 0.5) / 26.0;
            let a = j as f32 * 2.40;
            let r = (1.0 - h * h).sqrt();
            let n = vec3(r * a.cos(), h, r * a.sin());
            let (u, v) = axes(n);
            let stem = bloom + n * 0.49 * size;
            for k in 0..4 {
                let t = k as f32 * FRAC_PI_2;
                let d = u * t.cos() + v * t.sin();
                let side = n.cross(d) * size * 0.073;
                let tip = stem + (d * 0.17 + n * 0.047) * size;
                m.quad([stem, tip - side, tip + d * 0.032 * size, tip + side], palette[(i + j / 8) % 4], mat::SOLID);
            }
        }
    }
}

fn desk(m: &mut Mesh, left: f32, right: f32) {
    let x = (left + right) * 0.5;
    let w = right - left;
    for i in 0..5 {
        let z = (i as f32 - 2.0) * 1.34;
        m.part([x, DESK_Y - 0.16, z], [w, 0.32, 1.334], 0.12, BIRCH, mat::WOOD);
    }
    m.part([x, DESK_Y - 0.42, 0.03], [w - 0.40, 0.20, 6.30], 0.24, ENAMEL, mat::SOLID);
    m.part([x, DESK_Y + 0.012, 0.20], [w - 0.8, 0.024, 4.2], 0.38, INDIGO, mat::FABRIC);
    let legs = (w / 10.0).ceil().max(2.0) as usize;
    for i in 0..legs {
        let lx = left + 1.0 + (w - 2.0) * i as f32 / (legs - 1) as f32;
        for side in [-1.0, 1.0] {
            let foot = vec3(lx, 0.13, side * 2.65);
            cylindrical_side(m, foot, vec3(lx, DESK_Y - 0.40, side * 1.44), 0.16, ENAMEL);
            m.slab(foot, vec3(0.59, 0.23, 0.68), 0.10, GRAPHITE, mat::SOLID);
            cylindrical_side(m, vec3(lx, 1.2, side * 2.36), vec3(lx, DESK_Y - 1.2, -side * 1.56), 0.055, ENAMEL);
        }
        m.part([lx, DESK_Y - 0.56, 0.0], [0.46, 0.28, 5.25], 0.09, ENAMEL, mat::SOLID);
    }
    for dz in [1.97, 2.05] {
        m.part([x, DESK_Y + 0.027, dz], [w - 1.4, 0.009, 0.035], 0.01, [139, 171, 188], mat::FABRIC);
    }
    m.part([x, DESK_Y + 0.16, -2.75], [w - 0.8, 0.32, 0.32], 0.08, ENAMEL, mat::SOLID);
    m.part([x, DESK_Y + 0.22, -2.57], [w - 1.18, 0.065, 0.04], 0.02, CERAMIC, mat::STRIP);
}

fn room(m: &mut Mesh, p: &Plan) {
    let (lo, hi) = p.limits();
    let (cx, cz) = ((lo.x+hi.x)*0.5, (lo.z+hi.z)*0.5);
    let (w,d) = (hi.x-lo.x, hi.z-lo.z);
    m.part([cx,-0.14,cz], [w,0.28,d], 0.08, [145,165,185], mat::DECK);
    let side_end = p.side_window_end();
    let wall_depth = hi.z - side_end;
    let wall_z = side_end + wall_depth * 0.5;
    for x in [lo.x,hi.x] {
        m.part([x,hi.y*0.5,wall_z], [0.35,hi.y,wall_depth], 0.1, [159,178,196], mat::SOLID);
        m.part([x,4.0,wall_z], [0.48,7.8,wall_depth], 0.12, GRAPHITE, mat::SOLID);
        for i in 0..6 {
            let z = side_end + (i as f32+0.5)*wall_depth/6.0;
            m.part([x,hi.y*0.5,z], [0.9,hi.y,0.42], 0.08, BIRCH, mat::WOOD);
            m.part([x,8.2,z], [1.0,0.15,2.8], 0.06, CERAMIC, mat::LAMP);
        }
        let window_depth = side_end - lo.z;
        let side_mid = lo.z + window_depth * 0.5;
        m.part([x,1.45,side_mid], [0.65,2.9,window_depth], 0.10, GRAPHITE, mat::SOLID);
        m.part([x,2.98,side_mid], [0.92,0.17,window_depth], 0.07, BIRCH, mat::WOOD);
        for z in [lo.z, side_mid, side_end] {
            m.part([x,hi.y*0.5,z], [0.48,hi.y,0.23], 0.05, GRAPHITE, mat::SOLID);
        }
    }
    m.part([cx,hi.y*0.5,hi.z], [w,hi.y,0.40], 0.15, [126,153,180], mat::SOLID);
    m.part([cx,hi.y,cz], [w,0.35,d], 0.1, [184,201,212], mat::SOLID);
    m.part([cx,1.45,lo.z], [w,2.9,0.75], 0.15, GRAPHITE, mat::SOLID);
    m.part([cx,2.98,lo.z], [w,0.17,1.1], 0.07, BIRCH, mat::WOOD);
    for x in [lo.x+1.0,lo.x+w*0.32,hi.x-1.0] {
        m.part([x,hi.y*0.5,lo.z], [0.20,hi.y,0.52], 0.05, GRAPHITE, mat::SOLID);
    }
    let (left,right) = p.desk;
    desk(m, left+0.18,right+0.45);
    field_book(m, left+1.62);
    for i in 0..p.minis {
        let x = p.mini_x(i);
        desk(m, x - MINI_SLOT*0.5+0.12, x+MINI_SLOT*0.5-0.12);
        m.part([x+2.45,DESK_Y+0.15,1.0], [1.8,0.30,1.75], 0.15, BIRCH, mat::WOOD);
    }
    m.part([cx,0.038,6.8], [w-8.0,0.074,6.8], 0.50, [86,120,155], mat::FABRIC);
    for z in [3.72,9.88] {
        m.part([cx,0.080,z], [w-8.65,0.012,0.065], 0.02, CERAMIC, mat::SOLID);
    }
    let bay_left = left-p.rack_width();
    parasol(m,bay_left-2.6,-4.5,8.9,3.1);
    planter(m,bay_left-2.9,0.0,0.15,1.5);
    hydrangeas(m,right+2.35,-3.0,2.1);
    pendant(m,bay_left+2.0,-4.5,13.7,hi.y,1.62);
    pendant(m,right+0.2,-5.4,14.8,hi.y,1.40);
    wind_bell(m,bay_left+(right-bay_left)*0.59,15.0,lo.z+0.8,hi.y);
    for side in [-1.0,1.0] {
        let x = cx + side*(w*0.5-3.0);
        m.part([x,1.15,cz], [3.4,2.3,9.4], 0.50, BIRCH, mat::WOOD);
        for k in [-1.0,0.0,1.0] {
            m.part([x,2.60,cz+k*3.1], [3.25,0.60,3.0], 0.34, INDIGO, mat::FABRIC);
        }
        planter(m,x,0.0,cz-6.8,1.9);
    }
    // 벽 쪽을 돌아봤을 때에도 서가와 소파 사이로 걸어갈 통로를 둔다.
    let library_width = w.min(44.0) * 0.62;
    let shelf_z = hi.z - 1.8;
    for level in 0..4 {
        let y = 2.0 + level as f32 * 2.45;
        m.part([cx,y,shelf_z], [library_width,0.24,2.45], 0.13, BIRCH, mat::WOOD);
        m.part([cx,y-0.14,shelf_z-0.9], [library_width-0.35,0.06,0.08], 0.03, CERAMIC, mat::LAMP);
        for side in [-1.0,1.0] {
            let edge = cx + side * library_width*0.5;
            m.part([edge,y-1.17,shelf_z], [0.19,2.35,2.25],0.08,ENAMEL,mat::SOLID);
        }
        if level < 3 {
            for j in 0..12 {
                let x = cx - library_width*0.43 + (j/4) as f32 * library_width*0.30 + (j%4) as f32 * 0.40;
                let book_h = 1.12 + ((j+level*2)%4) as f32 * 0.18;
                let cover = [INDIGO, [149,84,65], [172,151,104], [88,112,127]][(j+level)%4];
                m.part([x,y+0.12+book_h*0.5,shelf_z-0.24], [0.33,book_h,1.15],0.035,cover,mat::FABRIC);
                m.part([x,y+0.48,shelf_z-0.83],[0.25,0.06,0.04],0.015,ENAMEL,mat::SOLID);
            }
        }
    }
    planter(m, cx+library_width*0.31,9.47,shelf_z,1.05);
    for side in [-1.0,1.0] {
        let x = cx + side*(w*0.5-7.6);
        m.part([x,1.45,cz], [2.7,0.20,3.4], 0.35, BIRCH, mat::WOOD);
        m.part([x,0.72,cz], [0.52,1.44,0.52],0.2,ENAMEL,mat::SOLID);
        m.part([x,0.10,cz], [1.8,0.2,2.0],0.35,GRAPHITE,mat::SOLID);
        m.part([x-0.18,1.64,cz-0.20],[1.35,0.18,1.8],0.05,CERAMIC,mat::SOLID);
    }
}
