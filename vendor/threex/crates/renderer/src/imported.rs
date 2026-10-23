//! ATOM extension: immutable imported triangle buffers, owned entirely by wgpu/Rust.
use std::collections::{BTreeMap, BTreeSet};
use wgpu::util::DeviceExt;
pub struct MeshView<'a> {
    pub instance: u32,
    pub key: &'a str,
    pub positions: &'a [[f32; 3]],
    pub colors: &'a [[f32; 3]],
    pub indices: &'a [u32],
}
struct Buffers {
    vertices: wgpu::Buffer,
    indices: wgpu::Buffer,
    count: u32,
}
pub struct Imported {
    pub pipeline: wgpu::RenderPipeline,
    buffers: BTreeMap<String, Buffers>,
    mapping: BTreeMap<u32, String>,
}
impl Imported {
    pub fn new(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        layout: &wgpu::PipelineLayout,
    ) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("OpenUSD mesh"),
            source: wgpu::ShaderSource::Wgsl(include_str!("imported.wgsl").into()),
        });
        let pipeline=device.create_render_pipeline(&wgpu::RenderPipelineDescriptor{label:Some("OpenUSD mesh"),layout:Some(layout),vertex:wgpu::VertexState{module:&shader,entry_point:Some("vs"),compilation_options:Default::default(),buffers:&[wgpu::VertexBufferLayout{array_stride:24,step_mode:wgpu::VertexStepMode::Vertex,attributes:&wgpu::vertex_attr_array![0=>Float32x3,6=>Float32x3]},wgpu::VertexBufferLayout{array_stride:80,step_mode:wgpu::VertexStepMode::Instance,attributes:&wgpu::vertex_attr_array![1=>Float32x4,2=>Float32x4,3=>Float32x4,4=>Float32x4,5=>Float32x4]}]},fragment:Some(wgpu::FragmentState{module:&shader,entry_point:Some("fs"),compilation_options:Default::default(),targets:&[Some(wgpu::ColorTargetState{format,blend:None,write_mask:wgpu::ColorWrites::ALL})]}),primitive:wgpu::PrimitiveState::default(),depth_stencil:Some(wgpu::DepthStencilState{format:wgpu::TextureFormat::Depth32Float,depth_write_enabled:true,depth_compare:wgpu::CompareFunction::Less,stencil:Default::default(),bias:Default::default()}),multisample:Default::default(),multiview:None,cache:None});
        Self {
            pipeline,
            buffers: BTreeMap::new(),
            mapping: BTreeMap::new(),
        }
    }
    pub fn sync<'a>(
        &mut self,
        device: &wgpu::Device,
        meshes: impl IntoIterator<Item = MeshView<'a>>,
    ) -> Result<(), String> {
        let meshes: Vec<_> = meshes.into_iter().collect();
        let mut bytes = 0usize;
        for m in &meshes {
            bytes = bytes.saturating_add(m.positions.len() * 24 + m.indices.len() * 4);
            if m.positions.len() < 3
                || m.positions.len() > 300_000
                || m.colors.len() != m.positions.len()
                || m.indices.len() > 900_000
                || m.indices.len() % 3 != 0
                || m.indices.iter().any(|&i| i as usize >= m.positions.len())
                || m.positions.iter().flatten().any(|v| !v.is_finite())
                || m.colors.iter().flatten().any(|v| !v.is_finite())
                || bytes > 64 * 1024 * 1024
                || bytes as u64 > device.limits().max_buffer_size
            {
                return Err("invalid imported mesh or GPU budget".into());
            }
        }
        let active: BTreeSet<_> = meshes.iter().map(|m| m.key.to_string()).collect();
        self.buffers.retain(|key, _| active.contains(key));
        self.mapping.clear();
        for m in meshes {
            self.mapping.insert(m.instance, m.key.into());
            self.buffers.entry(m.key.into()).or_insert_with(|| {
                let vertices: Vec<[f32; 6]> = m
                    .positions
                    .iter()
                    .zip(m.colors)
                    .map(|(p, c)| [p[0], p[1], p[2], c[0], c[1], c[2]])
                    .collect();
                Buffers {
                    vertices: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("OpenUSD vertices"),
                        contents: bytemuck::cast_slice(&vertices),
                        usage: wgpu::BufferUsages::VERTEX,
                    }),
                    indices: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("OpenUSD indices"),
                        contents: bytemuck::cast_slice(m.indices),
                        usage: wgpu::BufferUsages::INDEX,
                    }),
                    count: m.indices.len() as u32,
                }
            });
        }
        Ok(())
    }
    pub fn is_empty(&self) -> bool {
        self.mapping.is_empty()
    }
    pub fn contains(&self, index: u32) -> bool {
        self.mapping.contains_key(&index)
    }
    pub fn draw(&self, pass: &mut wgpu::RenderPass<'_>, instances: &wgpu::Buffer) {
        pass.set_pipeline(&self.pipeline);
        pass.set_vertex_buffer(1, instances.slice(..));
        for (&index, key) in &self.mapping {
            let b = &self.buffers[key];
            pass.set_vertex_buffer(0, b.vertices.slice(..));
            pass.set_index_buffer(b.indices.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..b.count, 0, index..index + 1);
        }
    }
}
