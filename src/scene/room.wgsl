// 재질 번호는 mesh.rs `mat` 과 같다. 빛은 선형 공간에서 셈해 sRGB 로 내보낸다(창 표면은 sRGB 접미가 없는 형식이다).

struct Scene {
    clip_from_world: mat4x4<f32>,
    eye: vec4<f32>,
    // x 시간(초), y 낮 0..1, z 해 질 녘 0..1, w 실내등 0..1
    params: vec4<f32>,
    // 모형의 가운데(xyz)와 반 길이(w)를 받아 빛의 범위를 맞춘다.
    lamp: vec4<f32>,
    desk_light: vec4<f32>,
    // 창 가운데(xyz)와 폭(w) — 높이는 WINDOW_H
    window: vec4<f32>,
    // 러그 가운데(xy = 세계 xz)와 반 크기(zw)
    rug: vec4<f32>,
};

const WINDOW_H: f32 = 4.4;
const WALL_H: f32 = 13.5;

@group(0) @binding(0) var<uniform> scene: Scene;

struct Vertex {
    @location(0) point: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) material: vec4<f32>,
    @location(3) position: vec3<f32>,
    @location(4) lift: f32,
    @location(5) glow: f32,
};

struct Surface {
    @builtin(position) clip: vec4<f32>,
    @location(0) world: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) local: vec3<f32>,
    @location(3) @interpolate(flat) material: vec4<f32>,
    @location(4) @interpolate(flat) lift: f32,
    @location(5) @interpolate(flat) glow: f32,
};

@vertex
fn project(v: Vertex) -> Surface {
    // 빛 웅덩이는 들리지 않고 바닥(책상)에 남는다.
    let rise = select(v.lift * 0.45, 0.0, v.material.w == 6.0);
    let world = v.position + v.point + vec3<f32>(0.0, rise, 0.0);
    return Surface(scene.clip_from_world * vec4<f32>(world, 1.0), world, v.normal, v.point, v.material, v.lift, v.glow);
}

fn lin(c: vec3<f32>) -> vec3<f32> {
    return pow(max(c, vec3<f32>(0.0)), vec3<f32>(2.2));
}

fn hash(p: vec2<f32>) -> f32 {
    return fract(sin(dot(p, vec2<f32>(127.1, 311.7))) * 43758.5453);
}

fn noise(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    return mix(mix(hash(i), hash(i + vec2<f32>(1.0, 0.0)), u.x), mix(hash(i + vec2<f32>(0.0, 1.0)), hash(i + vec2<f32>(1.0, 1.0)), u.x), u.y);
}

fn fbm(p: vec2<f32>) -> f32 {
    return noise(p) * 0.55 + noise(p * 2.1) * 0.3 + noise(p * 4.3) * 0.15;
}

// 창밖 — 낮·노을·밤 하늘과 흐르는 구름, 밤엔 별. sRGB 로 돌려준다.
fn sky(t: f32, u: f32, v: f32, width: f32) -> vec3<f32> {
    let day = scene.params.y;
    let dusk = scene.params.z;
    let x = u * width / WINDOW_H;
    let night = (1.0 - day) * (1.0 - dusk);
    let horizon = mix(vec3<f32>(0.23, 0.25, 0.44), vec3<f32>(0.88, 0.92, 0.95), day);
    let zenith = mix(vec3<f32>(0.05, 0.08, 0.23), vec3<f32>(0.29, 0.59, 0.87), day);
    var col = mix(horizon, zenith, v);
    col = mix(col, mix(vec3<f32>(1.0, 0.61, 0.46), vec3<f32>(0.29, 0.26, 0.53), v), dusk);
    let moon_distance = length(vec2<f32>((u - 0.72) * width / WINDOW_H, v - 0.73));
    col += vec3<f32>(0.11, 0.16, 0.29) * exp(-moon_distance * 8.0) * night;
    let moon = (1.0 - smoothstep(0.048, 0.053, moon_distance)) * night;
    col = mix(col, vec3<f32>(0.88, 0.92, 1.0), moon);
    let grid = vec2<f32>(x, v) * 33.0;
    let star = step(0.981, hash(floor(grid))) * (1.0 - smoothstep(0.03, 0.16, length(fract(grid) - vec2<f32>(0.5))));
    col += vec3<f32>(0.58, 0.65, 0.83) * star * night;
    let clouds = smoothstep(0.49, 0.78, fbm(vec2<f32>(x * 0.8 + t * 0.003, v * 4.0 + 3.0)));
    let cloud_col = mix(vec3<f32>(0.27, 0.29, 0.47), vec3<f32>(0.98, 0.99, 1.0), day) + vec3<f32>(0.37, 0.16, 0.06) * dusk;
    col = mix(col, cloud_col, clouds * (0.41 + 0.38 * day));
    for (var layer = 0; layer < 3; layer += 1) {
        let depth = f32(layer) * 0.5;
        let ridge = 0.31 - 0.17 * depth + (fbm(vec2<f32>(x * 1.8 + 5.0 * depth, 4.2)) - 0.5) * 0.35;
        let hill = mix(vec3<f32>(0.16, 0.20, 0.36), vec3<f32>(0.43, 0.61, 0.62), day)
            * (1.0 - 0.51 * depth) + vec3<f32>(0.14, 0.04, 0.08) * dusk;
        col = mix(col, hill, 1.0 - smoothstep(ridge, ridge + 0.007, v));
    }
    return col;
}

fn plank(world: vec3<f32>, base: vec3<f32>) -> vec3<f32> {
    let row = floor(world.z / 1.25);
    let offset = hash(vec2<f32>(row, 3.0)) * 9.0;
    let board = floor((world.x + offset) / 7.0);
    let tint = 0.84 + 0.24 * hash(vec2<f32>(row, board));
    let grain = 0.93 + 0.07 * sin((world.x + offset) * 2.6 + fbm(vec2<f32>(world.x * 0.5, row * 3.0)) * 7.0);
    let fx = fract((world.x + offset) / 7.0);
    let fz = fract(world.z / 1.25);
    let seam = max(1.0 - smoothstep(0.0, 0.02, min(fz, 1.0 - fz)), 1.0 - smoothstep(0.0, 0.006, min(fx, 1.0 - fx)));
    return base * tint * grain * (1.0 - seam * 0.45);
}

fn lamp_color() -> vec3<f32> {
    return vec3<f32>(1.0, 0.53, 0.28);
}

fn desk_light_color() -> vec3<f32> {
    let sky_tone = mix(vec3<f32>(0.27, 0.34, 1.0), vec3<f32>(0.26, 0.69, 1.0), scene.params.y);
    return mix(sky_tone, vec3<f32>(1.0, 0.29, 0.37), scene.params.z);
}

fn line_light(delta: vec3<f32>, n: vec3<f32>, reach: f32) -> f32 {
    let d2 = max(dot(delta, delta), 0.001);
    return max(dot(n, delta) * inverseSqrt(d2), 0.0) * reach / (reach + d2);
}

fn light(world: vec3<f32>, n: vec3<f32>, base: vec3<f32>) -> vec3<f32> {
    let day = scene.params.y;
    let dusk = scene.params.z;
    let indoor = scene.params.w;
    let sky_col = mix(vec3<f32>(0.30, 0.34, 0.52), vec3<f32>(1.0, 0.97, 0.92), day) + vec3<f32>(0.30, 0.12, 0.0) * dusk;
    let hemi = 0.6 + 0.4 * n.y;
    var col = base * sky_col * (0.2 + 0.24 * day) * hemi;
    // 창에서 들어오는 빛 — 창 쪽을 보는 면과 창 가까운 곳이 밝다.
    let w = scene.window.xyz + vec3<f32>(0.0, 0.0, 2.0);
    let to_window = w - world;
    let dw = length(to_window);
    col = col + base * sky_col * max(dot(n, to_window / dw), 0.0) * (0.06 + 0.55 * day) * (12.0 / (12.0 + dw));
    var lamp = scene.lamp.xyz - world;
    lamp.y -= clamp(lamp.y, -scene.lamp.w, scene.lamp.w);
    var strip = scene.desk_light.xyz - world;
    strip.x -= clamp(strip.x, -scene.desk_light.w, scene.desk_light.w);
    col += base * lamp_color() * line_light(lamp, n, 20.0) * (0.15 + 0.64 * indoor);
    col += base * desk_light_color() * line_light(strip, n, 6.5) * (0.08 + 0.62 * indoor);
    return col;
}

fn out(c: vec3<f32>, a: f32) -> vec4<f32> {
    // 부드러운 어깨 톤매핑 뒤 sRGB.
    let mapped = c / (1.0 + c * 0.35) * 1.18;
    return vec4<f32>(pow(clamp(mapped, vec3<f32>(0.0), vec3<f32>(1.0)), vec3<f32>(1.0 / 2.2)), a);
}

@fragment
fn shade(s: Surface) -> @location(0) vec4<f32> {
    let mode = s.material.w;
    let base = lin(s.material.rgb);
    let n = normalize(s.normal + vec3<f32>(0.0, 1e-5, 0.0));
    let t = scene.params.x;
    if mode == 1.0 {
        return out(base * (0.5 + 1.2 * s.glow), 1.0);
    }
    if mode == 2.0 {
        return out(light(s.world, n, plank(s.world, base)), 1.0);
    }
    if mode == 3.0 {
        let stripe = 0.965 + 0.035 * step(0.5, fract((s.world.x + s.world.z) * 0.7));
        let top = 1.0 - smoothstep(WALL_H - 3.0, WALL_H, s.world.y) * 0.18;
        return out(light(s.world, n, base * stripe) * top, 1.0);
    }
    if mode == 4.0 {
        // 켜진 화면은 푸른빛, 꺼진 화면은 유리 반사만.
        let on = s.glow;
        let grad = 0.8 + 0.2 * smoothstep(-0.6, 1.2, s.local.y - s.local.x * 0.2);
        let lit = base * grad * 1.3;
        let off = lin(vec3<f32>(0.07, 0.075, 0.09)) + 0.03 * smoothstep(0.3, 1.2, s.local.x * 0.3 + s.local.y * 0.5);
        return out(mix(off, lit, on), 1.0);
    }
    if mode == 5.0 {
        let r = length(s.normal.xz);
        return vec4<f32>(0.0, 0.0, 0.0, 0.32 * (1.0 - smoothstep(0.25, 1.0, r)));
    }
    if mode == 6.0 {
        let r = length(s.normal.xz);
        return vec4<f32>(s.material.rgb, 0.5 * s.lift * (1.0 - smoothstep(0.15, 1.0, r)));
    }
    if mode == 7.0 {
        let w = scene.window;
        let u = (s.world.x - (w.x - w.w * 0.5)) / w.w;
        let v = clamp((s.world.y - (w.y - WINDOW_H * 0.5)) / WINDOW_H, 0.0, 1.0);
        return vec4<f32>(sky(t, u, v, w.w), 1.0);
    }
    if mode == 8.0 {
        let q = abs(s.world.xz - scene.rug.xy) / scene.rug.zw;
        let edge = max(q.x, q.y);
        let band = step(0.78, edge) * step(edge, 0.86) + step(0.62, edge) * step(edge, 0.65);
        let weave = 0.95 + 0.05 * fbm(s.world.xz * 6.0);
        let tone = mix(base, lin(vec3<f32>(0.92, 0.84, 0.70)), band * 0.75) * weave;
        return out(light(s.world, n, tone), 1.0);
    }
    if mode == 9.0 {
        return out(lamp_color() * (0.25 + 0.95 * scene.params.w), 1.0);
    }
    if mode == 10.0 {
        let tone = base * (0.8 + 0.4 * noise(s.world.xy * 2.0));
        return out(light(s.world, n, tone), 1.0);
    }
    if mode == 12.0 {
        return out(desk_light_color() * (0.35 + 0.85 * scene.params.w), 1.0);
    }
    var tone = base;
    if mode == 11.0 {
        tone = base * (0.9 + 0.1 * sin(s.world.x * 2.2 + s.world.z * 0.4 + fbm(s.world.zy * 1.3) * 5.0));
    }
    var lit = light(s.world, n, tone);
    let facing = max(dot(n, scene.eye.xyz), 0.0);
    lit = lit + lin(vec3<f32>(1.0, 0.84, 0.62)) * pow(1.0 - facing, 3.0) * 0.18 * s.lift;
    return out(lit, 1.0);
}
