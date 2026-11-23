// 재질 번호는 mesh.rs `mat` 과 같다. 빛은 선형 공간에서 셈해 sRGB 로 내보낸다(창 표면은 sRGB 접미가 없는 형식이다).

struct Scene {
    clip_from_world: mat4x4<f32>,
    sun_clip: mat4x4<f32>,
    eye: vec4<f32>,
    camera: vec4<f32>,
    lighting: vec4<f32>,
    viewport: vec4<f32>,
    bench: vec4<f32>,
};

@group(0) @binding(0) var<uniform> scene: Scene;
@group(0) @binding(1) var panorama: texture_2d<f32>;
@group(0) @binding(2) var linear_sampler: sampler;

@group(0) @binding(3) var products: texture_2d_array<f32>;

@group(1) @binding(0) var shadow_map: texture_depth_2d;
@group(1) @binding(1) var compare_depth: sampler_comparison;

@vertex
fn project_sky(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    let vertices = array(vec2<f32>(-1.0, -1.0), vec2<f32>(3.0, -1.0), vec2<f32>(-1.0, 3.0));
    return vec4<f32>(vertices[i], 1.0, 1.0);
}

fn orbit(uv: vec2<f32>) -> vec3<f32> {
    return textureSampleLevel(panorama, linear_sampler, uv, 0.0).rgb;
}

@fragment
fn horizon(@builtin(position) pixel: vec4<f32>) -> @location(0) vec4<f32> {
    let uv = pixel.xy / scene.viewport.xy;
    let dimensions = vec2<f32>(textureDimensions(panorama));
    let aspect = (scene.viewport.x / scene.viewport.y) / (dimensions.x / dimensions.y);
    let cover = min(vec2<f32>(aspect, 1.0 / aspect), vec2<f32>(1.0));
    let offset = vec2<f32>(scene.eye.x - 0.13, 0.22 - scene.eye.y) * 0.09;
    let photo = orbit((uv - 0.5) * cover * 0.96 + 0.5 + offset);
    let night = vec3<f32>(0.60, 0.77, 0.97);
    let dusk = vec3<f32>(1.12, 0.88, 0.85);
    let tint = mix(mix(night, vec3<f32>(1.0), scene.lighting.x), dusk, scene.lighting.y);
    let vignette = 1.0 - 0.24 * dot(uv - 0.5, uv - 0.5);
    return vec4<f32>(photo * tint * vignette, 1.0);
}

struct Vertex {
    @location(0) point: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) material: vec4<f32>,
    @location(3) position: vec3<f32>,
    @location(4) lift: f32,
    @location(5) glow: f32,
    @location(6) uv: vec2<f32>,
};

struct Surface {
    @builtin(position) clip: vec4<f32>,
    @location(0) world: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) local: vec3<f32>,
    @location(3) @interpolate(flat) material: vec4<f32>,
    @location(4) @interpolate(flat) lift: f32,
    @location(5) @interpolate(flat) glow: f32,
    @location(6) uv: vec2<f32>,
};

@vertex
fn project_shadow(v: Vertex) -> @builtin(position) vec4<f32> {
    let world = v.position + v.point + vec3<f32>(0.0, v.lift * 0.45, 0.0);
    return scene.sun_clip * vec4<f32>(world, 1.0);
}

@vertex
fn project(v: Vertex) -> Surface {
    // 빛 웅덩이는 들리지 않고 바닥(책상)에 남는다.
    let rise = select(v.lift * 0.45, 0.0, v.material.w == 6.0);
    let world = v.position + v.point + vec3<f32>(0.0, rise, 0.0);
    return Surface(scene.clip_from_world * vec4<f32>(world, 1.0), world, v.normal, v.point, v.material, v.lift, v.glow, v.uv);
}

fn lin(c: vec3<f32>) -> vec3<f32> {
    return pow(max(c, vec3<f32>(0.0)), vec3<f32>(2.2));
}

fn accent() -> vec3<f32> {
    return mix(mix(vec3<f32>(0.16, 0.66, 0.98), vec3<f32>(0.36, 0.92, 0.90), scene.lighting.x),
               vec3<f32>(1.0, 0.34, 0.13), scene.lighting.y);
}

fn light(s: Surface, n: vec3<f32>, base: vec3<f32>) -> vec3<f32> {
    let sun = normalize(vec3<f32>(0.76, 0.45, -0.42));
    let bounce = normalize(vec3<f32>(-0.54, 0.52, 0.61));
    let gold = vec3<f32>(1.0, 0.64, 0.40);
    let blue = vec3<f32>(0.42, 0.69, 0.94);
    let clip = scene.sun_clip * vec4<f32>(s.world + n * 0.012, 1.0);
    let uv = clip.xy * vec2<f32>(0.5, -0.5) + 0.5;
    var visibility = 1.0;
    if all(uv > vec2<f32>(0.0)) && all(uv < vec2<f32>(1.0)) {
        visibility = textureSampleCompareLevel(shadow_map, compare_depth, uv, clip.z);
    }
    let incident = vec3<f32>(0.25, 0.32, 0.39) * (0.85 + 0.20 * n.y)
        + gold * max(dot(n, sun), 0.0) * (0.95 + 0.65 * scene.lighting.x) * visibility
        + blue * max(dot(n, bounce), 0.0) * 0.78;
    let strip = vec3<f32>(clamp(s.world.x, scene.bench.x, scene.bench.y), scene.bench.z + 0.20, -2.5) - s.world;
    let spill = max(dot(n, strip), 0.0) / (0.8 + dot(strip, strip));
    let view_dir = normalize(scene.camera.xyz - s.world);
    let reflected = reflect(-view_dir, n);
    let sweep = pow(max(dot(reflected, normalize(vec3<f32>(0.63, 0.23, -0.75))), 0.0), 6.0);
    let sky_reflection = mix(blue * 0.045, gold * 0.23, sweep);
    let edge = pow(1.0 - clamp(dot(n, view_dir), 0.0, 1.0), 3.0);
    let metal = pow(max(dot(n, normalize(sun + view_dir)), 0.0), 36.0);
    return base * (incident + accent() * spill * 1.8)
        + (sky_reflection + base * 0.16) * (0.12 + edge) + gold * metal * 0.20 * visibility;
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
    if mode >= 32.0 {
        let tex = textureSampleLevel(products, linear_sampler, s.uv, i32(mode - 32.0) % 32, 0.0).rgb;
        if mode >= 64.0 { return vec4<f32>(s.material.rgb * tex * (0.65 + 0.35 * s.glow), 1.0); }
        return out(light(s, n, base * lin(tex)), 1.0);
    }
    if mode == 6.0 {
        let r = length(s.normal.xz);
        let color = mix(vec3<f32>(0.006, 0.012, 0.020), accent(), s.lift);
        return vec4<f32>(color, 0.62 * (1.0 - smoothstep(0.26, 1.0, r)));
    }
    if mode == 9.0 {
        return vec4<f32>(vec3<f32>(1.0, 0.79, 0.53) * (0.84 + 0.14 * scene.lighting.z), 1.0);
    }
    if mode == 12.0 {
        return vec4<f32>(mix(accent(), vec3<f32>(1.0), 0.23), 1.0);
    }
    // 브러시 결은 화면 한 픽셀보다 좁아지면 평균값으로 수렴시킨다.
    let grain_uv = (s.world.x + s.world.z + s.world.y * 0.37) * 138.0;
    let fine = sin(grain_uv) * (1.0 - smoothstep(0.8, 3.1, fwidth(grain_uv)));
    var tone = base * (1.0 + 0.035 * fine);
    if mode == 2.0 {
        let seam = abs(fract(s.world.xz * vec2<f32>(0.2, 0.34)) - 0.5);
        let joint = smoothstep(0.494, 0.499, max(seam.x, seam.y));
        tone *= 1.0 - 0.42 * joint;
    }
    return out(light(s, n, tone), 1.0);
}
