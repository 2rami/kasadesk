use std::collections::HashMap;
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use anyhow::{bail, ensure, Context, Result};
use glam::{vec3, Vec3};
use serde_json::Value;

use super::mesh::{Model, Vertex};
use crate::roster::{DeviceKind, Machine};

pub const ATLAS_EDGE: u32 = 1024;
pub const VENDOR_MATERIAL: f32 = 32.0;

pub struct Geometry {
    pub vertices: Vec<Vertex>,
    pub atlas: Vec<u8>,
    pub center: Vec3,
    pub half: Vec3,
    pub screen: Option<[f32; 4]>,
}

pub struct Asset {
    pub name: String,
    pub model: Model,
    pub geometry: Option<Geometry>,
}

#[derive(Default)]
pub struct Catalog {
    pub assets: Vec<Asset>,
    indices: HashMap<String, usize>,
    manifest: Value,
}

pub fn library() -> &'static Catalog {
    static CATALOG: OnceLock<Catalog> = OnceLock::new();
    CATALOG.get_or_init(|| {
        let Some(dir) = directory() else { return Catalog::default() };
        if !dir.join("catalog.json").is_file() { return Catalog::default() }
        Catalog::open(&dir).unwrap_or_else(|e| {
            eprintln!("[models] {}: {e:#}", dir.display());
            Catalog::default()
        })
    })
}

fn directory() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("KASADESK_MODEL_DIR").filter(|s| !s.is_empty()) {
        return Some(dir.into());
    }
    #[cfg(target_os = "macos")]
    let base = PathBuf::from(std::env::var_os("HOME")?).join("Library/Application Support");
    #[cfg(windows)]
    let base = PathBuf::from(std::env::var_os("LOCALAPPDATA")?);
    #[cfg(not(any(target_os = "macos", windows)))]
    let base = std::env::var_os("XDG_DATA_HOME").map(PathBuf::from)
        .or_else(|| Some(PathBuf::from(std::env::var_os("HOME")?).join(".local/share")))?;
    Some(base.join("KasaDesk").join("Models"))
}

impl Catalog {
    fn open(dir: &Path) -> Result<Self> {
        let mut manifest: Value = serde_json::from_reader(std::fs::File::open(dir.join("catalog.json"))?)?;
        let bindings = dir.join("bindings.json");
        if bindings.is_file() {
            let overrides: HashMap<String, Value> = serde_json::from_reader(std::fs::File::open(bindings)?)?;
            for (key, binding) in overrides { manifest["devices"][key] = binding; }
        }
        let models = manifest["models"].as_object().context("models must be an object")?;
        ensure!(models.len() <= 32, "too many model entries");
        let mut assets = Vec::new();
        let mut indices = HashMap::new();
        let mut remaining_vertices = 4_000_000;
        for (key, value) in models {
            ensure!(!key.is_empty() && key.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-'), "invalid model key");
            let model = Model::parse(value["kind"].as_str().context("missing model kind")?).context("unknown model kind")?;
            let name = value["name"].as_str().unwrap_or(key).to_owned();
            let file = value["cache"].as_str().filter(|f| {
                f.ends_with(".kdm") && f.bytes().all(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'.'))
            });
            let geometry = if let Some(path) = file.map(|f| dir.join(f)).filter(|p| p.is_file()) {
                match Geometry::load(&path, model, assets.len(), remaining_vertices, value["width_m"].as_f64(), value["screen_offset_m"].as_f64().unwrap_or(0.0)) {
                    Ok(g) => { remaining_vertices -= g.vertices.len(); Some(g) }
                    Err(e) => { eprintln!("[models] {key}: {e:#}"); None }
                }
            } else { None };
            indices.insert(key.clone(), assets.len());
            assets.push(Asset { name, model, geometry });
        }
        Ok(Self { assets, indices, manifest })
    }

    fn binding<'a>(&'a self, machine: &Machine, slot: &str) -> Option<&'a str> {
        let bindings = &self.manifest["devices"];
        [machine.kasanet.get("id").and_then(Value::as_str), Some(machine.label.as_str()), machine.model_id.as_deref()]
            .into_iter().flatten().find_map(|id| bindings[id][slot].as_str())
    }

    pub fn description(&self, machine: &Machine) -> Option<&str> {
        self.binding(machine, "description")
    }

    pub fn kind(&self, machine: &Machine) -> DeviceKind {
        if self.binding(machine, "case").is_some() {
            return DeviceKind::Desktop;
        }
        let fallback = if !machine.kind.on_rack() && self.binding(machine, "display").is_some() { DeviceKind::Desktop } else { machine.kind };
        self.binding(machine, "computer").and_then(|key| self.indices.get(key)).map_or(fallback, |i| {
            match self.assets[*i].model {
                Model::Pro => DeviceKind::Pro, Model::Air => DeviceKind::Air, Model::Laptop => DeviceKind::Laptop,
                Model::Mini => DeviceKind::Mini, Model::MiniM4 => DeviceKind::MiniM4, _ => machine.kind,
            }
        })
    }

    pub fn resolve(&self, machine: &Machine, model: Model) -> Option<usize> {
        let slot = match model { Model::Monitor => "display", Model::Tower => "case", _ => "computer" };
        let default_key = if model == Model::Monitor && self.kind(machine).on_rack() { "mini-display" } else { model.key() };
        let key = self.binding(machine, slot).or_else(|| self.manifest["defaults"][default_key].as_str())?;
        self.indices.get(key).copied().filter(|&i| self.assets[i].model == model)
    }
}

impl Geometry {
    fn load(path: &Path, model: Model, layer: usize, remaining: usize, width_m: Option<f64>, screen_offset_m: f64) -> Result<Self> {
        let f = std::fs::File::open(path)?;
        let length = f.metadata()?.len();
        let mut f = BufReader::new(f);
        let mut header = [0; 12];
        f.read_exact(&mut header)?;
        ensure!(&header[..4] == b"KDM1", "unsupported model format");
        let count = u32::from_le_bytes(header[4..8].try_into()?) as usize;
        let tex_bytes = u32::from_le_bytes(header[8..].try_into()?) as usize;
        ensure!(count > 0 && count <= remaining.min(3_000_000) && count % 3 == 0, "invalid vertex count");
        ensure!(tex_bytes <= 12 << 20 && length == (12 + count * 48 + tex_bytes) as u64, "invalid model length");
        let mut vertices = vec![bytemuck::Zeroable::zeroed(); count];
        f.read_exact(bytemuck::cast_slice_mut::<Vertex, u8>(&mut vertices))?;
        let (anchor, half) = model.hull();
        let scale = width_m.map_or(half.x * 2.0, |w| (w as f32 / 0.127).clamp(0.1, half.x * 2.0));
        let (mut screen_lo, mut screen_hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
        let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
        for v in &mut vertices {
            let p = Vec3::from_array(v.point);
            ensure!(p.is_finite() && p.abs().max_element() < 8.0 && Vec3::from_array(v.normal).is_finite()
                && v.material.iter().chain(&v.uv).all(|n| n.is_finite()), "invalid vertex");
            let mut world = p * scale + vec3(0.0, 0.0, anchor.z);
            let flags = v.material[3] as u8;
            if flags & 2 != 0 {
                // Some AR files put opaque cover glass above the original luminous panel in this renderer.
                world.z += (screen_offset_m as f32).clamp(0.0, 0.005) / 0.127;
                screen_lo = screen_lo.min(world);
                screen_hi = screen_hi.max(world);
            }
            v.material[3] = VENDOR_MATERIAL + layer as f32 + 32.0 * f32::from(flags & 3);
            v.point = world.to_array();
            lo = lo.min(world);
            hi = hi.max(world);
        }
        let mut png = vec![0; tex_bytes];
        f.read_exact(&mut png)?;
        let reader = image::ImageReader::with_format(std::io::Cursor::new(&png), image::ImageFormat::Png);
        if reader.into_dimensions()? != (ATLAS_EDGE, ATLAS_EDGE) { bail!("unexpected atlas dimensions") }
        let atlas = image::load_from_memory_with_format(&png, image::ImageFormat::Png)?.into_rgba8().into_raw();
        let screen = (screen_hi.x > screen_lo.x && screen_hi.y > screen_lo.y).then(||
            [screen_lo.x, screen_lo.y, screen_hi.x - screen_lo.x, screen_hi.y - screen_lo.y]);
        Ok(Self { vertices, atlas, screen, center: (lo + hi) / 2.0, half: (hi - lo) / 2.0 })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_bound_desktop_resolves_its_display_and_case_even_with_an_old_roster() {
        let catalog = Catalog {
            assets: vec![
                Asset { name: "monitor".into(), model: Model::Monitor, geometry: None },
                Asset { name: "case".into(), model: Model::Tower, geometry: None },
            ],
            indices: [("monitor".into(), 0), ("case".into(), 1)].into(),
            manifest: json!({"devices":{"pc-id":{"display":"monitor","case":"case"}}}),
        };
        let machine = Machine {
            label: "renamed machine".into(), kind: DeviceKind::Mini, model_id: None,
            kasanet: json!({"id":"pc-id"}), fresh: false, account: None,
        };
        assert_eq!(catalog.kind(&machine), DeviceKind::Desktop);
        assert_eq!(catalog.resolve(&machine, Model::Monitor), Some(0));
        assert_eq!(catalog.resolve(&machine, Model::Tower), Some(1));
        assert_eq!(catalog.resolve(&machine, Model::Mini), None);
    }
}
