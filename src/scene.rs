//! 강변 작업실의 책상에서 기기를 선택하면 카메라가 해당 화면으로 이동한다.

mod catalog;
mod gpu;
mod mesh;

use std::borrow::Cow;
use std::time::{Duration, Instant};

use glam::{Mat4, Vec2, Vec3, vec2, vec3};
use kasa_gridview::GridRenderer;
use winit::keyboard::KeyCode;

use crate::roster::{DeviceKind, Machine};
use crate::text::Ui;
use mesh::{Model, Plan};

const PAD: f32 = 24.0;
const BOTTOM: f32 = 52.0;
use crate::theme::{CLOUD as TEXT, INK as TAG, MIST as MUTE, RAIN as LIVE, SUN as WARM};
const IDLE: [u8; 4] = [159, 174, 191, 255];
/// 다가가며 덮는 색 — 보기 화면 바탕(`desk::BG`)과 같다.
const FADE: [u8; 3] = [24, 24, 27];
/// 한 방에 놓는 기기 수 — 넘치면 장을 넘긴다.
const PAGE: usize = 10;
const FRAME_INTERVAL: Duration = Duration::from_millis(16);
/// 비를 멈춘 창에서도 해질녘 광량은 따라간다.
const IDLE_TICK: Duration = Duration::from_secs(20);
const HIGHLIGHT_TIME: f32 = 0.16;
const DIVE_TIME: f32 = 0.75;
const FOV_Y: f32 = 0.76;
const HOME_YAW: f32 = 0.13;
const HOME_PITCH: f32 = 0.22;
const LAPTOP_SLOT: f32 = 3.6;
const DESKTOP_SLOT: f32 = 11.4;

type Point = (f32, f32);
type Rect = (f32, f32, f32, f32);

/// 바라보는 면의 반 높이로 거리를 정해 기기에 접근하는 동안에도 시야각을 유지한다.
#[derive(Clone, Copy, Debug, PartialEq)]
struct View {
    target: Vec3,
    yaw: f32,
    pitch: f32,
    half: f32,
}

impl View {
    fn direction(&self) -> Vec3 {
        let (sy, cy) = self.yaw.sin_cos();
        let (sp, cp) = self.pitch.sin_cos();
        vec3(sy * cp, sp, cy * cp)
    }

    fn with_look(self, look: Vec2) -> Self {
        Self { yaw: self.yaw + look.x * 0.048, pitch: self.pitch - look.y * 0.026, ..self }
    }

    fn mix(a: View, b: View, t: f32) -> View {
        View {
            target: a.target.lerp(b.target, t),
            yaw: a.yaw + (b.yaw - a.yaw).sin().atan2((b.yaw - a.yaw).cos()) * t,
            pitch: a.pitch + (b.pitch - a.pitch) * t,
            // 확대는 배율로 섞어야 고르게 다가간다.
            half: a.half * (b.half / a.half).powf(t),
        }
    }

    fn frame(&self, window: Point) -> Frame {
        let aspect = window.0 / window.1.max(1.0);
        let eye = self.direction();
        let distance = self.half / (FOV_Y * 0.5).tan();
        let view = Mat4::look_at_rh(self.target + eye * distance, self.target, Vec3::Y);
        let projection = Mat4::perspective_rh(FOV_Y, aspect, 0.09, distance + 400.0);
        let clip_from_world = projection * view;
        let (day, dusk, indoor) = gpu::daylight();
        Frame { window, clip_from_world, world_from_clip: clip_from_world.inverse(), camera: self.target + eye * distance, day, dusk, indoor, rain_time: 0.0 }
    }
}

pub struct Frame {
    window: Point,
    clip_from_world: Mat4,
    world_from_clip: Mat4,
    camera: Vec3,
    day: f32,
    dusk: f32,
    indoor: f32,
    rain_time: f32,
}

impl Frame {
    fn project(&self, world: Vec3) -> Vec2 {
        let ndc = self.clip_from_world.project_point3(world);
        vec2((ndc.x + 1.0) * 0.5 * self.window.0, (1.0 - ndc.y) * 0.5 * self.window.1)
    }

    fn visible(&self, p: Vec3) -> Option<Vec2> {
        let h = self.clip_from_world * p.extend(1.0);
        if h.w <= 0.0 || h.z < 0.0 || h.z > h.w || h.x.abs() > h.w || h.y.abs() > h.w { return None; }
        Some(self.project(p))
    }

    fn ray(&self, (x, y): Point) -> Option<Ray> {
        let (w, h) = self.window;
        if x < 0.0 || x >= w || y < 0.0 || y >= h {
            return None;
        }
        let (x, y) = (x / w * 2.0 - 1.0, 1.0 - y / h * 2.0);
        let near = self.world_from_clip.project_point3(vec3(x, y, 0.0));
        let far = self.world_from_clip.project_point3(vec3(x, y, 1.0));
        Some(Ray { origin: near, direction: (far - near).normalize() })
    }
}

struct Ray {
    origin: Vec3,
    direction: Vec3,
}

impl Ray {
    fn hit_box(&self, low: Vec3, high: Vec3) -> Option<f32> {
        let mut near: f32 = 0.0;
        let mut far = f32::INFINITY;
        for axis in 0..3 {
            let (o, d) = (self.origin[axis], self.direction[axis]);
            if d.abs() < 1e-6 {
                if o < low[axis] || o > high[axis] {
                    return None;
                }
                continue;
            }
            let (a, b) = ((low[axis] - o) / d, (high[axis] - o) / d);
            near = near.max(a.min(b));
            far = far.min(a.max(b));
        }
        (near <= far).then_some(near)
    }
}

/// 방에 놓인 기기 하나 — 선택 조명과 화면 빛(길이 서 있으면 1).
struct Device {
    index: usize,
    highlight: f32,
    glow: f32,
}

/// 기기의 모형 한 조각 — Windows 는 모니터(주)와 타워 둘이다.
struct Part {
    device: usize,
    model: Model,
    asset: Option<usize>,
    bounds: Option<(Vec3, Vec3)>,
    position: Vec3,
    main: bool,
}

impl Part {
    fn new(device: usize, machine: &Machine, model: Model, position: Vec3) -> Self {
        let library = catalog::library();
        let asset = library.resolve(machine, model);
        let bounds = asset.and_then(|i| library.assets[i].geometry.as_ref()).map(|g| (g.center, g.half));
        Self { device, model, asset, bounds, position, main: model != Model::Tower }
    }

    fn asset(&self) -> Option<&'static catalog::Asset> {
        self.asset.map(|i| &catalog::library().assets[i])
    }

    fn top(&self) -> Vec3 {
        self.bounds.map_or(Vec3::Y * 0.45, |(center, half)| center + Vec3::Y * half.y)
    }
}

pub struct Layout {
    window: Point,
    plan: Plan,
    home: View,
    devices: Vec<Device>,
    parts: Vec<Part>,
}

impl Layout {
    fn new(window: Point, machines: &[Machine], page: usize) -> Self {
        let first = page * PAGE;
        let page_machines = &machines[first.min(machines.len())..machines.len().min(first + PAGE)];
        let catalog = catalog::library();
        let kinds: Vec<_> = page_machines.iter().map(|m| catalog.kind(m)).collect();
        let slot = |kind| if kind == DeviceKind::Desktop { DESKTOP_SLOT } else { LAPTOP_SLOT };
        let on_desk: Vec<usize> = (0..page_machines.len()).filter(|&i| !kinds[i].on_rack()).collect();
        let minis: Vec<usize> = (0..page_machines.len()).filter(|&i| kinds[i].on_rack()).collect();
        let desk_len = (1.3 + on_desk.iter().map(|&i| slot(kinds[i])).sum::<f32>() + 1.3).max(5.6);
        let plan = Plan::new(desk_len, minis.len());
        let mut parts = Vec::new();
        let mut x = plan.desk_start();
        for &i in &on_desk {
            let width = slot(kinds[i]);
            let cx = x + width / 2.0;
            x += width;
            if kinds[i] == DeviceKind::Desktop {
                parts.push(Part::new(i, &page_machines[i], Model::Monitor, vec3(cx - 1.90, mesh::DESK_Y, -0.3)));
                parts.push(Part::new(i, &page_machines[i], Model::Tower, vec3(cx + 3.92, mesh::DESK_Y, -0.35)));
            } else {
                let model = match kinds[i] {
                    DeviceKind::Air => Model::Air,
                    DeviceKind::Pro => Model::Pro,
                    _ => Model::Laptop,
                };
                parts.push(Part::new(i, &page_machines[i], model, vec3(cx, mesh::DESK_Y, 0.25)));
            }
        }
        for (k, &i) in minis.iter().enumerate() {
            let model = if kinds[i] == DeviceKind::MiniM4 { Model::MiniM4 } else { Model::Mini };
            let x = plan.mini_x(k);
            let mut mini = Part::new(i, &page_machines[i], model, vec3(x + 2.45, mesh::DESK_Y + 0.30, 1.0));
            mini.main = false;
            parts.push(mini);
            parts.push(Part::new(i, &page_machines[i], Model::Monitor, vec3(x - 0.65, mesh::DESK_Y, -0.4)));
        }
        let devices = (0..page_machines.len()).map(|i| Device { index: first + i, highlight: 0.0, glow: 0.0 }).collect();
        Self { window, home: home_view(&plan, window), plan, devices, parts }
    }

    fn computer_asset(&self, device: usize) -> Option<&'static catalog::Asset> {
        self.parts.iter().find(|p| p.device == device && matches!(p.model, Model::Mini | Model::MiniM4))
            .or_else(|| self.main_part(device)).and_then(Part::asset)
    }

    fn main_part(&self, device: usize) -> Option<&Part> {
        self.parts.iter().find(|p| p.device == device && p.main)
    }

    fn device_of(&self, index: usize) -> Option<usize> {
        self.devices.iter().position(|d| d.index == index)
    }

    fn hit(&self, frame: &Frame, pointer: Point) -> Option<usize> {
        let ray = frame.ray(pointer)?;
        self.parts.iter().filter_map(|part| {
            let device = &self.devices[part.device];
            let (center, half) = part.bounds?;
            let center = part.position + center;
            ray.hit_box(center - half, center + half).map(|t| (t, device.index))
        }).min_by(|a, b| a.0.total_cmp(&b.0)).map(|(_, index)| index)
    }

    /// 기기마다 이름표 자리(화면 좌표).
    fn anchors(&self, frame: &Frame) -> Vec<(usize, Vec2)> {
        self.devices.iter().enumerate().filter_map(|(d, device)| {
            let part = self.main_part(d)?;
            Some((device.index, frame.visible(part.position + part.top() + Vec3::Y * 0.35)?))
        }).collect()
    }

    /// 그 기기 화면 바로 앞.
    fn dive_view(&self, index: usize) -> Option<View> {
        let part = self.main_part(self.device_of(index)?)?;
        let (mut face, mut size, yaw, pitch) = part.model.face();
        if let Some((center, half)) = part.bounds {
            let (nominal_center, nominal_half) = part.model.hull();
            let ratio = half / nominal_half;
            face = center + (face - nominal_center) * ratio;
            size *= vec2(ratio.x, ratio.y);
        }
        let aspect = self.window.0 / self.window.1.max(1.0);
        let half = (size.y * 0.5 / 0.62).max(size.x * 0.5 / 0.84 / aspect);
        Some(View { target: part.position + face, yaw, pitch, half })
    }
}

fn home_view(plan: &Plan, (width, height): Point) -> View {
    let (desk_left, right) = plan.desk;
    let left = desk_left - plan.rack_width();
    let aspect = width / height.max(1.0);
    let half = ((right - left + 3.5) / (2.0 * aspect) + 0.45).max(4.8);
    View {
        target: vec3((left + right) * 0.5, mesh::DESK_Y + 1.1 + half * 0.32, 0.0),
        yaw: HOME_YAW, pitch: HOME_PITCH, half,
    }
}

/// 다가가기 — `progress` 0 은 방, 1 은 화면 앞. `out` 이면 1 에서 0 으로 빠져나온다.
struct Dive {
    index: usize,
    progress: f32,
    out: bool,
}

impl Dive {
    fn eased(&self) -> f32 {
        let t = self.progress;
        if t < 0.5 { 4.0 * t * t * t } else { 1.0 - (-2.0 * t + 2.0).powi(3) / 2.0 }
    }
}

#[derive(Default)]
pub struct Room {
    layout: Option<Layout>,
    gpu: Option<gpu::RoomGpu>,
    rain_time: f64,
    rain_paused: bool,
    pointer: Option<Point>,
    look: Vec2,
    free: Option<View>,
    prior_rgb_view: Option<Option<View>>,
    movement: [bool; 4],
    dragging: Option<Point>,
    hovered: Option<usize>,
    keyboard: Option<String>,
    selected: Option<usize>,
    page: usize,
    animated_at: Option<Instant>,
    painted_at: Option<Instant>,
    dive: Option<Dive>,
    arrived: Option<usize>,
    tags: Vec<(Rect, usize)>,
}

impl Room {
    pub fn reflow(&mut self, window: Point, machines: &[Machine]) {
        let window = (window.0.max(1.0), window.1.max(1.0));
        let key = self.keyboard.as_ref().and_then(|name| machines.iter().position(|m| &m.label == name));
        if self.keyboard.is_some() && key.is_none() {
            self.keyboard = None;
        }
        self.page = key.map_or(self.page.min(last_page(machines.len())), |i| i / PAGE);
        self.layout = Some(Layout::new(window, machines, self.page));
        self.tags.clear();
        if self.dive.as_ref().is_some_and(|d| self.layout.as_ref().and_then(|l| l.device_of(d.index)).is_none()) {
            self.dive = None;
        }
        self.hovered = self.pointer.and_then(|p| self.hit_at(p));
        self.select(key.or(self.hovered).or(self.dive.as_ref().map(|d| d.index)));
    }

    pub fn suspend(&mut self) {
        *self = Self { free: self.prior_rgb_view.take().unwrap_or(self.free), ..Self::default() };
    }

    fn home_frame(&self) -> Option<Frame> {
        let layout = self.layout.as_ref()?;
        Some(self.free.unwrap_or_else(|| layout.home.with_look(self.look)).frame(layout.window))
    }

    pub fn stop_navigation(&mut self) {
        self.movement = [false; 4];
        self.dragging = None;
    }

    pub fn travel_key(&mut self, code: KeyCode, pressed: bool) -> bool {
        let axis = match code {
            KeyCode::KeyW => 0, KeyCode::KeyA => 1, KeyCode::KeyS => 2, KeyCode::KeyD => 3,
            KeyCode::Home if pressed && self.dive.is_none() => {
                self.free = None;
                self.stop_navigation();
                self.tags.clear();
                return true;
            }
            _ => return false,
        };
        if self.dive.is_some() { return false }
        if self.movement[axis] != pressed && !self.movement.iter().any(|&down| down) {
            self.animated_at = Some(Instant::now());
        }
        self.movement[axis] = pressed;
        true
    }

    fn move_eye(view: &mut View, delta: Vec3, layout: &Layout) {
        let distance = view.half / (FOV_Y * 0.5).tan();
        let offset = view.direction() * distance;
        let eye = view.target + offset;
        let (lo, hi) = layout.plan.limits();
        let home_eye = layout.home.target + layout.home.direction() * (layout.home.half / (FOV_Y * 0.5).tan());
        view.target = (eye + delta).clamp(lo + vec3(0.5, 1.0, 0.5), (hi - Vec3::splat(0.5)).max(home_eye)) - offset;
    }

    pub fn scroll(&mut self, steps: f32) -> bool {
        if self.dive.is_some() || !steps.is_finite() || steps == 0.0 { return false }
        let Some(layout) = &self.layout else { return false };
        let view = self.free.get_or_insert_with(|| layout.home.with_look(self.look));
        let delta = -view.direction() * steps.clamp(-8.0, 8.0) * 1.25;
        Self::move_eye(view, delta, layout);
        self.tags.clear();
        true
    }

    pub fn drag(&mut self, at: Option<Point>) -> bool {
        if self.dive.is_some() { return false }
        self.dragging = at;
        true
    }

    fn look_target(pointer: Option<Point>, window: Point) -> Vec2 {
        pointer.map_or(Vec2::ZERO, |(x, y)| {
            (vec2(x / window.0, y / window.1) * 2.0 - Vec2::ONE).clamp(-Vec2::ONE, Vec2::ONE)
        })
    }

    fn hit_at(&self, p: Point) -> Option<usize> {
        if self.dive.is_some() {
            return None;
        }
        if let Some(&(_, index)) = self.tags.iter().find(|(r, _)| contains(*r, p)) {
            return Some(index);
        }
        self.layout.as_ref()?.hit(&self.home_frame()?, p)
    }

    fn select(&mut self, selected: Option<usize>) -> bool {
        let changed = self.selected != selected;
        self.selected = selected;
        if changed || self.animated_at.is_none() {
            self.animated_at = Some(Instant::now());
        }
        changed
    }

    pub fn pointer(&mut self, point: Option<Point>) -> bool {
        let moved = point != self.pointer;
        self.pointer = point;
        if let Some(last) = self.dragging {
            if let (Some(p), Some(layout)) = (point, &self.layout) {
                let v = self.free.get_or_insert_with(|| layout.home.with_look(self.look));
                let d = v.half / (FOV_Y * 0.5).tan();
                let eye = v.target + v.direction() * d;
                v.yaw = (v.yaw - (p.0 - last.0) * 0.004) % std::f32::consts::TAU;
                v.pitch = (v.pitch + (p.1 - last.1) * 0.004).clamp(-1.35, 1.35);
                v.target = eye - v.direction() * d;
                self.tags.clear();
            }
            self.dragging = point;
            return self.select(None) || moved;
        }
        if self.dive.is_some() {
            return false;
        }
        self.hovered = point.and_then(|p| self.hit_at(p));
        if point.is_some() {
            self.keyboard = None;
            self.select(self.hovered) || moved
        } else if self.keyboard.is_none() {
            self.select(None) || moved
        } else {
            false
        }
    }

    pub fn rescale_pointer(&mut self, ratio: f32) {
        self.pointer = self.pointer.map(|(x, y)| (x * ratio, y * ratio));
    }

    pub fn pointer_on_device(&self) -> bool {
        self.dive.is_none() && self.hovered.is_some()
    }

    pub fn hit(&self) -> Option<usize> {
        self.hit_at(self.pointer?)
    }

    /// 그 기기로 다가가기 시작한다 — 다 다가가면 `take_arrival` 이 돌려준다.
    pub fn dive(&mut self, index: usize) -> bool {
        if self.dive.is_some() || self.layout.as_ref().and_then(|l| l.device_of(index)).is_none() {
            return false;
        }
        self.stop_navigation();
        self.dive = Some(Dive { index, progress: 0.0, out: false });
        self.keyboard = None;
        self.select(Some(index));
        self.animated_at = Some(Instant::now());
        true
    }

    /// 보기 화면에서 돌아올 때 — 그 기기 화면 앞에서 방으로 빠져나온다.
    pub fn surface(&mut self, index: usize) {
        if self.layout.as_ref().and_then(|l| l.device_of(index)).is_some() {
            self.dive = Some(Dive { index, progress: 1.0, out: true });
            self.animated_at = Some(Instant::now());
        }
    }

    pub fn take_arrival(&mut self) -> Option<usize> {
        let index = self.arrived.take()?;
        self.dive = None;
        self.select(self.hovered);
        Some(index)
    }

    pub fn keyboard(&mut self, code: KeyCode, machines: &[Machine]) -> bool {
        if self.dive.is_some() || machines.is_empty() {
            return false;
        }
        let Some(frame) = self.home_frame() else { return false };
        let Some(layout) = &self.layout else { return false };
        let step = match code {
            KeyCode::ArrowLeft => vec2(-1.0, 0.0),
            KeyCode::ArrowRight => vec2(1.0, 0.0),
            KeyCode::ArrowUp => vec2(0.0, -1.0),
            KeyCode::ArrowDown => vec2(0.0, 1.0),
            _ => return false,
        };
        let anchors = layout.anchors(&frame);
        let from = self.selected.and_then(|s| anchors.iter().find(|(i, _)| *i == s)).map(|a| a.1);
        let found = match from {
            None => anchors.first().map(|a| a.0),
            Some(from) => anchors.iter().filter(|(i, _)| Some(*i) != self.selected).filter_map(|&(i, p)| {
                let d = p - from;
                let along = d.dot(step);
                (along > 1.0).then(|| (along + (d - step * along).length() * 2.0, i))
            }).min_by(|a, b| a.0.total_cmp(&b.0)).map(|a| a.1),
        };
        let found = found.or_else(|| {
            // 그쪽에 더 없으면 옆 장으로.
            let last = last_page(machines.len());
            match code {
                KeyCode::ArrowRight if self.page < last => Some((self.page + 1) * PAGE),
                KeyCode::ArrowLeft if self.page > 0 => Some(self.page * PAGE - 1),
                _ => None,
            }
        });
        let Some(found) = found else { return false };
        self.keyboard = Some(machines[found].label.clone());
        if found / PAGE != self.page {
            let window = layout.window;
            self.reflow(window, machines);
            return true;
        }
        self.select(Some(found))
    }

    pub fn rgb_index(&self, machines: &[Machine], config: Option<&crate::rgb::Config>) -> Option<usize> {
        let config = config?;
        let layout = self.layout.as_ref()?;
        if self.dive.is_some() {
            return None;
        }
        let tower = layout.parts.iter().find(|p| {
            p.model == Model::Tower
                && catalog::library().is_y60(p.asset)
                && p.bounds.is_some()
                && machines.get(layout.devices[p.device].index).is_some_and(|m| config.matches(m))
        })?;
        Some(layout.devices[tower.device].index)
    }

    pub fn fan_hit(&self, index: usize, p: Point) -> Option<usize> {
        let layout = self.layout.as_ref()?;
        let part = layout.parts.iter().find(|p| p.model == Model::Tower && layout.devices[p.device].index == index)?;
        let frame = self.home_frame()?;
        if layout.hit(&frame, p) != Some(index) {
            return None;
        }
        let ray = frame.ray(p)?;
        mesh::y60::fan_hit(ray.origin - part.position, ray.direction)
    }

    pub fn rgb_view(&mut self, index: Option<usize>) {
        self.stop_navigation();
        self.tags.clear();
        if let Some(index) = index {
            if self.prior_rgb_view.is_some() {
                return;
            }
            let Some(l) = &self.layout else { return };
            let Some(p) = l.parts.iter().find(|p| p.model == Model::Tower && l.devices[p.device].index == index) else { return };
            let (center, half) = p.bounds.unwrap_or_else(|| p.model.hull());
            let aspect = l.window.0 / l.window.1.max(1.0);
            let framing = (half.y * 1.60).max(half.x * 1.90 / aspect);
            let yaw = 0.065_f32;
            let right = vec3(yaw.cos(), 0.0, -yaw.sin());
            let shift = if l.window.0 > 580.0 { framing * aspect * (380.0 / l.window.0).min(0.58) } else { 0.0 };
            self.prior_rgb_view = Some(self.free);
            self.free =
                Some(View { target: p.position + center - right * shift + Vec3::Y * 0.12, yaw, pitch: 0.10, half: framing });
        } else if let Some(v) = self.prior_rgb_view.take() {
            self.free = v;
        }
    }

    pub fn has_screen(&self, index: usize) -> bool {
        self.layout.as_ref().and_then(|l| l.main_part(l.device_of(index)?)).and_then(Part::asset)
            .and_then(|a| a.geometry.as_ref()?.screen).is_some()
    }

    pub fn selected(&self) -> Option<usize> {
        self.selected.filter(|_| self.dive.is_none())
    }

    fn paging(&self, total: usize) -> Option<(usize, [Rect; 2])> {
        let (width, height) = self.layout.as_ref()?.window;
        let last = last_page(total);
        let y = height - BOTTOM - 34.0;
        (last > 0).then(|| (last, [(width * 0.5 - 62.0, y, 30.0, 28.0), (width * 0.5 + 32.0, y, 30.0, 28.0)]))
    }

    pub fn pointer_on_page(&self, total: usize) -> bool {
        self.page_step(total).is_some()
    }

    fn page_step(&self, total: usize) -> Option<isize> {
        let (last, [prev, next]) = self.paging(total)?;
        let p = self.pointer?;
        if self.page > 0 && contains(prev, p) {
            Some(-1)
        } else if self.page < last && contains(next, p) {
            Some(1)
        } else {
            None
        }
    }

    pub fn click_page(&mut self, machines: &[Machine]) -> bool {
        let Some(step) = self.page_step(machines.len()) else { return false };
        self.turn_page(step, machines)
    }

    pub fn turn_page(&mut self, step: isize, machines: &[Machine]) -> bool {
        let Some(layout) = &self.layout else { return false };
        let window = layout.window;
        let page = self.page.saturating_add_signed(step).min(last_page(machines.len()));
        if self.page == page || self.dive.is_some() {
            return false;
        }
        self.page = page;
        self.keyboard = None;
        self.reflow(window, machines);
        true
    }

    pub fn toggle_rain(&mut self) {
        self.rain_paused = !self.rain_paused;
    }

    pub fn next_frame(&self, animate_rain: bool) -> Option<Instant> {
        let layout = self.layout.as_ref()?;
        let moving = self.dive.as_ref().is_some_and(|d| self.arrived.is_none() && (d.out || d.progress < 1.0))
            || self.movement.iter().any(|&down| down)
            || (self.free.is_none() && self.dive.is_none() && self.look != Self::look_target(self.pointer, layout.window))
            || layout.devices.iter().any(|d| d.highlight != highlight_target(self.selected, d.index));
        if moving {
            return Some(self.animated_at.unwrap_or_else(Instant::now) + FRAME_INTERVAL);
        }
        let tick = if animate_rain && !self.rain_paused { Duration::from_millis(42) } else { IDLE_TICK };
        self.painted_at.map(|at| at + tick)
    }

    fn animate(&mut self, now: Instant) {
        // 창이 멈췄다 깨어나도 다가가기를 건너뛰지 않게 한 번에 0.1초까지만.
        let elapsed = self.animated_at.replace(now).map_or(0.0, |at| now.saturating_duration_since(at).as_secs_f32()).min(0.1);
        let Some(layout) = &mut self.layout else { return };
        if self.dive.is_none() && self.movement.iter().any(|&down| down) {
            let down = |i: usize| u8::from(self.movement[i]) as f32;
            let v = self.free.get_or_insert_with(|| layout.home.with_look(self.look));
            let (sin, cos) = v.yaw.sin_cos();
            let forward = vec3(-sin, 0.0, -cos);
            let right = vec3(cos, 0.0, -sin);
            let walk = forward * (down(0) - down(2)) + right * (down(3) - down(1));
            Self::move_eye(v, walk.normalize_or_zero() * elapsed * 11.0, layout);
            self.tags.clear();
        }
        if self.free.is_none() && self.dive.is_none() {
            let target = Self::look_target(self.pointer, layout.window);
            self.look = self.look.lerp(target, 1.0 - (-elapsed * 12.0).exp());
            if self.look.distance_squared(target) < 1e-5 { self.look = target; }
        }
        let delta = elapsed / HIGHLIGHT_TIME;
        for device in &mut layout.devices {
            let target = highlight_target(self.selected, device.index);
            device.highlight = if target > device.highlight { (device.highlight + delta).min(target) } else { (device.highlight - delta).max(target) };
        }
        if let Some(dive) = &mut self.dive {
            let step = elapsed / DIVE_TIME;
            if dive.out {
                dive.progress = (dive.progress - step).max(0.0);
                if dive.progress <= 0.0 {
                    self.dive = None;
                }
            } else {
                dive.progress = (dive.progress + step).min(1.0);
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn paint(
        &mut self,
        preview: Option<(usize, &crate::preview::Picture)>,
        rgb: Option<(usize, &crate::rgb::Snapshot)>,
        editing: Option<(usize, usize)>,
        g: &mut GridRenderer,
        ui: &mut Ui,
        machines: &[Machine],
        paths: &[String],
        live: &[bool],
        foot: &str,
        app_alive: bool,
        overlay: impl FnOnce(&mut GridRenderer, &mut Ui, (f32, f32)),
    ) -> anyhow::Result<usize> {
        let now = Instant::now();
        self.animate(now);
        if !self.rain_paused {
            self.rain_time += self.painted_at.map_or(0.0, |at| now.duration_since(at).as_secs_f64().min(0.1));
        }
        self.painted_at = Some(now);
        let paging = self.paging(machines.len());
        let Some(layout) = &mut self.layout else { return g.render(0.0, true, |_, _, _, _, _, _| {}) };
        for device in &mut layout.devices {
            device.glow = if live.get(device.index).copied().unwrap_or(false) { 1.0 } else { 0.0 };
        }
        let layout = &*layout;
        let (width, height) = layout.window;
        let dive = self.dive.as_ref().map(|d| (d.index, d.eased(), d.progress));
        let home_view = self.free.unwrap_or_else(|| layout.home.with_look(self.look));
        let view = dive.and_then(|(i, t, _)| Some(View::mix(home_view, layout.dive_view(i)?, t))).unwrap_or(home_view);
        let mut frame = view.frame(layout.window);
        frame.rain_time = self.rain_time as f32;
        let calm = 1.0 - dive.map_or(0.0, |(_, t, _)| smooth(0.0, 0.3, t));

        let home = home_view.frame(layout.window);
        let mut tags = Vec::new();
        if calm > 0.0 && editing.is_none() {
            // 고른 기기 먼저 제자리에, 나머지는 겹치면 위로 비켜 쌓는다 — 이름을 접지 않는다.
            let mut anchors: Vec<_> = layout.anchors(&home).into_iter()
                .map(|(i, p)| (i, p, machines[i].label.as_str())).collect();
            for part in layout.parts.iter().filter(|p| p.model == Model::Tower) {
                if let Some(asset) = part.asset() {
                    let where_ = if asset.geometry.is_none() { Vec3::Y * 0.5 } else { part.top() };
                    let Some(at) = home.visible(part.position + where_) else { continue };
                    anchors.push((layout.devices[part.device].index, at, &asset.name));
                }
            }
            anchors.sort_by(|a, b| (Some(b.0) == self.selected).cmp(&(Some(a.0) == self.selected)).then(b.1.y.total_cmp(&a.1.y)));
            let mut placed: Vec<(Rect, usize, Vec2, bool, Tag)> = Vec::new();
            for &(index, at, name) in &anchors {
                let focus = self.selected == Some(index);
                let product = if name == machines[index].label {
                    catalog::library().description(&machines[index]).or_else(||
                        layout.device_of(index).and_then(|i| layout.computer_asset(i)).map(|a| a.name.as_str()))
                } else { None };
                let tag = Tag { name, product, path: &paths[index], live: live.get(index).copied().unwrap_or(false) };
                let (w, h) = tag.size(g, ui, focus);
                let left = (at.x - w / 2.0).clamp(4.0, (width - w - 4.0).max(4.0));
                let mut rect = (left, at.y - h - 8.0, w, h);
                for _ in 0..12 {
                    let Some(top) = placed.iter().filter(|(r, ..)| overlaps(*r, rect)).map(|(r, ..)| r.1).min_by(f32::total_cmp) else { break };
                    rect.1 = top - h - 4.0;
                }
                placed.push((rect, index, at, focus, tag));
            }
            placed.sort_by_key(|p| p.3);
            // 줄을 먼저 다 긋는다 — 위로 비켜 선 이름표의 줄이 아래 이름표 글자를 지우지 않게.
            for &((_, y, _, h), _, at, _, _) in &placed {
                let stem = at.y - (y + h);
                if stem > 0.0 {
                    g.rect(at.x - 1.0, y + h, 2.0, stem, fade(TAG, calm * 0.8));
                }
            }
            for (rect, index, _, focus, tag) in placed {
                tag.draw(g, ui, rect, focus, calm);
                tags.push((rect, index));
            }
        }

        if let Some((index, fan)) = editing {
            if let Some(tower) = layout.parts.iter().find(|p| p.model == Model::Tower && layout.devices[p.device].index == index)
            {
                if let Some(at) = home.visible(tower.position + mesh::y60::fan_center(fan)) {
                    let label = crate::rgb::NAMES[fan];
                    let width = ui.measure(g, label, 11.0, true) + 18.0;
                    g.round_rect_stroke(at.x - 7.0, at.y - 7.0, 14.0, 14.0, 7.0, 1.5, [254, 211, 150, 245]);
                    g.rect(at.x + 7.0, at.y - 0.75, 10.0, 1.5, [254, 211, 150, 245]);
                    g.round_rect_fill(at.x + 16.0, at.y - 10.0, width, 20.0, 4.0, [26, 34, 38, 238]);
                    ui.draw(g, at.x + 25.0, at.y - 5.6, label, 11.0, [254, 211, 150, 255], true);
                }
            }
        }
        let heading = if editing.is_some() { "조명 · 냉각" } else { "여우비 작업실" };
        let title_w = ui.measure(g, heading, 18.0, true) + 71.0;
        g.round_rect_fill(PAD, 16.0, title_w, 52.0, 9.0, fade(TAG, calm));
        crate::theme::sunshower(g, PAD + 12.0, 25.0, calm);
        g.rect(PAD + 52.0, 25.0, 1.0, 34.0, fade(crate::theme::EDGE, calm));
        ui.draw(g, PAD + 62.0, 22.0, "KasaDesk", 10.0, fade(LIVE, calm), true);
        ui.draw(g, PAD + 61.0, 37.0, heading, 18.0, fade(TEXT, calm), true);
        if machines.is_empty() {
            let text = if app_alive { "카사텀 명부에 기기를 추가하세요" } else { "카사텀을 띄우거나 로그인하면 기기가 보여요" };
            let desk = home.project(vec3((layout.plan.desk.0 + layout.plan.desk.1) / 2.0, mesh::DESK_Y + 3.0, 0.0));
            let w = ui.measure(g, text, 14.0, false) + 32.0;
            g.round_rect_fill(desk.x - w / 2.0, desk.y - 18.0, w, 36.0, 18.0, TAG);
            ui.draw(g, desk.x - w / 2.0 + 16.0, desk.y - 9.0, text, 14.0, TEXT, false);
        }
        if let Some((last, [left, right])) = paging {
            for ((x, y, w, h), mark, enabled) in [(left, "‹", self.page > 0), (right, "›", self.page < last)] {
                g.round_rect_fill(x, y, w, h, 14.0, TAG);
                let mw = ui.measure(g, mark, 18.0, true);
                ui.draw(g, x + (w - mw) / 2.0, y + 3.0, mark, 18.0, if enabled { TEXT } else { IDLE }, true);
            }
            let pages = format!("{}/{}", self.page + 1, last + 1);
            let pw = ui.measure(g, &pages, 12.0, false);
            ui.draw(g, width * 0.5 - pw / 2.0, left.1 + 7.0, &pages, 12.0, MUTE, false);
        }
        let travel = [("WASD", "이동"), ("휠", "앞뒤"), ("우클릭", "시선"), ("Home", "복귀"),
            ("R", if self.rain_paused { "날씨 재생" } else { "날씨 멈춤" })];
        let fan_controls = [("클릭", "팬 선택"), ("Tab", "이동"), ("Esc", "닫기")];
        let controls = if editing.is_some() { fan_controls.as_slice() } else { travel.as_slice() };
        let mut control_x = width - PAD;
        let bg = fade(TAG, calm);
        if !machines.is_empty() && width >= 680.0 {
            for &(key, action) in controls.iter().rev() {
                let key_w = ui.measure(g, key, 11.0, true) + 12.0;
                let action_w = ui.measure(g, action, 11.5, false);
                let w = key_w + action_w + 17.0;
                control_x -= w;
                g.round_rect_fill(control_x,height-39.0,w,25.0,6.0,bg);
                g.round_rect_fill(control_x+3.0,height-36.0,key_w,19.0,4.0,fade(crate::theme::SURFACE,calm));
                ui.draw(g,control_x+9.0,height-33.0,key,11.0,fade(LIVE,calm),true);
                ui.draw(g,control_x+key_w+9.0,height-33.0,action,11.5,fade(TEXT,calm),false);
                control_x -= 3.0;
            }
        }
        let foot = editing.map_or(foot, |(i, _)| machines[i].label.as_str());
        let foot = fit(g, ui, foot, 12.0, false, control_x - PAD - 34.0);
        let foot_w = ui.measure(g, &foot, 12.0, false) + 28.0;
        g.round_rect_fill(PAD,height-39.0,foot_w,25.0,6.0,bg);
        g.round_rect_fill(PAD+9.0,height-29.0,5.0,5.0,2.5,fade(if app_alive { LIVE } else { WARM },calm));
        ui.draw(g,PAD+20.0,height-33.0,&foot,12.0,fade(TEXT,calm),false);
        overlay(g, ui, (width, height));
        if let Some((_, _, progress)) = dive {
            let a = smooth(0.62, 1.0, progress);
            if a > 0.0 {
                g.rect(0.0, 0.0, width, height, [FADE[0], FADE[1], FADE[2], (a * 255.0) as u8]);
            }
        }
        let arrived = dive.and_then(|(i, _, p)| (p >= 1.0 && self.dive.as_ref().is_some_and(|d| !d.out)).then_some(i));
        let gpu = &mut self.gpu;
        let result = g.render_with_underlay(0.0, true, |device, queue, encoder, texture, target, format| {
            gpu.get_or_insert_with(|| gpu::RoomGpu::new(device, queue, format))
                .draw(device, queue, encoder, texture, target, layout, &frame, preview, rgb);
        });
        self.tags = tags;
        if self.keyboard.is_none() && self.dive.is_none() && self.dragging.is_none() {
            self.hovered = self.pointer.and_then(|p| self.hit_at(p));
            self.select(self.hovered);
        }
        if arrived.is_some() {
            self.arrived = arrived;
        }
        result
    }
}

struct Tag<'a> {
    name: &'a str,
    product: Option<&'a str>,
    path: &'a str,
    live: bool,
}

impl Tag<'_> {
    const DOT: f32 = 7.0;
    const NAME_MAX: f32 = 200.0;
    const PATH_MAX: f32 = 300.0;

    fn text_size(focus: bool) -> f32 {
        if focus { 14.0 } else { 12.5 }
    }

    fn lines(&self, g: &mut GridRenderer, ui: &mut Ui, focus: bool) -> (String, Option<String>) {
        let name = fit(g, ui, self.name, Self::text_size(focus), focus, Self::NAME_MAX).into_owned();
        let path = focus.then(|| {
            let text = self.product.map(|product| format!("{product} · {}", self.path));
            fit(g, ui, text.as_deref().unwrap_or(self.path), 11.5, false, Self::PATH_MAX).into_owned()
        });
        (name, path)
    }

    /// 이름표 크기 — 평소엔 점과 이름, 고르면 길 상태 한 줄을 더.
    fn size(&self, g: &mut GridRenderer, ui: &mut Ui, focus: bool) -> (f32, f32) {
        let (name, path) = self.lines(g, ui, focus);
        let name_w = ui.measure(g, &name, Self::text_size(focus), focus);
        let path_w = path.as_ref().map_or(0.0, |p| ui.measure(g, p, 11.5, false));
        ((name_w + Self::DOT + 6.0).max(path_w) + 24.0, if focus { 46.0 } else { 26.0 })
    }

    /// 기기 위에 뜨는 이름표.
    fn draw(&self, g: &mut GridRenderer, ui: &mut Ui, (x, y, w, h): Rect, focus: bool, alpha: f32) {
        let bg = fade(TAG, alpha);
        let size = Self::text_size(focus);
        let (name, path) = self.lines(g, ui, focus);
        if focus {
            g.round_rect_fill(x - 1.5, y - 1.5, w + 3.0, h + 3.0, 7.0, fade(WARM, alpha));
        }
        g.round_rect_fill(x, y, w, h, 6.0, bg);
        if !focus { g.round_rect_stroke(x,y,w,h,6.0,1.0,fade(crate::theme::EDGE, alpha)); }
        if focus {
            g.round_rect_fill(x + 3.0, y + 8.0, 2.0, h - 16.0, 1.0, fade(WARM, alpha));
        }
        let line = if focus { 7.0 } else { 5.5 };
        let dot = fade(if self.live { LIVE } else { IDLE }, alpha);
        g.round_rect_fill(x + 12.0, y + line + size * 0.6 - Self::DOT / 2.0, Self::DOT, Self::DOT, Self::DOT / 2.0, dot);
        ui.draw(g, x + 12.0 + Self::DOT + 6.0, y + line, &name, size, fade(TEXT, alpha), focus);
        if let Some(path) = path {
            ui.draw(g, x + 12.0, y + 26.0, &path, 11.5, fade(MUTE, alpha), false);
        }
    }
}

fn overlaps(a: Rect, b: Rect) -> bool {
    a.0 < b.0 + b.2 + 4.0 && b.0 < a.0 + a.2 + 4.0 && a.1 < b.1 + b.3 + 4.0 && b.1 < a.1 + a.3 + 4.0
}

fn fade(c: [u8; 4], alpha: f32) -> [u8; 4] {
    [c[0], c[1], c[2], (f32::from(c[3]) * alpha.clamp(0.0, 1.0)) as u8]
}

fn smooth(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn last_page(count: usize) -> usize {
    count.saturating_sub(1) / PAGE
}

fn highlight_target(selected: Option<usize>, index: usize) -> f32 {
    if selected == Some(index) { 1.0 } else { 0.0 }
}

fn contains((x, y, w, h): Rect, (px, py): Point) -> bool {
    px >= x && px < x + w && py >= y && py < y + h
}

fn fit<'a>(g: &mut GridRenderer, ui: &mut Ui, text: &'a str, size: f32, bold: bool, max: f32) -> Cow<'a, str> {
    if ui.measure(g, text, size, bold) <= max {
        return text.into();
    }
    let available = max - ui.measure(g, "…", size, bold);
    if available <= 0.0 {
        return "".into();
    }
    let offsets: Vec<usize> = text.char_indices().map(|(i, _)| i).skip(1).collect();
    let n = offsets.partition_point(|&i| ui.measure(g, &text[..i], size, bold) <= available);
    let end = if n == 0 { 0 } else { offsets[n - 1] };
    format!("{}…", &text[..end]).into()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn machines(kinds: &[DeviceKind]) -> Vec<Machine> {
        kinds.iter().enumerate().map(|(i, &kind)| Machine {
            label: format!("fixture-{i}"),
            model_id: None,
            kind,
            kasanet: serde_json::Value::Null,
            fresh: false,
            account: None,
        }).collect()
    }

    fn mixed(count: usize) -> Vec<Machine> {
        let kinds: Vec<_> = (0..count).map(|i| [DeviceKind::Laptop, DeviceKind::Mini, DeviceKind::Desktop][i % 3]).collect();
        machines(&kinds)
    }

    /// 한 번에 0.1초까지만 움직이니 여러 번 나눠 끝까지.
    fn settle(room: &mut Room) {
        for _ in 0..20 {
            let at = room.animated_at.unwrap_or_else(Instant::now) + Duration::from_millis(100);
            room.animate(at);
        }
    }

    fn attach_fixture_geometry(layout: &mut Layout) {
        for part in &mut layout.parts { part.bounds = Some(part.model.hull()); }
    }

    fn center(layout: &Layout, frame: &Frame, index: usize) -> Point {
        let part = layout.main_part(layout.device_of(index).unwrap()).unwrap();
        let p = frame.project(part.position + part.bounds.unwrap_or_else(|| part.model.hull()).0);
        (p.x, p.y)
    }

    #[test]
    fn devices_sit_on_their_furniture_inside_the_window() {
        let devices = mixed(9);
        let layout = Layout::new((1280.0, 800.0), &devices, 0);
        let frame = layout.home.frame(layout.window);
        for part in &layout.parts {
            match part.model {
                Model::Laptop | Model::Air | Model::Pro | Model::Monitor => assert_eq!(part.position.y, mesh::DESK_Y),
                Model::Tower => assert_eq!(part.position.y, mesh::DESK_Y),
                Model::Mini | Model::MiniM4 => assert!(part.position.y > mesh::DESK_Y && part.position.x < layout.plan.desk.0),
            }
        }
        for part in &layout.parts {
            let (center, half) = part.bounds.unwrap_or_else(|| part.model.hull());
            for side in [-1.0, 1.0] {
                let p = frame.project(part.position + center + half * side);
                assert!(p.x >= PAD && p.x <= 1280.0 - PAD, "{p}");
                assert!(p.y >= 78.0 && p.y <= 800.0 - BOTTOM, "{p}");
            }
        }
    }

    #[test]
    fn dense_stations_keep_each_device_pickable_as_the_camera_leans() {
        for kind in [DeviceKind::Mini, DeviceKind::MiniM4, DeviceKind::Desktop, DeviceKind::Air, DeviceKind::Pro] {
            let devices = machines(&[kind; PAGE]);
            for window in [(600.0, 400.0), (1920.0, 700.0), (600.0, 900.0)] {
                let mut layout = Layout::new(window, &devices, 0);
                attach_fixture_geometry(&mut layout);
                for look in [Vec2::ONE, -Vec2::ONE] {
                    let frame = layout.home.with_look(look).frame(window);
                    for i in 0..PAGE {
                        let p = center(&layout, &frame, i);
                        assert_eq!(layout.hit(&frame, p), Some(i), "{kind:?} {window:?} {look:?} {p:?}");
                    }
                }
            }
        }
    }

    #[test]
    fn travel_and_drag_use_the_camera_position_and_home_restores_it() {
        let mut room = Room::default();
        room.reflow((1280.0,800.0), &mixed(4));
        let start = room.home_frame().unwrap();
        assert!(room.travel_key(KeyCode::KeyW,true));
        settle(&mut room);
        room.travel_key(KeyCode::KeyW,false);
        let walk = room.home_frame().unwrap();
        assert!(walk.camera.z < start.camera.z - 1.0);
        assert!((walk.camera.y-start.camera.y).abs() < 0.001);
        room.scroll(1.0);
        let dolly = room.home_frame().unwrap();
        assert!((dolly.camera-walk.camera).length() > 1.0);
        room.drag(Some((100.0,200.0)));
        room.pointer(Some((400.0,250.0)));
        let turn = room.home_frame().unwrap();
        let center = (640.0, 400.0);
        let before = dolly.ray(center).unwrap().direction;
        let after = turn.ray(center).unwrap().direction;
        assert!(turn.camera.distance(dolly.camera) < 0.001 && after.distance(before) > 0.2);
        assert!(turn.visible(turn.camera - after).is_none());
        room.stop_navigation();
        room.travel_key(KeyCode::Home,true);
        assert!(room.free.is_none() && room.dragging.is_none());
        assert!(room.home_frame().unwrap().camera.distance(start.camera) < 0.001);
    }

    #[test]
    fn mini_body_and_display_select_the_same_machine() {
        let machines = machines(&[DeviceKind::MiniM4, DeviceKind::MiniM4]);
        let layout = Layout::new((1280.0,800.0), &machines,0);
        for i in 0..2 {
            assert_eq!(layout.main_part(i).unwrap().model, Model::Monitor);
            assert!(layout.parts.iter().any(|p| p.model==Model::MiniM4 && p.device==i));
        }
    }

    #[test]
    fn a_name_only_part_has_no_invisible_pick_volume() {
        let machines = machines(&[DeviceKind::Laptop]);
        let mut layout = Layout::new((800.0, 600.0), &machines, 0);
        layout.parts[0].bounds = None;
        let frame = layout.home.frame(layout.window);
        assert_eq!(layout.hit(&frame, center(&layout, &frame, 0)), None);
        assert_eq!(layout.anchors(&frame).len(), 1);
    }

    #[test]
    fn hover_keeps_the_device_pickable_and_its_screen_and_tag_stationary() {
        let devices = mixed(9);
        let mut room = Room::default();
        room.reflow((1280.0, 800.0), &devices);
        attach_fixture_geometry(room.layout.as_mut().unwrap());
        let frame = room.home_frame().unwrap();
        let layout = room.layout.as_ref().unwrap();
        let anchors = layout.anchors(&frame);
        let focus_view = layout.dive_view(8);
        for i in 0..devices.len() {
            let p = center(room.layout.as_ref().unwrap(), &frame, i);
            room.pointer(Some(p));
            assert_eq!(room.hit(), Some(i), "기기 {i} at {p:?}");
        }
        assert!(room.next_frame(false).is_some());
        settle(&mut room);
        let layout = room.layout.as_ref().unwrap();
        assert_eq!(layout.devices[8].highlight, 1.0);
        assert_eq!(layout.anchors(&frame), anchors);
        assert_eq!(layout.dive_view(8), focus_view);
        assert_eq!(room.hit(), Some(8));
        assert!(room.next_frame(false).is_none());
        assert!(room.pointer(None));
        assert!(room.hit().is_none());
    }

    #[test]
    fn diving_moves_the_camera_to_the_screen_then_reports_arrival() {
        let devices = mixed(4);
        let mut room = Room::default();
        room.reflow((1280.0, 800.0), &devices);
        assert!(room.dive(0));
        assert!(room.hit().is_none() && room.selected().is_none());
        assert!(!room.dive(1));
        let layout = room.layout.as_ref().unwrap();
        let end = layout.dive_view(0).unwrap();
        assert!(end.half < layout.home.half / 4.0);
        let mid = View::mix(layout.home, end, 0.5);
        assert!(mid.half < layout.home.half && mid.half > end.half);
        let screen = end.frame(layout.window).project(end.target);
        assert!((screen.x - 640.0).abs() < 1.0 && (screen.y - 400.0).abs() < 1.0);
        room.dive.as_mut().unwrap().progress = 1.0;
        room.arrived = Some(0);
        assert_eq!(room.take_arrival(), Some(0));
        assert!(room.dive.is_none() && room.take_arrival().is_none());
        room.surface(0);
        assert!(room.next_frame(false).is_some());
        settle(&mut room);
        assert!(room.dive.is_none());
    }

    #[test]
    fn arrows_walk_the_room_by_where_devices_appear() {
        let devices = machines(&[DeviceKind::Laptop, DeviceKind::Laptop, DeviceKind::Mini, DeviceKind::Desktop]);
        let mut room = Room::default();
        room.reflow((1280.0, 800.0), &devices);
        assert!(room.keyboard(KeyCode::ArrowRight, &devices));
        assert_eq!(room.selected(), Some(0));
        assert!(room.keyboard(KeyCode::ArrowRight, &devices));
        assert_eq!(room.selected(), Some(1));
        assert!(room.keyboard(KeyCode::ArrowRight, &devices));
        assert_eq!(room.selected(), Some(3));
        assert!(!room.keyboard(KeyCode::ArrowRight, &devices));
        assert!(room.keyboard(KeyCode::ArrowUp, &devices) || room.keyboard(KeyCode::ArrowLeft, &devices));
    }

    #[test]
    fn pages_hold_ten_devices_and_keep_the_keyboard_device_on_resize() {
        let devices = mixed(23);
        let mut room = Room::default();
        room.reflow((1280.0, 800.0), &devices);
        assert_eq!(room.layout.as_ref().unwrap().devices.len(), PAGE);
        assert!(room.turn_page(2, &devices));
        assert_eq!(room.page, 2);
        assert_eq!(room.layout.as_ref().unwrap().devices.len(), 3);
        room.keyboard = Some(devices[12].label.clone());
        room.reflow((600.0, 400.0), &devices);
        assert_eq!(room.page, 1);
        assert_eq!(room.selected(), Some(12));
        room.reflow((600.0, 400.0), &devices[..3]);
        assert_eq!(room.page, 0);
        assert_eq!(room.selected(), None);
    }
}
