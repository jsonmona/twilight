use crate::image::ImageBuf;
use crate::util::DesktopUpdate;
use crate::viewer::compose_renderable::ComposeRenderable;
use crate::viewer::display_state::DisplayState;
use wgpu::{BindGroup, RenderPipeline, Texture};

pub struct DesktopView {
    composer: ComposeRenderable,
    compose_texture: Texture,
    bind_group: BindGroup,
    render_pipeline: RenderPipeline,
    next_update: Option<DesktopUpdate<ImageBuf>>,
}

impl DesktopView {
    pub fn new(display: &DisplayState, width: u32, height: u32) -> Self {
        let device = &display.device;
        let surface_config = &display.surface_config;

        let desktop_extent = wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        };

        let compose_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("DesktopView compose_texture"),
            size: desktop_extent,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Bgra8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[wgpu::TextureFormat::Bgra8Unorm],
        });

        let compose_view = compose_texture.create_view(&Default::default());
        let compose_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("DesktopView compose_sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("DesktopView bind_group_layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("desktop bind group"),
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&compose_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&compose_sampler),
                },
            ],
        });

        let shader = device.create_shader_module(wgpu::include_wgsl!("display.wgsl"));

        let render_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("DesktopView render_pipeline_layout"),
                bind_group_layouts: &[&bind_group_layout],
                push_constant_ranges: &[],
            });

        let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("DesktopView render_pipeline"),
            layout: Some(&render_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs_main",
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "fs_main",
                targets: &[Some(wgpu::ColorTargetState {
                    format: surface_config.format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: Some(wgpu::Face::Back),
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview: None,
        });

        DesktopView {
            composer: ComposeRenderable::new(display, width, height),
            compose_texture,
            bind_group,
            render_pipeline,
            next_update: None,
        }
    }

    pub fn update(&mut self, mut update: DesktopUpdate<ImageBuf>) {
        let next_width = update.desktop.width;
        let next_height = update.desktop.height;
        assert!(
            self.compose_texture.width() == next_width
                && self.compose_texture.height() == next_height,
            "image size must not change"
        );

        if let Some(prev) = self.next_update.take() {
            update.collapse_from(prev);
        }

        self.next_update = Some(update);
    }

    pub fn render(&mut self, state: &DisplayState) -> Result<(), wgpu::SurfaceError> {
        let output = state.surface.get_current_texture()?;
        let output_view = output.texture.create_view(&Default::default());

        if let Some(update) = self.next_update.take() {
            self.composer
                .render(state, update, &self.compose_texture)
                .expect("cannot fail");
        }

        let mut encoder = state
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("DesktopView command_encoder"),
            });

        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("DesktopView render_pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &output_view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: 0.0,
                        g: 0.0,
                        b: 0.0,
                        a: 1.0,
                    }),
                    store: true,
                },
            })],
            depth_stencil_attachment: None,
        });

        pass.set_pipeline(&self.render_pipeline);
        pass.set_bind_group(0, &self.bind_group, &[]);
        pass.draw(0..3, 0..1);
        drop(pass);

        state.queue.submit(std::iter::once(encoder.finish()));
        output.present();

        Ok(())
    }
}
