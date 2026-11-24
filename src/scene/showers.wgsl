// KasaTerm showers.wgsl, MIT (c) 2026 2rami. Source revision in assets/river-sunshower.md.
fn grain(p: vec2f) -> f32 {
  var q = fract(p.xyx * vec3f(0.1031, 0.1030, 0.0973));
  q += vec3f(dot(q, q.yzx + 33.33));
  return fract((q.x + q.y) * q.z);
}

fn falling(p: vec2f, time: f32, wind: f32, amount: f32, depth: f32) -> f32 {
  let pitch = mix(11.0, 41.0, depth);
  let speed = mix(180.0, 560.0, depth) * (0.75 + amount * 0.4);
  let spacing = mix(72.0, 181.0, depth);
  let laneX = (p.x - wind * p.y) / pitch;
  let grid = vec2f(laneX, (p.y - time * speed) / spacing + grain(vec2f(floor(laneX), depth * 191.0)));
  let id = floor(grid) + depth * 231.0;
  let r = grain(id);
  if (r > 0.16 + 0.67 * amount) { return 0.0; }
  let delta = (fract(grid) - vec2f(0.12 + 0.76 * grain(id + 12.0), 0.33 + 0.58 * grain(id + 17.0))) * vec2f(pitch, spacing);
  let len = mix(5.0, 33.0, depth) * (0.6 + grain(id + 6.0) * 0.8);
  let body = smoothstep(-len, 0.0, delta.y) * (1.0 - smoothstep(0.0, 1.2, delta.y));
  return body * (1.0 - smoothstep(0.0, 0.65 + depth * 0.8, abs(delta.x))) * (0.17 + depth * 0.32);
}

// The approaching drop and its spray share a landing position and a clock.
fn impact(p: vec2f, t: f32, amount: f32, wind: f32, seed: f32) -> f32 {
  if (p.y < -32.0 || p.y > 2.0) { return 0.0; }
  let pitch = 24.0;
  let lane = floor(p.x / pitch);
  var light = 0.0;
  for (var i = -1; i <= 1; i++) {
    let id = lane + f32(i);
    let phase = grain(vec2f(id, seed));
    let clock = t * (0.45 + 1.8 * amount) * (0.8 + 0.5 * phase) + phase;
    let key = vec2f(id + seed, floor(clock));
    let land = (id + 0.12 + 0.76 * grain(key)) * pitch;
    let age = (fract(clock) - 0.48) / ((0.45 + 1.8 * amount) * (0.8 + 0.5 * phase));
    let dx = p.x - land;
    if (age < 0.0) {
      let head = age * 455.0;
      let tail = p.y - head;
      light += (1.0 - smoothstep(0.05, 1.15, abs(dx - wind * p.y))) *
          smoothstep(-16.0, 0.0, tail) * (1.0 - smoothstep(0.0, 1.0, tail));
    } else if (age < 0.26) {
      light += (1.0 - smoothstep(0.0, 1.0, length(vec2f(dx * 0.28, p.y)))) *
          (1.0 - smoothstep(0.0, 0.09, age));
      for (var j = 0; j < 3; j++) {
        let r = grain(key + f32(j) * 9.7 + 3.3);
        let u = age / (0.13 + r * 0.13);
        if (u >= 1.0) { continue; }
        let vx = (f32(j) - 1.0) * (4.0 + 7.0 * r) + wind * 7.0;
        let center = vec2f(vx * u, -4.0 * u * (1.0 - u) * (2.0 + r * 8.0));
        light += (1.0 - smoothstep(0.15, 0.85 + r * 0.7, length(p - vec2f(land, 0.0) - center))) *
            (1.0 - u * u);
      }
    }
  }
  return min(light, 1.0);
}

// A bright lower rim and a dark interior keep the beads legible over both themes.
fn bead(p: vec2f, amount: f32, seed: f32) -> vec2f {
  let grid = p / vec2f(17.0, 21.0);
  let cell = floor(grid) + seed;
  let r = grain(cell);
  if (r > 0.10 + 0.38 * amount) { return vec2f(0.0); }
  var q = (fract(grid) - vec2f(0.2 + 0.6 * grain(cell + 2.0), 0.2 + 0.6 * grain(cell + 4.0)))
      * vec2f(17.0, 21.0) / (0.9 + 2.0 * grain(cell + 8.0));
  q.y = q.y * 0.76;
  let mask = 1.0 - smoothstep(0.58, 1.10, length(q));
  let rim = smoothstep(0.2, 0.86, q.y - 0.28 * q.x);
  let glint = 1.0 - smoothstep(0.0, 0.40, length(q + vec2f(0.23, 0.36)));
  return mask * vec2f(0.025 + 0.19 * smoothstep(0.0, 0.83, -q.y + 0.22 * q.x), rim * 0.52 + glint * 0.32);
}

fn haze(p: vec2f) -> f32 {
  let cell = floor(p);
  var u = fract(p);
  u = u * u * (3.0 - 2.0 * u);
  return mix(mix(grain(cell), grain(cell + vec2f(1.0, 0.0)), u.x),
             mix(grain(cell + vec2f(0.0, 1.0)), grain(cell + 1.0), u.x), u.y);
}

fn wash(color: vec3f, alpha: f32) -> vec4f {
    return vec4f(color * alpha, alpha);
}

fn cloudField(p: vec2f) -> f32 {
    let warp = haze(p * 0.63);
    let q = p + vec2f(0.74 * warp, 0.32 * warp);
    return 0.56 * haze(q) + 0.28 * haze(q * 2.03 + 9.1) + 0.16 * haze(q * 4.19 + 3.7);
}

fn cloudBank(uv: vec2f, clock: f32, cover: f32, wind: f32) -> vec4f {
    let q = uv * vec2f(3.0, 3.8) - vec2f(clock * (0.047 + 0.09 * wind), clock * 0.009);
    let field = cloudField(q) - uv.y * 0.16;
    let threshold = mix(0.67, 0.23, cover);
    let mass = smoothstep(threshold - 0.085, threshold + 0.13, field);
    let relief = cloudField(q + vec2f(0.11, 0.18)) - field - uv.y * 0.16;
    let light = clamp(0.58 + 1.7 * relief + 0.28 * (1.0 - uv.y), 0.0, 1.0);
    let color = mix(vec3f(0.24, 0.31, 0.41), vec3f(0.83, 0.87, 0.91), light);
    return wash(color, mass * (0.18 + 0.21 * cover));
}

fn flakes(p: vec2f, clock: f32, wind: f32, pitch: f32, speed: f32, density: f32, sizePx: f32, ice: bool) -> f32 {
    var q = p - vec2f(wind * speed * clock, speed * clock);
    if (!ice) { q.x += sin(p.y * 0.017 + clock * 1.3 + pitch) * pitch * 0.20; }
    let grid = q / pitch;
    let cell = floor(grid);
    let noise = grain(cell + pitch);
    let d = (fract(grid) - vec2f(0.22 + 0.56 * grain(cell + 3.0), 0.22 + 0.56 * grain(cell + 8.0))) * pitch;
    let radius = sizePx * (0.48 + noise * 0.80);
    var dist = length(d);
    if (ice) { dist = abs(d.x) + abs(d.y) * 0.76; }
    return (1.0 - smoothstep(radius * 0.3, radius + 0.55, dist)) * step(noise, density);
}

fn windScene(p: vec2f, clock: f32, variant: f32, wind: f32, ink: vec3f) -> vec4f {
    var direction = 1.0;
    if (wind < 0.0) { direction = -1.0; }
    let travel = direction * p.x - clock * (73.0 + variant * 105.0 + abs(wind) * 60.0);
    let lift = 6.0 * sin(p.x * 0.029 + clock * 0.54) + 3.4 * sin(p.x * 0.043 - clock * 0.22);
    let grid = vec2f(travel / 116.0, (p.y + lift) / 28.0);
    let cell = floor(grid);
    let y = 0.20 + 0.60 * grain(cell);
    let x = fract(grid.x);
    let arc = (1.0 - smoothstep(0.005, 0.047, abs(fract(grid.y) - y))) * sin(x * 3.14159);
    let fade = smoothstep(0.16, 0.59, grain(cell + 4.0) + variant * 0.25);
    return wash(ink, arc * fade * (0.39 + 0.21 * variant));
}
