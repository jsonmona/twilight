use crate::image::ImageBuf;
use std::sync::Mutex;
use wgpu::{BindGroup, CommandEncoder, Device, Queue, SurfaceConfiguration, Texture, TextureView};

pub struct DesktopDisplayState {
    next_img: Mutex<Option<ImageBuf>>,
    render_pipeline: wgpu::RenderPipeline,
    desktop_texture: Texture,
    bind_group: BindGroup,
}

impl DesktopDisplayState {
    pub fn new(device: &Device, surface_config: &SurfaceConfiguration, image: ImageBuf) -> Self {
        let desktop_extent = wgpu::Extent3d {
            width: image.width,
            height: image.height,
            depth_or_array_layers: 1,
        };

        let desktop_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("desktop_texture"),
            size: desktop_extent,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Bgra8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[wgpu::TextureFormat::Bgra8Unorm],
        });

        let desktop_view = desktop_texture.create_view(&Default::default());
        let desktop_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("desktop sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("desktop texture bind group layout"),
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
            label: Some("desktop texture bind group"),
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
            ],
        });

        let shader = device.create_shader_module(wgpu::include_wgsl!("display.wgsl"));

        let render_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Render Pipeline Layout"),
                bind_group_layouts: &[&bind_group_layout],
                push_constant_ranges: &[],
            });

        let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Render Pipeline"),
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

        DesktopDisplayState {
            next_img: Mutex::new(Some(image)),
            desktop_texture,
            bind_group,
            render_pipeline,
        }
    }

    pub fn update(&mut self, img: ImageBuf) {
        //assert!(tex.width() == img.width && tex.height() == img.height, "image size changed");

        let mut next_img = self.next_img.lock().unwrap();
        *next_img = Some(img);
    }

    pub fn render(
        &mut self,
        _device: &Device,
        queue: &Queue,
        output_view: &TextureView,
        encoder: &mut CommandEncoder,
    ) -> Result<(), wgpu::SurfaceError> {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("desktop render pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: output_view,
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

        {
            let mut next_img = self.next_img.lock().unwrap();
            if let Some(img) = next_img.take() {
                let desktop_size = wgpu::Extent3d {
                    width: img.width,
                    height: img.height,
                    depth_or_array_layers: 1,
                };

                queue.write_texture(
                    wgpu::ImageCopyTexture {
                        texture: &self.desktop_texture,
                        mip_level: 0,
                        origin: wgpu::Origin3d::ZERO,
                        aspect: wgpu::TextureAspect::All,
                    },
                    &img.data,
                    wgpu::ImageDataLayout {
                        offset: 0,
                        bytes_per_row: Some((img.width * 4).try_into().unwrap()),
                        rows_per_image: Some(img.height.try_into().unwrap()),
                    },
                    desktop_size,
                );
            }
        }

        pass.set_pipeline(&self.render_pipeline);
        pass.set_bind_group(0, &self.bind_group, &[]);
        pass.draw(0..3, 0..1);

        Ok(())
    }
}
