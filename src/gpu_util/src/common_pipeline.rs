#[derive(Clone)]
pub struct ComputePipeline {
    pub pipeline: wgpu::ComputePipeline,
    pub bind_group_layout: wgpu::BindGroupLayout,
}

impl ComputePipeline {
    pub fn new(
        device: &wgpu::Device,
        shader_desc: wgpu::ShaderModuleDescriptor<'static>,
        bind_group_entries: &[wgpu::BindGroupLayoutEntry],
        label_prefix: &str,
    ) -> Self {
        let shader = device.create_shader_module(shader_desc);

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some(&format!("{label_prefix} Bind Group Layout")),
            entries: bind_group_entries,
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some(&format!("{label_prefix} Pipeline Layout")),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some(&format!("{label_prefix} Pipeline")),
            layout: Some(&pipeline_layout),
            module: &shader,
            entry_point: Some("main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            cache: None,
        });

        Self {
            pipeline,
            bind_group_layout,
        }
    }
}

#[derive(Clone)]
pub struct RenderPipeline {
    pub pipeline: wgpu::RenderPipeline,
    pub bind_group_layout: wgpu::BindGroupLayout,
}

impl RenderPipeline {
    pub fn new(
        device: &wgpu::Device,
        shader_desc: wgpu::ShaderModuleDescriptor<'static>,
        bind_group_entries: &[wgpu::BindGroupLayoutEntry],
        target_format: wgpu::TextureFormat,
        label_prefix: &str,
    ) -> Self {
        let shader = device.create_shader_module(shader_desc);

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some(&format!("{label_prefix} Bind Group Layout")),
            entries: bind_group_entries,
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some(&format!("{label_prefix} Pipeline Layout")),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some(&format!("{label_prefix} Pipeline")),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: target_format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        Self {
            pipeline,
            bind_group_layout,
        }
    }
}
