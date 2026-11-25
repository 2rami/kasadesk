use std::ops::Range;

use super::mesh::{self, Vertex};
use glam::Vec3;

pub(super) struct IndexedModel {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
    pub body: Range<u32>,
    pub far: Range<u32>,
    pub glass: Range<u32>,
    pub contact: Range<u32>,
}

impl IndexedModel {
    pub fn new(source: &[Vertex], opaque_count: usize, center: Vec3, half: Vec3) -> Self {
        let (unique, remap) = meshopt::generate_vertex_remap(source, None);
        let mut vertices = meshopt::remap_vertex_buffer(source, unique, &remap);
        let (body, glass) = remap.split_at(opaque_count);
        let mut indices = meshopt::optimize_vertex_cache(body, unique);
        let end = indices.len() as u32;

        // A 0.3% object-space error stays below a pixel at the far-detail threshold.
        // Lock displays and addressable emitters so their sampled regions keep their boundaries.
        let stride = std::mem::size_of::<Vertex>();
        let adapter = meshopt::VertexDataAdapter::new(bytemuck::cast_slice(&vertices), stride, 0).unwrap();
        let attributes = &bytemuck::cast_slice::<_, f32>(&vertices)[3..];
        let locked: Vec<_> = vertices.iter().map(|v| matches!(v.material[3] as u32, 17..=30 | 96..)).collect();
        let reduced = meshopt::simplify_with_attributes_and_locks(
            body,
            &adapter,
            attributes,
            &[0.1, 0.1, 0.1, 0.25, 0.25, 0.25, 0.0, 0.8, 0.8],
            stride,
            &locked,
            body.len() / 9 * 3,
            0.003,
            meshopt::SimplifyOptions::empty(),
            None,
        );
        let far = if !reduced.is_empty() && reduced.len() < body.len() {
            indices.extend(meshopt::optimize_vertex_cache(&reduced, unique));
            end..indices.len() as u32
        } else {
            0..end
        };
        let glass_start = indices.len() as u32;
        indices.extend_from_slice(glass);
        let contact_start = indices.len() as u32;
        let first = vertices.len() as u32;
        vertices.extend(mesh::contact(center, half));
        indices.extend(first..vertices.len() as u32);
        let total = indices.len() as u32;
        Self { vertices, indices, body: 0..end, far, glass: glass_start..contact_start, contact: contact_start..total }
    }
}
