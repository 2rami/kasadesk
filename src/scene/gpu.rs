//! 방 그리기 — 창 렌더러의 덧그리기 패스(`render_with_underlay`)에서 깊이 버퍼를 따로 두고 그린다. 방 모형은 배치가 바뀔 때만
//! 다시 올리고, 기기는 모형 한 벌에 자리(인스턴스)만 바꾼다.

use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;
use glam::{Mat4, Vec3, vec3};

use super::{catalog, mesh};
use std::ops::Range;
use super::{Frame, Layout};

const VERTEX_ATTRS: [wgpu::VertexAttribute; 4] = wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x4, 6 => Float32x2];
const INSTANCE_ATTRS: [wgpu::VertexAttribute; 3] = wgpu::vertex_attr_array![3 => Float32x3, 4 => Float32, 5 => Float32];
const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth24Plus;
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Instance {
    position: [f32; 3],
    lift: f32,
    glow: f32,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Scene {
    clip_from_world: [[f32; 4]; 4],
    sun_clip: [[f32; 4]; 4],
    eye: [f32; 4],
    camera: [f32; 4],
    lighting: [f32; 4],
    viewport: [f32; 4],
    bench: [f32; 4],
}

pub struct RoomGpu {
    backdrop: wgpu::RenderPipeline,
    solid: wgpu::RenderPipeline,
    glaze: wgpu::RenderPipeline,
    shadows: wgpu::RenderPipeline,
    shadow_depth: wgpu::TextureView,
    shadow_binding: wgpu::BindGroup,
    scene: wgpu::Buffer,
    binding: wgpu::BindGroup,
    room: Option<(mesh::Plan, wgpu::Buffer, u32)>,
    products: wgpu::Buffer,
    ranges: Vec<(Range<u32>, Range<u32>)>,
    instances: Option<(usize, wgpu::Buffer)>,
    depth: Option<(wgpu::Extent3d, wgpu::TextureView)>,
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
            source: wgpu::ShaderSource::Wgsl(include_str!("room.wgsl").into()),
        });
        let plate = image::load_from_memory_with_format(include_bytes!("../../assets/orbit.jpg"), image::ImageFormat::Jpeg)
            .expect("embedded orbital panorama").into_rgba8();
        let texture = device.create_texture_with_data(queue, &wgpu::TextureDescriptor {
            label: Some("KasaDesk orbital horizon"),
            size: wgpu::Extent3d { width: plate.width(), height: plate.height(), depth_or_array_layers: 1 },
            mip_level_count: 1, sample_count: 1, dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm, usage: wgpu::TextureUsages::TEXTURE_BINDING, view_formats: &[],
        }, wgpu::util::TextureDataOrder::LayerMajor, plate.as_raw());
        let texture_view = texture.create_view(&Default::default());
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            mag_filter: wgpu::FilterMode::Linear, min_filter: wgpu::FilterMode::Linear, ..Default::default()
        });
        let library = catalog::library();
        let model_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("KasaDesk product atlases"),
            size: wgpu::Extent3d { width: catalog::ATLAS_EDGE, height: catalog::ATLAS_EDGE,
                depth_or_array_layers: library.assets.len().max(1) as u32 },
            mip_level_count: 1, sample_count: 1, dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm, usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let atlas_view = model_texture.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::D2Array), ..Default::default()
        });
        let count: usize = library.assets.iter().filter_map(|a| a.geometry.as_ref()).map(|g| g.vertices.len() + 6).sum();
        let stride = std::mem::size_of::<mesh::Vertex>() as u64;
        let products = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("KasaDesk product geometry"), size: count.max(1) as u64 * stride,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false,
        });
        let mut ranges = Vec::new();
        let mut start = 0;
        for (layer, asset) in library.assets.iter().enumerate() {
            let Some(g) = &asset.geometry else { ranges.push((0..0, 0..0)); continue };
            let end = start + g.vertices.len() as u32;
            queue.write_buffer(&products, u64::from(start) * stride, bytemuck::cast_slice(&g.vertices));
            let contact = mesh::contact(g.center, g.half);
            queue.write_buffer(&products, u64::from(end) * stride, bytemuck::cast_slice(&contact));
            let next = end + contact.len() as u32;
            ranges.push((start..end, end..next));
            start = next;
            queue.write_texture(wgpu::TexelCopyTextureInfo {
                texture: &model_texture, mip_level: 0, origin: wgpu::Origin3d { x: 0, y: 0, z: layer as u32 }, aspect: wgpu::TextureAspect::All,
            }, &g.atlas, wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(4 * catalog::ATLAS_EDGE), rows_per_image: None },
                wgpu::Extent3d { width: catalog::ATLAS_EDGE, height: catalog::ATLAS_EDGE, depth_or_array_layers: 1 });
        }
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
            mip_level_count: 1, sample_count: 1, dimension: wgpu::TextureDimension::D2, format: DEPTH_FORMAT,
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
                format: DEPTH_FORMAT, depth_write_enabled: true, depth_compare: wgpu::CompareFunction::LessEqual,
                bias: wgpu::DepthBiasState { constant: 2, slope_scale: 2.0, clamp: 0.0 }, stencil: Default::default(),
            }),
            multisample: Default::default(), fragment: None, multiview_mask: None, cache: None,
        });
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
                    depth_compare: wgpu::CompareFunction::LessEqual,
                    stencil: Default::default(),
                    bias: Default::default(),
                }),
                multisample: Default::default(),
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
            ],
        });
        Self { backdrop, solid, glaze, shadows, shadow_depth, shadow_binding, scene, binding, room: None, products, ranges, instances: None, depth: None }
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
    ) {
        let size = texture.size();
        if self.depth.as_ref().is_none_or(|(old, _)| *old != size) {
            let depth = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("KasaDesk room depth"),
                size,
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: DEPTH_FORMAT,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            });
            self.depth = Some((size, depth.create_view(&Default::default())));
        }
        if self.room.as_ref().is_none_or(|(plan, _, _)| *plan != layout.plan) {
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
        let sun = vec3(0.76, 0.45, -0.42).normalize();
        let sun_view = Mat4::look_at_rh(target + sun * (half + 20.0), target, Vec3::Y);
        let sun_clip = Mat4::orthographic_rh(-half, half, -half, half, 0.1, 2.0 * half + 45.0) * sun_view;
        queue.write_buffer(&self.scene, 0, bytemuck::bytes_of(&Scene {
            clip_from_world: frame.clip_from_world.to_cols_array_2d(),
            sun_clip: sun_clip.to_cols_array_2d(),
            eye: frame.eye.extend(0.0).to_array(),
            camera: frame.camera.extend(1.0).to_array(),
            lighting: [frame.day, frame.dusk, frame.indoor, 0.0],
            viewport: [size.width as f32, size.height as f32, 0.0, 0.0],
            bench: [left, right, mesh::DESK_Y, 0.0],
        }));
        // 방 한 칸 + 모형 종류마다 줄 세운 기기 자리.
        let mut data = vec![Instance { position: [0.0; 3], lift: 0.0, glow: 0.0 }];
        let mut ranges = Vec::new();
        for index in 0..self.ranges.len() {
            let start = data.len() as u32;
            for (part, device) in layout.parts.iter().filter(|p| p.asset == Some(index)).map(|p| (p, &layout.devices[p.device])) {
                let lift = if part.lifts { device.lift } else { 0.0 };
                data.push(Instance { position: part.position.to_array(), lift, glow: device.glow });
            }
            ranges.push(start..data.len() as u32);
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
        let (Some((_, instances)), Some((_, vertices, room_count)), Some((_, depth))) = (&self.instances, &self.room, &self.depth) else { return };
        queue.write_buffer(instances, 0, bytemuck::cast_slice(&data));
        {
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
            pass.set_vertex_buffer(0, self.products.slice(..));
            for ((body, _), range) in self.ranges.iter().zip(&ranges) {
                if !range.is_empty() { pass.draw(body.clone(), range.clone()); }
            }
        }
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("KasaDesk room"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: wgpu::StoreOp::Store },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: depth,
                depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Discard }),
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
        pass.set_pipeline(&self.backdrop);
        pass.draw(0..3, 0..1);
        pass.set_pipeline(&self.solid);
        pass.draw(0..*room_count, 0..1);
        pass.set_vertex_buffer(0, self.products.slice(..));
        for ((body, _), range) in self.ranges.iter().zip(&ranges) {
            if !range.is_empty() {
                pass.draw(body.clone(), range.clone());
            }
        }
        pass.set_pipeline(&self.glaze);
        for ((_, contact), range) in self.ranges.iter().zip(&ranges) {
            if !range.is_empty() {
                pass.draw(contact.clone(), range.clone());
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
