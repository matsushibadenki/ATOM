//! Bounded world-space angular guides. Only small command/state data enters Rust.
use wgpu::util::DeviceExt;
const MAX_LINES: usize = 384;
#[derive(Clone, Debug)]
pub struct Arc {
    pub channel: usize,
    pub axis: [f32; 3],
    pub zero: [f32; 3],
    pub range: [f32; 2],
    pub angle: f32,
}
#[derive(Clone, Debug)]
pub struct JointRanges {
    pub origin: [f32; 3],
    pub arcs: Vec<Arc>,
    pub radius: f32,
    pub active_axis: Option<usize>,
    pub active_bound: Option<(usize, usize)>,
}
/// Shared geometry for drawing and native hit testing; axis indices match the joint state.
pub fn arc_point(view: &JointRanges, index: usize, angle: f32) -> Option<[f32; 3]> {
    let arc = view.arcs.get(index)?;
    if !angle.is_finite() {
        return None;
    }
    let radius = view.radius * (1. + index as f32 * 0.16);
    let tangent = [
        arc.axis[1] * arc.zero[2] - arc.axis[2] * arc.zero[1],
        arc.axis[2] * arc.zero[0] - arc.axis[0] * arc.zero[2],
        arc.axis[0] * arc.zero[1] - arc.axis[1] * arc.zero[0],
    ];
    Some(std::array::from_fn(|j| {
        view.origin[j] + radius * (arc.zero[j] * angle.cos() + tangent[j] * angle.sin())
    }))
}
pub fn axis_line(view: &JointRanges, index: usize) -> Option<[[f32; 3]; 2]> {
    let arc = view.arcs.get(index)?;
    let radius = view.radius * (1. + index as f32 * 0.16);
    Some([
        std::array::from_fn(|j| view.origin[j] - arc.axis[j] * radius * 0.3),
        std::array::from_fn(|j| view.origin[j] + arc.axis[j] * radius * 0.3),
    ])
}
/// Lower/upper grip bars are offset radially so coincident ±pi endpoints remain distinguishable.
pub fn limit_handle(view: &JointRanges, index: usize, bound: usize) -> Option<[[f32; 3]; 2]> {
    let arc = view.arcs.get(index)?;
    let angle = *arc.range.get(bound)?;
    let end = arc_point(view, index, angle)?;
    let center: [f32; 3] = std::array::from_fn(|j| {
        view.origin[j] + (end[j] - view.origin[j]) * if bound == 0 { 0.88 } else { 1.12 }
    });
    let radius = view.radius * (1. + index as f32 * 0.16);
    Some([
        std::array::from_fn(|j| center[j] - arc.axis[j] * radius * 0.1),
        std::array::from_fn(|j| center[j] + arc.axis[j] * radius * 0.1),
    ])
}
fn lines(view: &JointRanges) -> Vec<[f32; 11]> {
    let mut lines = Vec::new();
    if !view.origin.iter().all(|v| v.is_finite()) || !view.radius.is_finite() || view.radius <= 0. {
        return lines;
    }
    for (i, arc) in view.arcs.iter().take(3).enumerate() {
        if arc
            .axis
            .iter()
            .chain(&arc.zero)
            .chain(&arc.range)
            .chain(std::iter::once(&arc.angle))
            .any(|v| !v.is_finite())
            || arc.range[0] >= arc.range[1]
        {
            continue;
        }
        let colors = [[0.96, 0.43, 0.32], [0.45, 0.83, 0.50], [0.45, 0.66, 1.]];
        let active = view.active_axis.is_none_or(|a| a == i);
        let alpha = if active { 0.95 } else { 0.35 };
        let channel = arc.channel.min(2);
        let color = [
            colors[channel][0],
            colors[channel][1],
            colors[channel][2],
            alpha,
        ];
        let point = |angle: f32| arc_point(view, i, angle).unwrap();
        let mut add = |a: [f32; 3], b: [f32; 3], color: [f32; 4], width: f32| {
            lines.push([
                a[0], a[1], a[2], b[0], b[1], b[2], color[0], color[1], color[2], color[3], width,
            ]);
        };
        for step in 0..96 {
            let angle = |s| arc.range[0] + (arc.range[1] - arc.range[0]) * s as f32 / 96.;
            add(
                point(angle(step)),
                point(angle(step + 1)),
                color,
                if active { 2.2 } else { 1.4 },
            );
        }
        for (bound, angle) in arc.range.into_iter().enumerate() {
            let p = point(angle);
            add(view.origin, p, color, 1.3);
            let [a, b] = limit_handle(view, i, bound).unwrap();
            let marker = if view.active_bound == Some((i, bound)) {
                [1., 1., 0.85, 1.]
            } else if bound == 0 {
                [0.40, 0.86, 0.88, alpha]
            } else {
                [0.94, 0.59, 0.84, alpha]
            };
            add(p, std::array::from_fn(|j| (a[j] + b[j]) * 0.5), marker, 1.8);
            add(
                a,
                b,
                marker,
                if view.active_bound == Some((i, bound)) {
                    5.
                } else {
                    3.5
                },
            );
        }
        add(view.origin, point(0.), [0.75, 0.73, 0.67, alpha * 0.5], 1.);
        add(
            view.origin,
            point(arc.angle),
            [1., 0.86, 0.48, alpha],
            if active { 3.4 } else { 1.8 },
        );
        let [start, end] = axis_line(view, i).unwrap();
        add(start, end, color, 2.2);
    }
    lines
}
pub(crate) struct Ranges {
    pipeline: wgpu::RenderPipeline,
    uniform: wgpu::Buffer,
    bind: wgpu::BindGroup,
    segments: wgpu::Buffer,
}
impl Ranges {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let uniform = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("joint range camera"),
            contents: &[0; 80],
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform.as_entire_binding(),
            }],
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("joint range guides"),
            source: wgpu::ShaderSource::Wgsl(include_str!("joint_ranges.wgsl").into()),
        });
        let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[&layout],
            push_constant_ranges: &[],
        });
        let pipeline=device.create_render_pipeline(&wgpu::RenderPipelineDescriptor{label:Some("joint angular limits"),layout:Some(&pl),vertex:wgpu::VertexState{module:&shader,entry_point:Some("vs"),compilation_options:Default::default(),buffers:&[wgpu::VertexBufferLayout{array_stride:44,step_mode:wgpu::VertexStepMode::Instance,attributes:&wgpu::vertex_attr_array![0=>Float32x3,1=>Float32x3,2=>Float32x4,3=>Float32]}]},fragment:Some(wgpu::FragmentState{module:&shader,entry_point:Some("fs"),compilation_options:Default::default(),targets:&[Some(wgpu::ColorTargetState{format,blend:Some(wgpu::BlendState::ALPHA_BLENDING),write_mask:wgpu::ColorWrites::ALL})]}),primitive:Default::default(),depth_stencil:None,multisample:Default::default(),multiview:None,cache:None});
        let segments = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("bounded joint arc segments"),
            size: (MAX_LINES * 44) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Self {
            pipeline,
            uniform,
            bind,
            segments,
        }
    }
    #[allow(clippy::too_many_arguments)]
    pub fn encode(
        &self,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        region: [u32; 4],
        vp: &[f32; 16],
        scale: f32,
        view: &JointRanges,
    ) {
        let lines = lines(view);
        if lines.is_empty() {
            return;
        }
        assert!(lines.len() <= MAX_LINES);
        let mut uniform = [0f32; 20];
        uniform[..16].copy_from_slice(vp);
        uniform[16..20].copy_from_slice(&[
            region[2] as f32,
            region[3] as f32,
            scale.clamp(0.5, 4.),
            0.,
        ]);
        queue.write_buffer(&self.uniform, 0, bytemuck::cast_slice(&uniform));
        queue.write_buffer(&self.segments, 0, bytemuck::cast_slice(&lines));
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("joint range overlay"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            ..Default::default()
        });
        pass.set_viewport(
            region[0] as f32,
            region[1] as f32,
            region[2] as f32,
            region[3] as f32,
            0.,
            1.,
        );
        pass.set_scissor_rect(region[0], region[1], region[2], region[3]);
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.bind, &[]);
        pass.set_vertex_buffer(0, self.segments.slice(..));
        pass.draw(0..6, 0..lines.len() as u32);
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn arc_bounds_and_current_direction_are_correct() {
        let view = JointRanges {
            origin: [1., 2., 3.],
            arcs: vec![Arc {
                channel: 2,
                axis: [0., 0., 1.],
                zero: [1., 0., 0.],
                range: [0., std::f32::consts::FRAC_PI_2],
                angle: std::f32::consts::FRAC_PI_2,
            }],
            radius: 1.,
            active_axis: Some(0),
            active_bound: None,
        };
        let data = lines(&view);
        assert_eq!(data.len(), 105);
        assert!(data.iter().flatten().all(|v| v.is_finite()));
        assert!((data[0][0] - 2.).abs() < 1e-5);
        assert!((data[95][4] - 3.).abs() < 1e-5);
        assert!(
            lines(&JointRanges {
                radius: f32::NAN,
                ..view
            })
            .is_empty()
        );
    }
    #[test]
    fn shader_validates() {
        let module = naga::front::wgsl::parse_str(include_str!("joint_ranges.wgsl")).unwrap();
        naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        )
        .validate(&module)
        .unwrap();
    }
}
