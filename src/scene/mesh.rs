//! 관측실의 조작대와 기기 모형은 둥근 상자(`slab`)를 쌓아 짓는다. 길이 1 은 12.7cm 쯤이다(맥미니 19.7cm 가 1.55).
//! 방은 배치(`Plan`)가 바뀔 때만 다시 짓고, 기기 모형은 종류마다 한 벌을 두고 자리(인스턴스)만 바꾼다.

use std::f32::consts::FRAC_PI_2;
use std::ops::Range;

use bytemuck::{Pod, Zeroable};
use glam::{Quat, Vec2, Vec3, vec2, vec3};

/// 재질 — 셰이더 `shade` 의 갈래와 같은 번호다.
mod mat {
    pub const SOLID: f32 = 0.0;
    pub const LED: f32 = 1.0;
    pub const DECK: f32 = 2.0;
    pub const SCREEN: f32 = 4.0;
    pub const CONTACT: f32 = 6.0;
    pub const LAMP: f32 = 9.0;
    pub const STRIP: f32 = 12.0;
    pub const FAN: f32 = 13.0;
    pub const COVER: f32 = 14.0;
    pub const GRILLE: f32 = 15.0;
}

const ALU: [u8; 3] = [200, 204, 210];
const SPACE: [u8; 3] = [92, 96, 104];
const GLASS: [u8; 3] = [22, 24, 30];
const SCREEN_BLUE: [u8; 3] = [104, 160, 226];
const TITANIUM: [u8; 3] = [140, 154, 161];
const GRAPHITE: [u8; 3] = [61, 74, 84];
const RECESS: [u8; 3] = [26, 36, 43];
const COPPER: [u8; 3] = [218, 119, 59];
const TOWER: [u8; 3] = [63, 73, 82];

pub const DESK_Y: f32 = 5.8;
const SHELF_SLOTS: usize = 3;
const SHELF_PITCH: f32 = 2.25;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct Vertex {
    pub point: [f32; 3],
    pub normal: [f32; 3],
    pub material: [f32; 4],
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
    /// 누름 판정·테두리 빛에 쓰는 겉 상자(모형 원점 기준 가운데와 반 크기).
    pub fn hull(self) -> (Vec3, Vec3) {
        match self {
            Model::Laptop | Model::Air | Model::Pro => (vec3(0.0, 0.88, -0.17), vec3(1.25, 0.9, 1.05)),
            Model::MiniM4 => (vec3(0.0, 0.20, 0.0), vec3(0.51, 0.21, 0.51)),
            Model::Mini => (vec3(0.0, 0.2, 0.0), vec3(0.8, 0.22, 0.8)),
            Model::Monitor => (vec3(0.0, 2.0, -0.3), vec3(2.45, 2.0, 0.55)),
            Model::Tower => (vec3(0.0, 1.8, 0.0), vec3(1.62, 1.8, 0.88)),
        }
    }

    /// 누르면 카메라가 바로 보며 다가갈 자리 — (가운데, 화면에 담을 너비·높이, 바라보는 방위각, 내려다보는 각).
    pub fn face(self) -> (Vec3, Vec2, f32, f32) {
        match self {
            Model::Laptop | Model::Air | Model::Pro => (vec3(0.0, 0.98, -0.98), vec2(2.3, 1.45), 0.0, LAPTOP_TILT),
            Model::Mini | Model::MiniM4 => (vec3(0.0, 0.2, 0.32), vec2(2.4, 1.5), 0.0, 0.42),
            Model::Monitor => (vec3(0.0, 2.58, -0.4), vec2(4.6, 2.6), 0.0, 0.06),
            Model::Tower => (vec3(0.0, 1.8, 1.72), vec2(1.8, 3.6), 0.0, 0.2),
        }
    }

    /// 이름표를 띄울 자리(모형 원점 기준) — 겉 상자 위.
    pub fn top(self) -> Vec3 {
        let (center, half) = self.hull();
        vec3(center.x, center.y + half.y, center.z)
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

    fn range(&mut self, draw: impl FnOnce(&mut Self)) -> Range<u32> {
        let start = self.vertices.len() as u32;
        draw(self);
        start..self.vertices.len() as u32
    }

    fn laptop(&mut self, model: Model) {
        let (shell, height) = match model {
            Model::Air => ([174, 199, 216], 0.088),
            Model::Pro => ([98, 103, 109], 0.165),
            _ => (ALU, 0.13),
        };
        self.part([0.0, height * 0.5, 0.0], [2.44, height, 1.72], 0.14, shell, mat::SOLID);
        self.turned(vec3(0.0, height, 0.0), Quat::IDENTITY, |m| {
            m.part([0.0, 0.008, -0.14], [1.90, 0.016, 0.88], 0.06, GLASS, mat::SOLID);
            for row in 0..5 {
                for col in 0..13 {
                    if row == 4 && (4..=8).contains(&col) { continue; }
                    m.part([(col as f32 - 6.0) * 0.14, 0.021, -0.46 + row as f32 * 0.157],
                           [0.123, 0.022, 0.135], 0.018, [49, 53, 60], mat::SOLID);
                }
            }
            m.part([0.0, 0.021, 0.168], [0.67, 0.022, 0.135], 0.018, [49, 53, 60], mat::SOLID);
            m.part([0.0, 0.006, 0.54], [0.90, 0.010, 0.47], 0.04, SPACE, mat::SOLID);
            m.part([0.0, 0.012, 0.54], [0.88, 0.008, 0.45], 0.035, shell, mat::SOLID);
            if model == Model::Pro {
                for x in [-1.06, 1.06] {
                    m.part([x, 0.007, -0.14], [0.16, 0.01, 0.83], 0.025, shell, mat::GRILLE);
                }
            }
        });
        self.turned(vec3(0.0, height, -0.84), Quat::from_rotation_x(-LAPTOP_TILT), |m| {
            m.part([0.0, 0.82, 0.0], [2.44, 1.64, 0.052], 0.07, shell, mat::SOLID);
            m.part([0.0, 0.82, 0.032], [2.40, 1.60, 0.014], 0.06, GLASS, mat::SOLID);
            m.part([0.0, 0.84, 0.043], [2.29, 1.46, 0.011], 0.04, SCREEN_BLUE, mat::SCREEN);
            if matches!(model, Model::Air | Model::Pro) {
                m.part([0.0, 1.55, 0.054], [0.24, 0.072, 0.011], 0.02, GLASS, mat::SOLID);
            }
        });
    }

    fn mini(&mut self, model: Model) {
        if model == Model::MiniM4 {
            self.part([0.0, 0.042, 0.0], [0.86, 0.084, 0.86], 0.26, GLASS, mat::SOLID);
            self.part([0.0, 0.237, 0.0], [1.00, 0.31, 1.00], 0.16, ALU, mat::SOLID);
            for x in [-0.21, -0.095] {
                self.part([x, 0.19, 0.501], [0.068, 0.026, 0.01], 0.01, GLASS, mat::SOLID);
            }
            self.part([0.13, 0.19, 0.501], [0.035, 0.035, 0.01], 0.01, GLASS, mat::SOLID);
            self.part([0.24, 0.19, 0.501], [0.017, 0.017, 0.01], 0.006, ALU, mat::LED);
            return;
        }
        self.part([0.0, 0.2, 0.0], [1.55, 0.39, 1.55], 0.24, ALU, mat::SOLID);
        self.part([0.0, 0.012, 0.0], [1.36, 0.024, 1.36], 0.2, GLASS, mat::SOLID);
        self.part([0.52, 0.16, 0.776], [0.05, 0.03, 0.012], 0.01, [140, 232, 172], mat::LED);
        for x in [-0.5, -0.36] {
            self.part([x, 0.16, 0.776], [0.09, 0.035, 0.012], 0.012, GLASS, mat::SOLID);
        }
    }

    fn monitor(&mut self) {
        self.part([0.0, 0.06, -0.3], [1.6, 0.1, 1.0], 0.2, SPACE, mat::SOLID);
        self.part([0.0, 1.0, -0.55], [0.36, 1.9, 0.14], 0.06, SPACE, mat::SOLID);
        self.part([0.0, 2.55, -0.5], [4.8, 2.85, 0.16], 0.1, [32, 33, 38], mat::SOLID);
        self.part([0.0, 2.58, -0.41], [4.5, 2.55, 0.012], 0.03, SCREEN_BLUE, mat::SCREEN);
    }

    fn tower(&mut self) {
        self.part([0.0, 1.76, -0.72], [3.22, 3.50, 0.24], 0.10, TOWER, mat::SOLID);
        for y in [0.14, 3.41] {
            self.part([0.0, y, 0.0], [3.24, 0.22, 1.72], 0.10, TOWER, mat::SOLID);
        }
        for x in [-1.54, 1.54] {
            self.part([x, 1.8, 0.0], [0.13, 3.28, 1.68], 0.05, SPACE, mat::SOLID);
        }
        self.part([-0.48, 1.91, -0.53], [1.81, 2.54, 0.09], 0.04, [37, 52, 57], mat::SOLID);
        self.part([-0.76, 2.29, -0.25], [0.68, 0.68, 0.48], 0.16, SPACE, mat::SOLID);
        self.part([-0.76, 2.29, 0.006], [0.43, 0.43, 0.04], 0.12, GLASS, mat::SOLID);
        for x in [-0.12, 0.09] {
            self.part([x, 2.22, -0.29], [0.13, 1.26, 0.45], 0.04, SPACE, mat::SOLID);
            self.part([x, 2.22, -0.05], [0.074, 1.15, 0.04], 0.02, ALU, mat::STRIP);
        }
        self.part([-0.49, 1.25, 0.02], [2.01, 0.45, 1.23], 0.07, SPACE, mat::SOLID);
        for i in 0..17 {
            self.part([-1.34 + i as f32 * 0.09, 1.25, 0.65], [0.045, 0.25, 0.038], 0.012, RECESS, mat::SOLID);
        }
        self.part([-0.52, 0.51, -0.02], [1.92, 0.45, 1.43], 0.06, TOWER, mat::SOLID);
        for y in [1.03, 2.38] {
            self.part([0.92, y, 0.24], [1.11, 1.16, 0.46], 0.11, RECESS, mat::SOLID);
            self.part([0.92, y, 0.48], [1.10, 1.14, 0.035], 0.04, TOWER, mat::FAN);
        }
        self.quad([vec3(-1.46, 0.25, 0.83), vec3(1.46, 0.25, 0.83),
                   vec3(1.46, 3.30, 0.83), vec3(-1.46, 3.30, 0.83)], ALU, mat::COVER);
    }

    fn model(&mut self, model: Model) {
        match model {
            Model::Laptop | Model::Air | Model::Pro => self.laptop(model),
            Model::Mini | Model::MiniM4 => self.mini(model),
            Model::Monitor => self.monitor(),
            Model::Tower => self.tower(),
        }
    }

    /// 접지 그림자와 선택 조명이 같은 자리를 쓰도록 반경 좌표를 함께 싣는다.
    fn halo(&mut self, model: Model) {
        let (center, half) = model.hull();
        let (x, z) = (half.x * 1.5 + 0.4, half.z * 1.5 + 0.4);
        let y = center.y - half.y + 0.03;
        let corner = |u: f32, v: f32| (vec3(center.x + u * x, y, center.z + v * z), vec3(u, 0.0, v));
        for (p, uv) in [corner(-1.0, -1.0), corner(-1.0, 1.0), corner(1.0, 1.0), corner(-1.0, -1.0), corner(1.0, 1.0), corner(1.0, -1.0)] {
            self.vertices.push(Vertex { point: (self.rotation * p).to_array(), normal: uv.to_array(), material: [1.0, 0.82, 0.58, mat::CONTACT] });
        }
    }
}

pub struct Meshes {
    pub vertices: Vec<Vertex>,
    pub room: Range<u32>,
    pub models: [(Range<u32>, Range<u32>); MODELS.len()],
}

impl Meshes {
    pub fn new(plan: &Plan) -> Self {
        let mut m = Mesh::default();
        let room = m.range(|m| room(m, plan));
        let models = MODELS.map(|model| {
            let body = m.range(|m| m.model(model));
            let halo = m.range(|m| m.halo(model));
            (body, halo)
        });
        Self { vertices: m.vertices, room, models }
    }
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
