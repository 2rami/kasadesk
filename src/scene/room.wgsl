// 재질 번호는 mesh.rs `mat` 과 같다. 빛은 선형 공간에서 셈해 sRGB 로 내보낸다(창 표면은 sRGB 접미가 없는 형식이다).

const BARE_SCREEN = 160.0;
struct PreviewScreen { rect: vec4f, size: vec4f };

struct Scene {
    clip_from_world: mat4x4<f32>,
    sun_clip: mat4x4<f32>,
    world_from_clip: mat4x4<f32>,
    camera: vec4<f32>,
    lighting: vec4<f32>,
    weather: vec4f,
    viewport: vec4<f32>,
    bench: vec4<f32>,
    landscape: vec4<f32>,
    windows: vec4<f32>,
    screens: array<PreviewScreen,10>,
    fan_colors: array<vec4<f32>,12>,
};

@group(0) @binding(0) var<uniform> scene: Scene;
@group(0) @binding(1) var panorama: texture_2d<f32>;
@group(0) @binding(2) var linear_sampler: sampler;

@group(0) @binding(3) var products: texture_2d_array<f32>;

@group(0) @binding(4) var preview_image: texture_2d_array<f32>;

@group(0) @binding(5) var kraken_lcd: texture_2d<f32>;

@group(1) @binding(0) var shadow_map: texture_depth_2d;
@group(1) @binding(1) var compare_depth: sampler_comparison;

@vertex
fn project_sky(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    let vertices = array(vec2<f32>(-1.0, -1.0), vec2<f32>(3.0, -1.0), vec2<f32>(-1.0, 3.0));
    return vec4<f32>(vertices[i], 0.0, 1.0);
}

fn panorama_uv(direction: vec3f) -> vec2f {
    let horizontal = length(direction.xz);
    let heading = atan2(direction.x, -direction.z);
    let elevation = atan2(direction.y, max(horizontal, 0.00001));
    // Allocate more texels to the river-facing part of the painting and less to the rear and poles.
    let lon = heading + 0.62 * sin(heading);
    let lat = elevation + 0.27 * sin(2.0 * elevation) + 0.36 * cos(elevation);
    return vec2f(0.5 + lon * (0.5 / 3.14159265), 0.5 - lat / 3.14159265);
}

// Identifiers are shared with weather::Sky. The remaining channels carry intensity, wind and gusts.
const CLEAR = 0u;
const CLOUD = 1u;
const FOG = 2u;
const RAIN = 3u;
const SNOW = 4u;
const STORM = 5u;
const HAIL = 6u;
const SUNSHOWER = 7u;

fn plate_tap(coord: vec2f) -> vec4f {
    let uv = vec2f(fract(coord.x), clamp(coord.y, 0.0, 1.0));
    let color = textureSampleLevel(panorama, linear_sampler, uv, 0.0);
    let seam = min(uv.x, 1.0 - uv.x);
    if seam < 0.018 {
        let other = textureSampleLevel(panorama, linear_sampler, vec2f(1.0 - uv.x, uv.y), 0.0);
        return mix((color + other) * 0.5, color, smoothstep(0.0, 0.018, seam));
    }
    return color;
}

fn soft_plate(uv: vec2f, kind: u32) -> vec4f {
    // Radius is in panorama texels, so resizing the window leaves the focus distance unchanged.
    let radius = select(0.90, 2.25, kind == FOG);
    let d = vec2f(radius) / vec2f(textureDimensions(panorama));
    let a = vec2f(d.x, d.y * 0.5);
    let b = vec2f(d.x * -0.5, d.y);
    return 0.25 * (plate_tap(uv+a) + plate_tap(uv-a) + plate_tap(uv+b) + plate_tap(uv-b));
}

fn river_sky(uv: vec2f) -> vec3f {
    let kind = u32(scene.weather.x);
    let paint = soft_plate(uv, kind);
    if kind == SUNSHOWER { return paint.rgb; }
    let t = scene.lighting.w;
    let day = scene.lighting.x;
    let amount = scene.weather.y;
    let wind = scene.weather.z;
    let luma = dot(paint.rgb, vec3f(0.24,0.61,0.15));
    let near_horizon = smoothstep(0.17,0.49,uv.y);
    let blue = mix(vec3f(0.22,0.52,0.81), vec3f(0.75,0.86,0.92), near_horizon);
    let night = mix(vec3f(0.028,0.054,0.13), vec3f(0.22,0.29,0.41), near_horizon);
    let fair = mix(mix(night,blue,day), vec3f(0.97,0.68,0.52), 0.59 * near_horizon * scene.lighting.y);
    if kind == CLEAR {
        let sun = (uv - vec2f(0.46, 0.34)) * vec2f(2.0,1.0);
        let glow = exp(-length(sun) * 14.0);
        let disk = 1.0 - smoothstep(0.008,0.012,length(sun));
        let angle = atan2(sun.y, sun.x + 0.00001);
        let rays = 0.5 + 0.5*sin(9.0*angle + t*0.12 + sin(4.0*angle - t*0.09));
        let cirrus = smoothstep(0.74, 0.94, luma) * 0.19;
        var sky = mix(fair,vec3f(0.96,0.96,0.88), clamp((cirrus + (0.12+0.12*rays)*glow + 0.77*disk)*day,0.0,1.0));
        if day < 1.0 {
            let grid = uv * vec2f(180.0,90.0);
            let cell = floor(grid);
            let star = grain(cell + 61.0);
            let offset = fract(grid) - vec2f(0.18+0.64*star,0.18+0.64*grain(cell+3.0));
            let spark = (1.0-smoothstep(0.016,0.065,length(offset))) * smoothstep(0.72,0.85,star);
            let crescent = disk*smoothstep(0.008,0.011,length(sun-vec2f(0.005,-0.004)));
            sky += vec3f(0.79,0.85,1.0)*(1.0-day)*(0.58*crescent + spark*(0.46+0.12*sin(t*0.8+star*97.0)));
        }
        if amount > 0.0 {
            let bank = cloudBank(uv, t*0.17, amount, wind);
            sky = sky*(1.0-bank.a*amount) + bank.rgb*amount;
        }
        let ground = mix(paint.rgb, vec3f(luma*0.91,luma,luma*1.04), 0.21);
        return mix(ground,sky,paint.a);
    }
    var cover = 0.78;
    if kind == CLOUD { cover = amount; }
    if kind == STORM || kind == HAIL { cover = 1.0; }
    let night_light = mix(0.46,1.0,day);
    let bank = cloudBank(uv, t*0.17, amount, wind);
    let cloud_color = mix(vec3f(0.24,0.31,0.44),vec3f(0.86,0.88,0.92),luma) * night_light;
    var sky = mix(mix(paint.rgb,fair,0.14), cloud_color, 0.3 + 0.7*cover);
    sky = sky*(1.0-bank.a) + bank.rgb * night_light;
    let storm = kind == STORM || kind == HAIL;
    sky *= select(1.0,0.69,storm);
    let ground = mix(paint.rgb,vec3f(luma*0.84,luma*0.95,luma*1.06),0.25 + 0.48*cover)
                 * select(1.0,0.80,storm);
    var color = mix(ground, sky, paint.a);
    if kind == FOG {
        let q = vec2f(uv.x * 2.7 - t*(0.018+0.026*wind), uv.y * 5.0);
        let fog = 0.35 + 0.45 * smoothstep(0.27,0.70,cloudField(q));
        color = mix(color,vec3f(0.76,0.83,0.87)*night_light,fog);
    }
    return color;
}

fn landscape_direction(ray: vec3f) -> vec3f {
    let offset = scene.camera.xyz - scene.landscape.xyz;
    let radius = scene.landscape.w;
    // Wide room layouts and portrait windows can put the camera farther from the glass.
    // Keep its projection inside the sphere so every ray has a forward intersection.
    let scaled = offset / max(radius, length(offset) / 0.55);
    let along = dot(scaled, ray);
    let distance = sqrt(1.0 - dot(scaled, scaled) + along * along) - along;
    let city = scaled + ray * distance;
    return mix(city, ray, smoothstep(0.03, 0.31, ray.y));
}

fn moving_clouds(uv: vec2f) -> vec2f {
    let phase = scene.lighting.w * (6.2831853 / 44.0);
    let amount = (1.0 - smoothstep(0.33, 0.47, uv.y)) * smoothstep(0.04, 0.18, uv.y);
    let drift = vec2f(sin(phase) * 0.010, sin(phase + uv.x * 6.2831853) * 0.0025);
    return uv + drift * amount;
}

fn rain_on_glass(ray: vec3f) -> vec3f {
    let camera = scene.camera.xyz;
    let z = scene.bench.w;
    if ray.z < -0.0001 {
        let distance = (z - camera.z) / ray.z;
        let hit = camera + ray * distance;
        if distance > 0.0 && hit.x >= scene.windows.x && hit.x <= scene.windows.y && hit.y <= scene.windows.z {
            return vec3f(hit.x, hit.y, 1.0);
        }
    }
    if abs(ray.x) > 0.0001 {
        let edge = select(scene.windows.x, scene.windows.y, ray.x > 0.0);
        let distance = (edge - camera.x) / ray.x;
        let hit = camera + ray * distance;
        if distance > 0.0 && hit.z >= z && hit.z <= scene.windows.w && hit.y <= scene.windows.z {
            // Rain coordinates continue around the corner so the wind has the same direction.
            return vec3f(edge + sign(ray.x) * (hit.z - z), hit.y, 1.0);
        }
    }
    return vec3f(0.0);
}

@fragment
fn horizon(@builtin(position) pixel: vec4<f32>) -> @location(0) vec4<f32> {
    let uv = pixel.xy / scene.viewport.xy;
    let ndc = uv * vec2f(2.0, -2.0) + vec2f(-1.0, 1.0);
    let point = scene.world_from_clip * vec4f(ndc, 0.05, 1.0);
    let ray = normalize(point.xyz / point.w - scene.camera.xyz);
    let sky_uv = moving_clouds(panorama_uv(landscape_direction(ray)));
    var photo = river_sky(sky_uv);
    let glass = rain_on_glass(ray);
    if glass.z > 0.0 {
        let p = vec2f(glass.x, 3.07 - glass.y) * 31.0;
        let t = scene.lighting.w;
        let kind = u32(scene.weather.x);
        let amount = scene.weather.y;
        let wind = scene.weather.z;
        if kind == RAIN || kind == STORM || kind == HAIL || kind == SUNSHOWER {
            let rain = falling(p,t,wind,amount,0.08) + falling(p,t,wind,amount,0.52) + falling(p,t,wind,amount,0.93);
            let spray = impact(p,t,amount,wind,23.0);
            let drop = bead(p*0.74,amount*0.60,47.0);
            let wet = clamp(spray*0.65 + rain*0.66 + drop.y*0.56,0.0,0.85);
            if drop.x + drop.y > 0.0 {
                let refracted = river_sky(sky_uv + vec2f(drop.y,-drop.x)*0.004);
                photo = mix(photo,refracted,min(1.0,5.0*(drop.x+drop.y)));
            }
            photo = mix(photo*(1.0-drop.x),vec3f(0.84,0.93,1.0),wet);
        }
        if kind == SNOW || kind == HAIL {
            let ice = kind == HAIL;
            let speed = select(52.0,196.0,ice);
            let far = flakes(p,t,wind,16.0,speed*0.56,amount,1.15,ice);
            let near = flakes(p+19.0,t,wind,32.0,speed,amount,2.70,ice);
            photo = mix(photo,vec3f(0.94,0.97,1.0),min(0.95,0.38*far + 0.82*near));
        }
        if kind == STORM || kind == HAIL {
            let cycle = t/6.8 + 0.14;
            let age = fract(cycle);
            let pulse = exp(-22.0*age)*smoothstep(0.0,0.022,age);
            let origin = 0.23 + 0.51*grain(vec2f(floor(cycle),49.0));
            let bend = 0.055*(haze(vec2f(sky_uv.y*43.0,floor(cycle)))-0.5);
            let bolt = (1.0-smoothstep(0.0004,0.0022,abs(sky_uv.x-origin-bend)))
                       *smoothstep(0.21,0.32,sky_uv.y)*(1.0-smoothstep(0.39,0.46,sky_uv.y));
            photo += vec3f(0.83,0.85,1.0)*pulse*(0.50*bolt + 0.065);
        }
        if scene.weather.w > 0.0 {
            let gust = windScene(p,t,0.0,wind,vec3f(0.86,0.91,0.96)) * scene.weather.w * 0.50;
            photo = photo*(1.0-gust.a) + gust.rgb;
        }
    }
    let night = vec3f(0.44, 0.57, 0.84);
    let dusk = vec3f(1.07, 0.85, 0.91);
    let tint = mix(mix(night, vec3<f32>(1.0), scene.lighting.x), dusk, scene.lighting.y);
    let vignette = 1.0 - 0.24 * dot(uv - 0.5, uv - 0.5);
    return vec4<f32>(photo * tint * vignette, 1.0);
}

struct Vertex {
    @location(0) point: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) material: vec4<f32>,
    @location(3) position: vec3<f32>,
    @location(4) highlight: f32,
    @location(5) glow: f32,
    @location(6) uv: vec2<f32>,
    @location(7) preview: f32,
    @location(8) rgb: f32,
};

struct Surface {
    @builtin(position) clip: vec4<f32>,
    @location(0) world: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) local: vec3<f32>,
    @location(3) @interpolate(flat) material: vec4<f32>,
    @location(4) @interpolate(flat) highlight: f32,
    @location(5) @interpolate(flat) glow: f32,
    @location(6) uv: vec2<f32>,
    @location(7) @interpolate(flat) preview: f32,
    @location(8) @interpolate(flat) rgb: f32,
};

@vertex
fn project_shadow(v: Vertex) -> @builtin(position) vec4<f32> {
    // The paper strip is too thin for the cached sunlight map to resolve.
    if v.material.w == 3.0 { return vec4f(2.0, 2.0, 2.0, 1.0); }
    let world = v.position + v.point;
    return scene.sun_clip * vec4<f32>(world, 1.0);
}

@vertex
fn project(v: Vertex) -> Surface {
    var world = v.position + v.point;
    if v.material.w == 3.0 {
        let flutter = sin(scene.lighting.w * 2.1) + 0.35 * sin(scene.lighting.w * 4.3 - 2.4 * v.uv.y);
        world.x += v.uv.y * v.uv.y * 0.40 * flutter;
        world.z += v.uv.y * (v.uv.x - 0.5) * 0.21 * flutter;
    }
    return Surface(scene.clip_from_world * vec4<f32>(world, 1.0), world, v.normal, v.point, v.material, v.highlight, v.glow, v.uv, v.preview, v.rgb);
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
    let gold = vec3<f32>(1.0, 0.78, 0.59);
    let blue = vec3<f32>(0.40, 0.66, 1.0);
    let clip = scene.sun_clip * vec4<f32>(s.world + n * 0.012, 1.0);
    let uv = clip.xy * vec2<f32>(0.5, -0.5) + 0.5;
    var visibility = 1.0;
    if all(uv > vec2<f32>(0.0)) && all(uv < vec2<f32>(1.0)) {
        visibility = textureSampleCompareLevel(shadow_map, compare_depth, uv, clip.z);
    }
    let sky_kind = u32(scene.weather.x);
    let overcast = select(0.0, scene.weather.y, sky_kind != CLEAR && sky_kind != SUNSHOWER);
    let incident = vec3<f32>(0.35, 0.42, 0.55) * (0.85 + 0.20 * n.y + 0.24 * overcast)
        + gold * max(dot(n, sun), 0.0) * (0.95 + 0.65 * scene.lighting.x) * visibility * (1.0 - 0.65*overcast)
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

fn preview_tap(pixel: vec2f, size: vec2f, layer: u32) -> vec3f {
    let coord = clamp(pixel, vec2f(0.5), size - 0.5) / vec2f(textureDimensions(preview_image));
    return textureSampleLevel(preview_image, linear_sampler, coord, i32(layer), 0.0).rgb;
}

fn preview_color(layer: u32, xy: vec2f, dx: vec2f, dy: vec2f) -> vec4f {
    let rect = scene.screens[layer].rect;
    let size = scene.screens[layer].size.xy;
    let ratio = (rect.z / rect.w) * (size.y / size.x);
    let stretch = max(vec2f(ratio, 1.0/ratio), vec2f(1.0));
    let uv = (vec2f(xy.x, 1.0-xy.y) - 0.5) * stretch + 0.5;
    if any(uv < vec2f(0.0)) || any(uv > vec2f(1.0)) { return vec4f(0.012,0.014,0.018,1.0); }
    let pixel = uv * (size - 1.0) + 0.5;
    let scale = stretch * (size - 1.0) * vec2f(1.0,-1.0);
    let x = dx * scale;
    let y = dy * scale;
    if max(dot(x,x), dot(y,y)) > 2.25 {
        let a = 0.25 * (x+y);
        let b = 0.25 * (x-y);
        return vec4f(0.25 * (preview_tap(pixel+a,size,layer) + preview_tap(pixel-a,size,layer)
                           + preview_tap(pixel+b,size,layer) + preview_tap(pixel-b,size,layer)), 1.0);
    }
    return vec4f(preview_tap(pixel,size,layer), 1.0);
}

@fragment
fn shade(s: Surface) -> @location(0) vec4<f32> {
    let mode = s.material.w;
    let base = lin(s.material.rgb);
    let n = normalize(s.normal + vec3<f32>(0.0, 1e-5, 0.0));
    let uv_dx = dpdx(s.uv);
    let uv_dy = dpdy(s.uv);
    let local_dx = dpdx(s.local.xy);
    let local_dy = dpdy(s.local.xy);
    if mode >= BARE_SCREEN { return preview_color(u32(mode - BARE_SCREEN), s.uv, uv_dx, uv_dy); }
    if mode >= 32.0 {
        if mode >= 96.0 && s.preview > 0.5 {
            let layer = u32(s.preview) - 1u;
            let rect = scene.screens[layer].rect;
            let xy = (s.local.xy - rect.xy) / rect.zw;
            return preview_color(layer, xy, local_dx/rect.zw, local_dy/rect.zw);
        }
        let tex = textureSampleGrad(products, linear_sampler, s.uv, i32(mode - 32.0) % 32, uv_dx, uv_dy).rgb;
        if (i32(mode - 32.0) / 32) % 2 == 1 { return vec4<f32>(s.material.rgb * tex * (0.65 + 0.35 * s.glow), 1.0); }
        return out(light(s, n, base * lin(tex)), 1.0);
    }
    if mode == 3.0 {
        let sun = 1.0 - smoothstep(0.105, 0.135, length((s.uv-vec2f(0.5,0.32))*vec2f(1.0,3.85)));
        let stripe = step(0.78, s.uv.y) * (1.0-step(0.83, s.uv.y));
        return vec4f(mix(mix(vec3f(0.98,0.88,0.70),vec3f(0.93,0.38,0.30),sun),vec3f(0.21,0.45,0.61),stripe),1.0);
    }
    if mode == 4.0 {
        let p = s.uv - vec2f(0.66,0.52);
        let isobar = length(p*vec2f(1.13,0.74)) + 0.035*sin(s.uv.x*11.0+s.uv.y*6.0);
        let spacing = abs(fract(isobar*10.0)-0.5);
        let line = (1.0-smoothstep(0.035,0.12,spacing))*(1.0-smoothstep(0.012,0.055,fwidth(isobar)));
        let spine = 1.0-smoothstep(0.008,0.044,abs(s.uv.x-0.5));
        let paper = mix(vec3f(0.95,0.91,0.76),vec3f(0.37,0.59,0.71),line);
        return out(light(s,n,lin(paper*(1.0-0.23*spine))),1.0);
    }
    if mode == 6.0 {
        let r = length(s.normal.xz);
        let color = mix(vec3<f32>(0.006, 0.012, 0.020), accent(), s.highlight);
        return vec4<f32>(color, 0.62 * (1.0 - smoothstep(0.26, 1.0, r)));
    }
    if mode == 15.0 {
        let view_dir = normalize(scene.camera.xyz - s.world);
        let r = reflect(-view_dir, n);
        let rim = pow(1.0 - abs(dot(view_dir, n)), 3.0);
        let reflection = river_sky(moving_clouds(panorama_uv(r)));
        let sheen = smoothstep(0.72, 0.97, dot(r, normalize(vec3<f32>(0.65, 0.33, -0.68))));
        let glass = mix(vec3<f32>(0.39, 0.55, 0.62), reflection, 0.55) + vec3<f32>(0.19)*sheen;
        return vec4<f32>(glass, 0.15 + rim*0.34 + sheen*0.10);
    }
    if mode == 29.0 {
        if s.rgb < 1.5 { discard; }
        return vec4<f32>(textureSampleLevel(kraken_lcd, linear_sampler, s.uv, 0.0).rgb, 1.0);
    }
    if mode >= 17.0 && mode < 29.0 {
        let fan = scene.fan_colors[u32(mode - 17.0)];
        let lamp = select(base, lin(fan.rgb), s.rgb > 0.5 && fan.a > 0.5);
        if s.uv.x > 0.5 {
            let diffuse = select(light(s,n,base),light(s,n,base)*0.30 + lamp*0.64,s.rgb>0.5 && fan.a>0.5);
            return out(diffuse,1.0);
        }
        return out(lamp * (0.84 + 0.40*s.glow), 1.0);
    }
    if mode == 16.0 {
        return out(base * (0.84 + 0.40*s.glow), 1.0);
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
    if mode == 13.0 {
        let grain = sin(s.world.z * 27.0 + 1.65 * sin(s.world.x * 0.48 + s.world.z * 1.8) + 0.9 * sin(s.world.x * 0.97));
        tone *= 0.92 + 0.12 * grain * (1.0 - smoothstep(0.04, 0.16, fwidth(s.world.z)));
    }
    if mode == 14.0 {
        let weave = cos(s.world.x * 97.0) * cos(s.world.z * 89.0);
        tone *= 1.0 + 0.075 * weave * (1.0 - smoothstep(0.015, 0.05, fwidth(s.world.x)));
    }
    if mode == 2.0 {
        let seam = abs(fract(s.world.xz * vec2<f32>(0.2, 0.34)) - 0.5);
        let joint = smoothstep(0.494, 0.499, max(seam.x, seam.y));
        tone *= 1.0 - 0.42 * joint;
    }
    return out(light(s, n, tone), 1.0);
}
