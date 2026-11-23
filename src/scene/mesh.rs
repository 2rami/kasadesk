//! 관측실의 조작대와 기기 모형은 둥근 상자(`slab`)를 쌓아 짓는다. 길이 1 은 12.7cm 쯤이다(맥미니 19.7cm 가 1.55).
//! 방은 배치(`Plan`)가 바뀔 때만 다시 짓고, 기기 모형은 종류마다 한 벌을 두고 자리(인스턴스)만 바꾼다.

use std::f32::consts::FRAC_PI_2;

use bytemuck::{Pod, Zeroable};
use glam::{Quat, Vec2, Vec3, vec2, vec3};

/// 재질 — 셰이더 `shade` 의 갈래와 같은 번호다.
mod mat {
    pub const SOLID: f32 = 0.0;
    pub const DECK: f32 = 2.0;
    pub const CONTACT: f32 = 6.0;
    pub const LAMP: f32 = 9.0;
    pub const STRIP: f32 = 12.0;
}

const TITANIUM: [u8; 3] = [140, 154, 161];
const GRAPHITE: [u8; 3] = [61, 74, 84];
const RECESS: [u8; 3] = [26, 36, 43];
const COPPER: [u8; 3] = [218, 119, 59];

pub const DESK_Y: f32 = 5.8;
const SHELF_SLOTS: usize = 3;
const SHELF_PITCH: f32 = 2.25;

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
        self.minis.min(SHELF_SLOTS) as f32 * SHELF_PITCH
    }

    pub fn shelf_slot(&self, i: usize) -> Vec3 {
        let col = i % SHELF_SLOTS;
        let row = i / SHELF_SLOTS;
        vec3(self.desk.0 - self.rack_width() + (col as f32 + 0.5) * SHELF_PITCH,
             DESK_Y + 0.72 + row as f32 * 1.35, -0.45 - row as f32 * 0.35)
    }
}

#[derive(Default)]
struct Mesh {
    vertices: Vec<Vertex>,
    rotation: Quat,
    offset: Vec3,
}

impl Mesh {
    fn push(&mut self, p: Vec3, n: Vec3, color: [u8; 3], mode: f32) {
        self.vertices.push(Vertex {
            point: (self.offset + self.rotation * p).to_array(),
            normal: (self.rotation * n).to_array(),
            uv: [0.0; 2],
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

    fn turned(&mut self, offset: Vec3, rotation: Quat, draw: impl FnOnce(&mut Self)) {
        let saved = (self.rotation, self.offset);
        self.offset = saved.1 + saved.0 * offset;
        self.rotation = saved.0 * rotation;
        draw(self);
        (self.rotation, self.offset) = saved;
    }

    /// 접지 그림자와 선택 조명이 같은 자리를 쓰도록 반경 좌표를 함께 싣는다.
    fn halo(&mut self, center: Vec3, half: Vec3) {
        let (x, z) = (half.x * 1.5 + 0.4, half.z * 1.5 + 0.4);
        let y = center.y - half.y + 0.03;
        let corner = |u: f32, v: f32| (vec3(center.x + u * x, y, center.z + v * z), vec3(u, 0.0, v));
        for (p, uv) in [corner(-1.0, -1.0), corner(-1.0, 1.0), corner(1.0, 1.0), corner(-1.0, -1.0), corner(1.0, 1.0), corner(1.0, -1.0)] {
            self.vertices.push(Vertex { point: (self.rotation * p).to_array(), normal: uv.to_array(), material: [1.0, 0.82, 0.58, mat::CONTACT], uv: [0.0; 2] });
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

fn room(m: &mut Mesh, p: &Plan) {
    // 양옆으로 이어진 갑판이 가까운 시점과 넓은 창에서도 바닥 끝을 가린다.
    m.part([0.0, DESK_Y - 0.35, 0.2], [600.0, 0.70, 6.4], 0.20, TITANIUM, mat::SOLID);
    m.part([0.0, DESK_Y - 0.84, 0.05], [600.0, 0.34, 6.5], 0.14, RECESS, mat::SOLID);
    m.part([0.0, DESK_Y - 1.06, 0.08], [600.0, 0.13, 6.5], 0.06, GRAPHITE, mat::SOLID);
    m.part([0.0, DESK_Y - 3.65, -0.2], [600.0, 5.4, 6.1], 0.12, GRAPHITE, mat::DECK);
    m.quad([vec3(-300.0, 0.0, -3.1), vec3(-300.0, 0.0, 300.0),
            vec3(300.0, 0.0, 300.0), vec3(300.0, 0.0, -3.1)], GRAPHITE, mat::DECK);

    let (left, right) = p.desk;
    let bay_left = left - p.rack_width();
    let span = right - bay_left;
    let center = (right + bay_left) * 0.5;
    m.part([center, DESK_Y + 0.003, -0.12], [span + 0.4, 0.012, 4.28], 0.26, RECESS, mat::SOLID);
    m.part([center, DESK_Y + 0.15, -2.72], [span + 2.0, 0.3, 0.38], 0.12, GRAPHITE, mat::SOLID);
    m.part([center, DESK_Y + 0.20, -2.50], [span + 1.5, 0.052, 0.07], 0.025, TITANIUM, mat::STRIP);

    for i in -10..=10 {
        let x = i as f32 * 5.0;
        m.part([x, DESK_Y - 0.84, 3.32], [4.76, 0.055, 0.05], 0.02, TITANIUM, mat::STRIP);
        m.part([x, DESK_Y - 2.25, 2.92], [4.80, 1.9, 0.21], 0.10, RECESS, mat::SOLID);
        m.part([x, DESK_Y - 2.26, 3.04], [4.59, 1.65, 0.075], 0.04, GRAPHITE, mat::SOLID);
        for j in 0..5 {
            m.part([x - 1.54 + j as f32 * 0.2, DESK_Y - 2.25, 3.09], [0.09, 0.58, 0.035], 0.014, RECESS, mat::SOLID);
        }
        m.part([x + 1.72, DESK_Y - 1.85, 3.10], [0.49, 0.10, 0.035], 0.02, COPPER, mat::SOLID);
        m.part([x, DESK_Y + 0.003, 2.88], [4.92, 0.012, 0.022], 0.006, GRAPHITE, mat::SOLID);
    }

    for row in 0..p.minis.div_ceil(SHELF_SLOTS) {
        let at = p.shelf_slot(row * SHELF_SLOTS);
        let width = p.rack_width() + 0.14;
        let x = left - p.rack_width() * 0.5;
        m.part([x, at.y - 0.12, at.z], [width, 0.24, 2.18], 0.12, GRAPHITE, mat::SOLID);
        for side in [-1.0, 1.0] {
            m.part([x + side * (width * 0.5 - 0.14), (at.y + DESK_Y) * 0.5, at.z - 0.84],
                   [0.22, at.y - DESK_Y, 0.25], 0.05, TITANIUM, mat::SOLID);
        }
    }
    // 2개의 작업등은 기기 자리 바깥에서 갑판 쪽으로 빛을 보낸다.
    for (x, side) in [(bay_left - 1.4, -1.0), (right + 1.4, 1.0)] {
        m.part([x, DESK_Y + 0.09, -1.9], [1.08, 0.18, 1.38], 0.17, GRAPHITE, mat::SOLID);
        m.turned(vec3(x, DESK_Y + 0.18, -1.9), Quat::from_rotation_z(-side * 0.16), |m| {
            m.part([0.0, 1.38, 0.0], [0.30, 2.76, 0.44], 0.12, GRAPHITE, mat::SOLID);
            m.part([0.0, 1.57, 0.23], [0.16, 2.03, 0.06], 0.03, TITANIUM, mat::LAMP);
        });
    }
}
