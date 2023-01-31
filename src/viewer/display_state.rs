use crate::image::Image;
use crate::viewer::desktop_display_state::DesktopDisplayState;
use anyhow::{ensure, Context, Result};
use winit::event::WindowEvent;
use winit::window::Window;

pub struct DisplayState {
    surface: wgpu::Surface,
    device: wgpu::Device,
    queue: wgpu::Queue,
    surface_config: wgpu::SurfaceConfiguration,
    size: winit::dpi::PhysicalSize<u32>,
    desktop_display: Option<DesktopDisplayState>,
}

impl DisplayState {
    // must be called from main thread (metal specific)
    pub async fn new(window: &Window) -> Result<Self> {
        let size = window.inner_size();

        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::PRIMARY,
            dx12_shader_compiler: wgpu::Dx12Compiler::Fxc,
        });

        let surface = unsafe { instance.create_surface(window) }.expect("unable to create surface");
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::LowPower,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .context("no compatible adapter found")?;

        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    features: wgpu::Features::empty(),
                    limits: wgpu::Limits::downlevel_webgl2_defaults()
                        .using_resolution(adapter.limits()),
                    label: None,
                },
                None,
            )
            .await?;

        ensure!(
            size.width > 0 && size.height > 0,
            "initial window size must not be zero"
        );

        let surface_config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: wgpu::TextureFormat::Bgra8Unorm,
            width: size.width,
            height: size.height,
            present_mode: wgpu::PresentMode::AutoVsync,
            alpha_mode: wgpu::CompositeAlphaMode::Opaque,
            view_formats: vec![wgpu::TextureFormat::Bgra8Unorm],
        };
        surface.configure(&device, &surface_config);

        Ok(Self {
            surface,
            device,
            queue,
            surface_config,
            size,
            desktop_display: None,
        })
    }

    pub fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) {
        if new_size.width > 0 && new_size.height > 0 {
            self.size = new_size;
            self.surface_config.width = new_size.width;
            self.surface_config.height = new_size.height;
            self.surface.configure(&self.device, &self.surface_config);
        }
    }

    pub fn reconfigure_surface(&mut self) {
        self.resize(self.size);
    }

    pub fn input(&mut self, _event: &WindowEvent) -> bool {
        false
    }

    pub fn update(&mut self, img: Image) {
        match self.desktop_display.as_mut() {
            Some(x) => x.update(img),
            None => {
                let created = DesktopDisplayState::new(&self.device, &self.surface_config, img);
                self.desktop_display = Some(created);
            }
        }
    }

    pub fn render(&mut self) -> Result<(), wgpu::SurfaceError> {
        let output = self.surface.get_current_texture()?;
        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("render encoder"),
            });

        match self.desktop_display.as_mut() {
            Some(x) => {
                x.render(&self.device, &self.queue, &view, &mut encoder)?;
            }
            None => {
                encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("empty render pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &view,
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
            }
        }

        self.queue.submit(std::iter::once(encoder.finish()));
        output.present();

        Ok(())
    }
}
