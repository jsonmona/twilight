use crate::image::{convert_color, ColorFormat, ImageBuf};
use crate::util::DesktopUpdate;
use crate::viewer::display_state::DisplayState;
use bytemuck::{Pod, Zeroable};
use std::convert::Infallible;
use wgpu::util::DeviceExt;
use wgpu::{BindGroup, Buffer, Texture};

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
struct Uniform {
    visible: u32,
    xor_cursor: u32,
    cursor_relative_size: [f32; 2],
    cursor_pos: [f32; 2],
    _unused: [u32; 2],
}

pub struct ComposeRenderable {
    render_pipeline: wgpu::RenderPipeline,
    uniform_buffer: Buffer,
    desktop_texture: Texture,
    cursor_texture: Texture,
    bind_group: BindGroup,
    xor: bool,
}

impl ComposeRenderable {
    pub fn new(display: &DisplayState, width: u32, height: u32) -> Self {
        assert_eq!(
            std::mem::size_of::<Uniform>() % 16,
            0,
            "buffer size must be multiple of 16"
        );

        let device = &display.device;
        let surface_config = &display.surface_config;

        let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("ComposeView uniform_buffer"),
            contents: bytemuck::bytes_of(&Uniform::zeroed()),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let desktop_extent = wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        };

        let desktop_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("ComposeView desktop_texture"),
            size: desktop_extent,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Bgra8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[wgpu::TextureFormat::Bgra8Unorm],
        });

        let cursor_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("ComposeView cursor_texture"),
            size: wgpu::Extent3d {
                width: 128,
                height: 128,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Bgra8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[wgpu::TextureFormat::Bgra8Unorm],
        });

        let desktop_view = desktop_texture.create_view(&Default::default());
        let desktop_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("ComposeView desktop_sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        let cursor_view = cursor_texture.create_view(&Default::default());
        let cursor_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("ComposeView cursor_sampler"),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("ComposeView bind_group_layout"),
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
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("ComposeView bind_group"),
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&desktop_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&desktop_sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&cursor_view),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::Sampler(&cursor_sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::Buffer(
                        uniform_buffer.as_entire_buffer_binding(),
                    ),
                },
            ],
        });

        let shader = device.create_shader_module(wgpu::include_wgsl!("compose.wgsl"));

        let render_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("ComposeView render_pipeline_layout"),
                bind_group_layouts: &[&bind_group_layout],
                push_constant_ranges: &[],
            });

        let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("ComposeView render_pipeline"),
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

        ComposeRenderable {
            desktop_texture,
            cursor_texture,
            bind_group,
            render_pipeline,
            uniform_buffer,
            xor: false,
        }
    }

    pub fn render(
        &mut self,
        state: &DisplayState,
        update: DesktopUpdate<ImageBuf>,
        dst: &Texture,
    ) -> Result<(), Infallible> {
        let output_view = dst.create_view(&Default::default());

        let mut encoder = state
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("ComposeView command_encoder"),
            });

        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("ComposeView render_pass"),
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
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });

        let desktop_img = update.desktop;
        let desktop_img = if desktop_img.color_format == ColorFormat::Bgra8888 {
            desktop_img
        } else {
            let mut copy_img = ImageBuf::alloc(
                desktop_img.width,
                desktop_img.height,
                None,
                ColorFormat::Bgra8888,
            );
            convert_color(&desktop_img, &mut copy_img);
            copy_img
        };

        let desktop_size = wgpu::Extent3d {
            width: desktop_img.width,
            height: desktop_img.height,
            depth_or_array_layers: 1,
        };

        state.queue.write_texture(
            wgpu::ImageCopyTexture {
                texture: &self.desktop_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &desktop_img.data,
            wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(desktop_img.stride),
                rows_per_image: Some(desktop_img.height),
            },
            desktop_size,
        );

        if let Some(cursor_state) = update.cursor {
            if let Some(shape) = cursor_state.shape {
                let mut temp_img = ImageBuf::alloc(128, 128, None, ColorFormat::Bgra8888);

                assert_eq!(shape.image.color_format, ColorFormat::Bgra8888);
                for i in 0..shape.image.height as usize {
                    for j in 0..shape.image.width as usize {
                        for k in 0..4 {
                            temp_img.data[i * temp_img.stride as usize + j * 4 + k] =
                                shape.image.data[i * shape.image.stride as usize + j * 4 + k];
                        }
                    }
                }

                state.queue.write_texture(
                    wgpu::ImageCopyTexture {
                        texture: &self.cursor_texture,
                        mip_level: 0,
                        origin: wgpu::Origin3d::ZERO,
                        aspect: wgpu::TextureAspect::All,
                    },
                    &temp_img.data,
                    wgpu::ImageDataLayout {
                        offset: 0,
                        bytes_per_row: Some(temp_img.stride),
                        rows_per_image: Some(temp_img.height),
                    },
                    wgpu::Extent3d {
                        width: 128,
                        height: 128,
                        depth_or_array_layers: 1,
                    },
                );

                self.xor = shape.xor;
            }

            let uniform = Uniform {
                visible: cursor_state.visible as u32,
                xor_cursor: self.xor as u32,
                cursor_relative_size: [
                    desktop_img.width as f32 / 128.,
                    desktop_img.height as f32 / 128.,
                ],
                cursor_pos: [
                    cursor_state.pos_x as f32 / desktop_img.width as f32,
                    cursor_state.pos_y as f32 / desktop_img.height as f32,
                ],
                _unused: Default::default(),
            };

            state
                .queue
                .write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(&uniform));
        }

        pass.set_pipeline(&self.render_pipeline);
        pass.set_bind_group(0, &self.bind_group, &[]);
        pass.draw(0..3, 0..1);
        drop(pass);

        state.queue.submit(std::iter::once(encoder.finish()));

        Ok(())
    }
}
