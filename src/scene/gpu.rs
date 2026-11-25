//! 방 그리기 — 창 렌더러의 덧그리기 패스(`render_with_underlay`)에서 깊이 버퍼를 따로 두고 그린다. 방 모형은 배치가 바뀔 때만
//! 다시 올리고, 기기는 모형 한 벌에 자리(인스턴스)만 바꾼다.

use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;
use glam::{Mat4, Vec3, vec3};

use super::{catalog, mesh, PAGE};
use std::ops::Range;
use super::{Frame, Layout};

const VERTEX_ATTRS: [wgpu::VertexAttribute; 4] = wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x4, 6 => Float32x2];
const INSTANCE_ATTRS: [wgpu::VertexAttribute; 6] = wgpu::vertex_attr_array![3 => Float32x3, 4 => Float32, 5 => Float32, 7 => Float32, 8 => Float32, 9 => Float32x4];
const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
// Four vendor flag combinations occupy material IDs 32 through 159.
const BARE_SCREEN: f32 = 160.0;
const SHADOW_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth24Plus;
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Instance {
    position: [f32; 3],
    highlight: f32,
    glow: f32,
    preview: f32,
    rgb: f32,
    gpu_color: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Scene {
    clip_from_world: [[f32; 4]; 4],
    sun_clip: [[f32; 4]; 4],
    world_from_clip: [[f32; 4]; 4],
    camera: [f32; 4],
    lighting: [f32; 4],
    weather: [f32; 4],
    viewport: [f32; 4],
    bench: [f32; 4],
    landscape: [f32; 4],
    windows: [f32; 4],
    screens: [PreviewScreen; PAGE],
    fan_colors: [[f32; 4]; 12],
}

#[repr(C)]
#[derive(Clone, Copy, Default, Pod, Zeroable)]
struct PreviewScreen {
    horizontal: [f32; 4],
    vertical: [f32; 4],
    image: [f32; 4],
    arc: [f32; 4],
}

struct Product {
    vertices: wgpu::Buffer,
    indices: wgpu::Buffer,
    body: Range<u32>,
    far: Range<u32>,
    glass: Range<u32>,
    contact: Range<u32>,
}

impl Product {
    fn bind(&self, pass: &mut wgpu::RenderPass<'_>) {
        pass.set_vertex_buffer(0, self.vertices.slice(..));
        pass.set_index_buffer(self.indices.slice(..), wgpu::IndexFormat::Uint32);
    }
}

struct Batch {
    near: Range<u32>,
    far: Range<u32>,
    all: Range<u32>,
}

struct Targets {
    size: wgpu::Extent3d,
    depth: wgpu::TextureView,
    color: Option<wgpu::TextureView>,
}

pub struct RoomGpu {
    backdrop: wgpu::RenderPipeline,
    solid: wgpu::RenderPipeline,
    glaze: wgpu::RenderPipeline,
    shadows: wgpu::RenderPipeline,
    shadow_parts: Vec<(usize, [f32; 3])>,
    shadow_depth: wgpu::TextureView,
    shadow_binding: wgpu::BindGroup,
    scene: wgpu::Buffer,
    binding: wgpu::BindGroup,
    room: Option<(mesh::Plan, wgpu::Buffer, u32)>,
    lcd_texture: wgpu::Texture,
    lcd_serial: u64,
    preview_texture: wgpu::Texture,
    bare_screens: wgpu::Buffer,
    preview_serial: [u64; PAGE],
    products: Vec<Option<Product>>,
    samples: u32,
    far_detail: Vec<bool>,
    instances: Option<(usize, wgpu::Buffer)>,
    targets: Option<Targets>,
}

impl RoomGpu {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, format: wgpu::TextureFormat) -> Self {
        let scene = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("KasaDesk room scene"),
            size: std::mem::size_of::<Scene>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("KasaDesk room shader"),
            source: wgpu::ShaderSource::Wgsl(concat!(include_str!("showers.wgsl"), "\n", include_str!("room.wgsl")).into()),
        });
        let mut plate = image::load_from_memory_with_format(include_bytes!("../../assets/river-panorama.jpg"), image::ImageFormat::Jpeg)
            .expect("embedded riverside panorama").into_rgba8();
        let sky = image::load_from_memory_with_format(include_bytes!("../../assets/river-sky-mask.png"), image::ImageFormat::Png)
            .expect("embedded skyline mask").into_luma8();
        assert_eq!(plate.dimensions(), sky.dimensions());
        for (pixel, &mask) in plate.pixels_mut().zip(sky.as_raw()) {
            pixel[3] = mask;
        }
        let texture = device.create_texture_with_data(queue, &wgpu::TextureDescriptor {
            label: Some("KasaDesk riverside horizon"),
            size: wgpu::Extent3d { width: plate.width(), height: plate.height(), depth_or_array_layers: 1 },
            mip_level_count: 1, sample_count: 1, dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm, usage: wgpu::TextureUsages::TEXTURE_BINDING, view_formats: &[],
        }, wgpu::util::TextureDataOrder::LayerMajor, plate.as_raw());
        let texture_view = texture.create_view(&Default::default());
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            mag_filter: wgpu::FilterMode::Linear, min_filter: wgpu::FilterMode::Linear, mipmap_filter: wgpu::MipmapFilterMode::Linear, anisotropy_clamp: 4, ..Default::default()
        });
        let library = catalog::library();
        let model_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("KasaDesk product atlases"),
            size: wgpu::Extent3d { width: catalog::ATLAS_EDGE, height: catalog::ATLAS_EDGE,
                depth_or_array_layers: library.assets.len().max(1) as u32 },
            mip_level_count: catalog::ATLAS_MIPS, sample_count: 1, dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm, usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let atlas_view = model_texture.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::D2Array), ..Default::default()
        });
        let products = library.assets.iter().enumerate().map(|(layer, asset)| {
            let g = asset.geometry.as_ref()?;
            for (level, image) in g.atlas.iter().enumerate() {
                queue.write_texture(wgpu::TexelCopyTextureInfo {
                    texture: &model_texture, mip_level: level as u32, origin: wgpu::Origin3d { x: 0, y: 0, z: layer as u32 },
                    aspect: wgpu::TextureAspect::All,
                }, image.as_raw(), wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(4*image.width()), rows_per_image: None },
                    wgpu::Extent3d { width: image.width(), height: image.height(), depth_or_array_layers: 1 });
            }
            let m = &g.mesh;
            let vertices = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("KasaDesk product vertices"), usage: wgpu::BufferUsages::VERTEX, contents: bytemuck::cast_slice(&m.vertices),
            });
            let indices = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("KasaDesk product detail levels"), usage: wgpu::BufferUsages::INDEX, contents: bytemuck::cast_slice(&m.indices),
            });
            Some(Product { vertices, indices, body: m.body.clone(), far: m.far.clone(), glass: m.glass.clone(), contact: m.contact.clone() })
        }).collect();
        let preview_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("KasaDesk preview"), size: wgpu::Extent3d { width: crate::preview::WIDTH, height: crate::preview::HEIGHT, depth_or_array_layers: PAGE as u32 },
            mip_level_count: 1, sample_count: 1, dimension: wgpu::TextureDimension::D2, format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST, view_formats: &[],
        });
        let bare_screens = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("KasaDesk screens without a model"), size: (PAGE*6*std::mem::size_of::<mesh::Vertex>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false,
        });
        let preview_view = preview_texture.create_view(&wgpu::TextureViewDescriptor { dimension: Some(wgpu::TextureViewDimension::D2Array), ..Default::default() });
        let lcd_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Kraken LCD"),
            size: wgpu::Extent3d { width: 192, height: 192, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let lcd_view = lcd_texture.create_view(&Default::default());
        let scene_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("KasaDesk room scene layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None },
                count: None,
            }, wgpu::BindGroupLayoutEntry {
                binding: 1, visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2, multisampled: false }, count: None,
            }, wgpu::BindGroupLayoutEntry {
                binding: 2, visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering), count: None,
            }, wgpu::BindGroupLayoutEntry {
                binding: 3, visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2Array, multisampled: false }, count: None,
            }, wgpu::BindGroupLayoutEntry {
                binding: 4, visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2Array, multisampled: false }, count: None,
            }, wgpu::BindGroupLayoutEntry {
                binding: 5, visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2, multisampled: false }, count: None,
            }],
        });
        let shadow_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("KasaDesk sunlight layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0, visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Depth,
                    view_dimension: wgpu::TextureViewDimension::D2, multisampled: false }, count: None,
            }, wgpu::BindGroupLayoutEntry {
                binding: 1, visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Comparison), count: None,
            }],
        });
        let shadow_depth = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("KasaDesk sunlight depth"),
            size: wgpu::Extent3d { width: 1536, height: 1536, depth_or_array_layers: 1 },
            mip_level_count: 1, sample_count: 1, dimension: wgpu::TextureDimension::D2, format: SHADOW_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING, view_formats: &[],
        }).create_view(&Default::default());
        let comparison = device.create_sampler(&wgpu::SamplerDescriptor {
            compare: Some(wgpu::CompareFunction::LessEqual),
            mag_filter: wgpu::FilterMode::Linear, min_filter: wgpu::FilterMode::Linear, ..Default::default()
        });
        let shadow_binding = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("KasaDesk sunlight"), layout: &shadow_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&shadow_depth) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&comparison) },
            ],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("KasaDesk room pipeline layout"), bind_group_layouts: &[&scene_layout, &shadow_layout], immediate_size: 0,
        });
        let buffers = [
            wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<mesh::Vertex>() as u64,
                step_mode: wgpu::VertexStepMode::Vertex, attributes: &VERTEX_ATTRS,
            },
            wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<Instance>() as u64,
                step_mode: wgpu::VertexStepMode::Instance, attributes: &INSTANCE_ATTRS,
            },
        ];
        let depth_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("KasaDesk shadow pass layout"), bind_group_layouts: &[&scene_layout], immediate_size: 0,
        });
        let shadows = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("KasaDesk shadow pass"), layout: Some(&depth_layout),
            vertex: wgpu::VertexState { module: &shader, entry_point: Some("project_shadow"), buffers: &buffers, compilation_options: Default::default() },
            primitive: Default::default(),
            depth_stencil: Some(wgpu::DepthStencilState {
                format: SHADOW_FORMAT, depth_write_enabled: true, depth_compare: wgpu::CompareFunction::LessEqual,
                bias: wgpu::DepthBiasState { constant: 2, slope_scale: 2.0, clamp: 0.0 }, stencil: Default::default(),
            }),
            multisample: Default::default(), fragment: None, multiview_mask: None, cache: None,
        });
        let flags = |f: wgpu::TextureFormat| f.guaranteed_format_features(device.features()).flags;
        let msaa = flags(format).contains(wgpu::TextureFormatFeatureFlags::MULTISAMPLE_RESOLVE)
            && flags(format).sample_count_supported(4) && flags(DEPTH_FORMAT).sample_count_supported(4);
        let samples = if msaa { 4 } else { 1 };
        let pipeline = |depth_write, backdrop| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("KasaDesk room pipeline"),
                layout: Some(&layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some(if backdrop { "project_sky" } else { "project" }),
                    buffers: if backdrop { &[] } else { &buffers },
                    compilation_options: Default::default(),
                },
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: DEPTH_FORMAT,
                    depth_write_enabled: depth_write,
                    depth_compare: wgpu::CompareFunction::GreaterEqual,
                    stencil: Default::default(),
                    bias: Default::default(),
                }),
                multisample: wgpu::MultisampleState { count: samples, ..Default::default() },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some(if backdrop { "horizon" } else { "shade" }),
                    targets: &[Some(wgpu::ColorTargetState { format, blend: Some(wgpu::BlendState::ALPHA_BLENDING), write_mask: wgpu::ColorWrites::ALL })],
                    compilation_options: Default::default(),
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        let backdrop = pipeline(false, true);
        let solid = pipeline(true, false);
        // 반투명 바닥 그림자·빛 웅덩이 — 깊이를 쓰지 않고 덧그린다.
        let glaze = pipeline(false, false);
        let binding = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("KasaDesk room scene binding"),
            layout: &scene_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: scene.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&texture_view) },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::Sampler(&sampler) },
                wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::TextureView(&atlas_view) },
                wgpu::BindGroupEntry { binding: 4, resource: wgpu::BindingResource::TextureView(&preview_view) },
                wgpu::BindGroupEntry { binding: 5, resource: wgpu::BindingResource::TextureView(&lcd_view) },
            ],
        });
        Self {
            backdrop,
            solid,
            glaze,
            shadows,
            shadow_parts: Vec::new(),
            shadow_depth,
            shadow_binding,
            scene,
            binding,
            room: None,
            lcd_texture,
            lcd_serial: 0,
            preview_texture,
            bare_screens,
            preview_serial: [0; PAGE],
            products,
            samples,
            far_detail: Vec::new(),
            instances: None,
            targets: None,
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn draw(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        texture: &wgpu::Texture,
        view: &wgpu::TextureView,
        layout: &Layout,
        frame: &Frame,
        previews: &[(usize, std::sync::Arc<crate::preview::Picture>)],
        rgb: Option<(usize, &crate::rgb::Snapshot)>,
        gpu_color: Option<(usize, crate::rgb::Color)>,
    ) {
        let size = texture.size();
        if self.targets.as_ref().is_none_or(|t| t.size != size) {
            let attachment = |label, format| device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label), size, mip_level_count: 1, sample_count: self.samples, dimension: wgpu::TextureDimension::D2,
                format, usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TRANSIENT, view_formats: &[],
            }).create_view(&Default::default());
            self.targets = Some(Targets { size, depth: attachment("KasaDesk depth", DEPTH_FORMAT),
                color: (self.samples > 1).then(|| attachment("KasaDesk MSAA color", texture.format())) });
        }
        let new_room = self.room.as_ref().is_none_or(|(plan, _, _)| *plan != layout.plan);
        if new_room {
            let vertices = mesh::room_vertices(&layout.plan);
            let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("KasaDesk room meshes"),
                contents: bytemuck::cast_slice(&vertices),
                usage: wgpu::BufferUsages::VERTEX,
            });
            self.room = Some((layout.plan, buffer, vertices.len() as u32));
        }
        let (left, right) = layout.plan.desk;
        let left = left - layout.plan.rack_width();
        let half = (right - left) * 0.5 + 8.0;
        let target = vec3((left + right) * 0.5, mesh::DESK_Y, 0.0);
        let (room_lo, room_hi) = layout.plan.limits();
        let landscape_radius = (half * 3.0).max(144.0);
        let sun = vec3(0.76, 0.45, -0.42).normalize();
        let sun_view = Mat4::look_at_rh(target + sun * (half + 20.0), target, Vec3::Y);
        let sun_clip = Mat4::orthographic_rh(-half, half, -half, half, 0.1, 2.0 * half + 45.0) * sun_view;
        if let Some((_, snap)) = rgb {
            if let Some(img) = snap.lcd.as_ref().filter(|_| self.lcd_serial != snap.serial) {
                queue.write_texture(
                    self.lcd_texture.as_image_copy(),
                    img.as_raw(),
                    wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(img.width() * 4), rows_per_image: None },
                    wgpu::Extent3d { width: img.width(), height: img.height(), depth_or_array_layers: 1 },
                );
                self.lcd_serial = snap.serial;
            }
        }
        let mut screens = [PreviewScreen::default(); PAGE];
        let mut bare = Vec::new();
        for (index, pic) in previews {
            let Some(slot) = layout.device_of(*index) else { continue };
            let Some(part) = layout.main_part(slot) else { continue };
            let mut screen = PreviewScreen::default();
            let aspect = if let Some(panel) = part.screen() {
                screen.horizontal = panel.horizontal.to_array();
                screen.vertical = panel.vertical.to_array();
                screen.arc = [panel.arc[0], panel.arc[1], 0.0, 0.0];
                panel.size.x / panel.size.y
            } else {
                let quad = part.screen_quad();
                let uv = [[0.0,0.0],[1.0,0.0],[1.0,1.0],[0.0,1.0]];
                for corner in [0,1,2,0,2,3] {
                    bare.push(mesh::Vertex { point: (part.position + quad[corner]).to_array(), normal: [0.0,0.0,1.0],
                        material: [1.0,1.0,1.0,BARE_SCREEN + slot as f32], uv: uv[corner] });
                }
                let (_, size, _, _) = part.face();
                size.x / size.y
            };
            let (w,h) = pic.size;
            let ratio = aspect * h as f32 / w as f32;
            screen.image = [w as f32, h as f32, ratio.min(1.0), ratio.recip().min(1.0)];
            screens[slot] = screen;
            if self.preview_serial[slot] != pic.serial {
                queue.write_texture(wgpu::TexelCopyTextureInfo {
                    texture: &self.preview_texture, mip_level: 0, origin: wgpu::Origin3d { x: 0, y: 0, z: slot as u32 }, aspect: wgpu::TextureAspect::All,
                }, &pic.rgba, wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(4*w), rows_per_image: None },
                    wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 });
                self.preview_serial[slot] = pic.serial;
            }
        }
        if !bare.is_empty() { queue.write_buffer(&self.bare_screens, 0, bytemuck::cast_slice(&bare)); }
        queue.write_buffer(&self.scene, 0, bytemuck::bytes_of(&Scene {
            clip_from_world: frame.clip_from_world.to_cols_array_2d(),
            sun_clip: sun_clip.to_cols_array_2d(),
            world_from_clip: frame.world_from_clip.to_cols_array_2d(),
            camera: frame.camera.extend(1.0).to_array(),
            lighting: [frame.day, frame.dusk, frame.indoor, frame.rain_time],
            weather: frame.weather,
            viewport: [size.width as f32, size.height as f32, 0.0, 0.0],
            screens, fan_colors:rgb.map_or([[0.0;4];12],|(_,s)|s.lights()),
            bench: [left, right, mesh::DESK_Y, room_lo.z],
            landscape: [target.x, 12.0, room_lo.z, landscape_radius],
            windows: [room_lo.x, room_hi.x, room_hi.y, layout.plan.side_window_end()],
        }));
        // 방 한 칸 + 모형 종류마다 줄 세운 기기 자리.
        let mut data = vec![Instance { position: [0.0; 3], highlight: 0.0, glow: 0.0, preview: 0.0, rgb: 0.0, gpu_color: [0.0; 4] }];
        self.far_detail.resize(layout.parts.len(), false);
        let mut batches = Vec::with_capacity(self.products.len());
        for index in 0..self.products.len() {
            let start = data.len() as u32;
            let mut groups: [Vec<Instance>; 3] = Default::default();
            for (part_index,part) in layout.parts.iter().enumerate().filter(|(_,p)| p.asset == Some(index)) {
                let device = &layout.devices[part.device];
                let Some((center,half)) = part.bounds else { continue };
                let center = part.position + center;
                let group = match frame.footprint(center-half,center+half) {
                    Some(size) => {
                        // Hysteresis keeps a slow camera sway from alternating index buffers.
                        let threshold = if self.far_detail[part_index] { 240.0 } else { 210.0 };
                        self.far_detail[part_index] = size.full < threshold;
                        usize::from(self.far_detail[part_index])
                    }
                    None => 2,
                };
                let preview = if part.main && screens[part.device].image[0] > 0.0 { (part.device+1) as f32 } else { 0.0 };
                groups[group].push(Instance { position: part.position.to_array(), highlight: device.highlight, glow: device.glow,
                    gpu_color: gpu_color.filter(|(i, _)| *i == device.index && part.model == mesh::Model::Tower)
                        .map_or([0.0; 4], |(_, [r,g,b])| [r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, 1.0]),
                    rgb:rgb.filter(|(i,_)|*i==device.index && part.model==mesh::Model::Tower).map_or(0.0,|(_,s)|if s.lcd.is_some(){2.0}else{1.0}), preview });
            }
            let middle = start + groups[0].len() as u32;
            let visible = middle + groups[1].len() as u32;
            for group in groups { data.extend(group); }
            batches.push(Batch { near: start..middle, far: middle..visible, all: start..data.len() as u32 });
        }
        if self.instances.as_ref().is_none_or(|(cap, _)| *cap < data.len()) {
            let cap = data.len().next_power_of_two().max(16);
            let buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("KasaDesk room instances"),
                size: (std::mem::size_of::<Instance>() * cap) as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            self.instances = Some((cap, buffer));
        }
        let (Some((_, instances)), Some((_, vertices, room_count)), Some(targets)) = (&self.instances, &self.room, &self.targets) else { return };
        queue.write_buffer(instances, 0, bytemuck::cast_slice(&data));
        // Only placement changes the shadow silhouette. Rain and selection animate independently.
        let placements = || layout.parts.iter().filter_map(|p| Some((p.asset?, p.position.to_array())));
        if new_room || !placements().eq(self.shadow_parts.iter().copied()) {
            self.shadow_parts.clear();
            self.shadow_parts.extend(placements());
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("KasaDesk sunlight"), color_attachments: &[],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.shadow_depth,
                    depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Store }),
                    stencil_ops: None,
                }),
                timestamp_writes: None, occlusion_query_set: None, multiview_mask: None,
            });
            pass.set_pipeline(&self.shadows);
            pass.set_vertex_buffer(0, vertices.slice(..));
            pass.set_vertex_buffer(1, instances.slice(..));
            pass.set_bind_group(0, &self.binding, &[]);
            pass.draw(0..*room_count, 0..1);
            for (model, batch) in self.products.iter().zip(&batches) {
                if let Some(m) = model.as_ref().filter(|_| !batch.all.is_empty()) {
                    m.bind(&mut pass);
                    pass.draw_indexed(m.body.clone(), 0, batch.all.clone());
                }
            }
        }
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("KasaDesk room"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: targets.color.as_ref().unwrap_or(view),
                depth_slice: None,
                resolve_target: targets.color.as_ref().map(|_| view),
                ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: if targets.color.is_some() { wgpu::StoreOp::Discard } else { wgpu::StoreOp::Store } },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &targets.depth,
                depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(0.0), store: wgpu::StoreOp::Discard }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_vertex_buffer(0, vertices.slice(..));
        pass.set_vertex_buffer(1, instances.slice(..));
        pass.set_bind_group(0, &self.binding, &[]);
        pass.set_bind_group(1, &self.shadow_binding, &[]);
        pass.set_pipeline(&self.solid);
        pass.draw(0..*room_count, 0..1);
        if !bare.is_empty() {
            pass.set_vertex_buffer(0, self.bare_screens.slice(..));
            pass.draw(0..bare.len() as u32, 0..1);
        }
        for (model, batch) in self.products.iter().zip(&batches) {
            if let Some(m) = model.as_ref().filter(|_| !batch.near.is_empty() || !batch.far.is_empty()) {
                m.bind(&mut pass);
                if !batch.near.is_empty() { pass.draw_indexed(m.body.clone(), 0, batch.near.clone()); }
                if !batch.far.is_empty() { pass.draw_indexed(m.far.clone(), 0, batch.far.clone()); }
            }
        }
        // Let opaque geometry reject covered pixels before evaluating the outdoor weather.
        pass.set_pipeline(&self.backdrop);
        pass.draw(0..3, 0..1);
        pass.set_pipeline(&self.glaze);
        for glass in [false,true] {
            for (model, batch) in self.products.iter().zip(&batches) {
                let visible = batch.near.start..batch.far.end;
                if let Some(m) = model.as_ref().filter(|_| !visible.is_empty()) {
                    let indices = if glass { &m.glass } else { &m.contact };
                    if !indices.is_empty() {
                        m.bind(&mut pass);
                        pass.draw_indexed(indices.clone(), 0, visible);
                    }
                }
            }
        }
    }
}

/// 이 컴퓨터 시각으로 (낮의 밝기, 해 질 녘, 실내등) — 6시~18시가 낮, 그 앞뒤 한 시간씩 노을.
pub fn daylight() -> (f32, f32, f32) {
    let hour = local_hour();
    let day = smooth(5.5, 7.0, hour) * (1.0 - smooth(17.5, 19.0, hour));
    let dusk = (1.0 - ((hour - 18.2).abs() / 1.0).min(1.0)).max(1.0 - ((hour - 6.2).abs() / 0.8).min(1.0));
    let indoor = 1.0 - day * 0.75;
    (day, dusk, indoor)
}

fn smooth(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn local_hour() -> f32 {
    if let Some(h) = std::env::var("KASATERM_VIEW_TEST_HOUR").ok().and_then(|v| v.trim().parse::<f32>().ok()) {
        return h;
    }
    #[cfg(unix)]
    {
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs()) as libc::time_t;
        let mut tm: libc::tm = unsafe { std::mem::zeroed() };
        // SAFETY: localtime_r 는 넘긴 tm 에만 쓴다.
        if unsafe { libc::localtime_r(&now, &mut tm) }.is_null() {
            return 12.0;
        }
        tm.tm_hour as f32 + tm.tm_min as f32 / 60.0
    }
    #[cfg(windows)]
    {
        // SAFETY: GetLocalTime 은 넘긴 구조체에만 쓴다.
        let st = unsafe {
            let mut st = std::mem::zeroed::<windows_sys::Win32::Foundation::SYSTEMTIME>();
            windows_sys::Win32::System::SystemInformation::GetLocalTime(&mut st);
            st
        };
        f32::from(st.wHour) + f32::from(st.wMinute) / 60.0
    }
}
