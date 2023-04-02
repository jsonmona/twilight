use crate::client::TwilightClientEvent;
use crate::util::NonSend;
use crate::viewer::desktop_view::DesktopView;
use crate::viewer::display_state::DisplayState;
use cfg_if::cfg_if;
use log::{error, info};
use std::time::{Duration, Instant};
use tokio::runtime::Handle;
use winit::event;
use winit::event::{Event, WindowEvent};
use winit::event_loop::{ControlFlow, EventLoop, EventLoopBuilder, EventLoopProxy};
use winit::window::WindowBuilder;

#[derive(Debug)]
pub struct ViewerApp {
    event_loop: EventLoop<TwilightClientEvent>,
    _rt: Handle,
    _guard: NonSend,
}

impl ViewerApp {
    /// Must be called from main thread
    pub fn new(rt: Handle) -> Self {
        ViewerApp {
            event_loop: EventLoopBuilder::<TwilightClientEvent>::with_user_event().build(),
            _rt: rt,
            _guard: Default::default(),
        }
    }

    pub fn create_proxy(&self) -> EventLoopProxy<TwilightClientEvent> {
        self.event_loop.create_proxy()
    }

    /// On native platform, use pollster to drive this function
    pub async fn launch(self) -> ! {
        let window = WindowBuilder::new().build(&self.event_loop).unwrap();

        let mut display_state;
        let mut desktop_view: Option<DesktopView> = None;

        cfg_if! {
            if #[cfg(target_family = "wasm")] {
                // wasm needs real await
                display_state = Some(DisplayState::new(&window).await.unwrap());
            } else {
                display_state = None;
            }
        }

        let mut old_time = Instant::now();
        let mut frames = 0u32;

        self.event_loop
            .run(move |event, _, control_flow| match event {
                Event::NewEvents(event::StartCause::Init) => {
                    *control_flow = ControlFlow::Wait;
                }
                Event::Resumed => {
                    // Initialize graphic state here
                    //TODO: What to do when display_state is not none? (relevant on mobile platforms)
                    if display_state.is_none() {
                        display_state =
                            Some(pollster::block_on(DisplayState::new(&window)).unwrap());
                    }
                }
                Event::MainEventsCleared => {}
                Event::UserEvent(kind) => match kind {
                    TwilightClientEvent::Connected { width, height } => {
                        info!("Connected to {width}x{height}");
                        let state = display_state.as_mut().unwrap();
                        desktop_view = Some(DesktopView::new(state, width, height));
                    }
                    TwilightClientEvent::NextFrame(update) => {
                        window.request_redraw();
                        desktop_view
                            .as_mut()
                            .expect("resolution not set before render")
                            .update(update);
                    }
                    TwilightClientEvent::Closed(r) => {
                        r.unwrap();
                        *control_flow = ControlFlow::Exit;
                    }
                },
                Event::RedrawRequested(_) => {
                    *control_flow = ControlFlow::Wait;

                    let state = display_state.as_mut().unwrap();

                    let elapsed = Instant::now() - old_time;
                    if elapsed > Duration::from_secs(10) {
                        let fps = frames as f64 / elapsed.as_secs_f64();
                        info!("Render FPS={fps:.2}");
                        old_time = Instant::now();
                        frames = 0;
                    }
                    frames += 1;

                    let render_result = match desktop_view.as_mut() {
                        Some(x) => x.render(state),
                        None => state.render_empty(),
                    };

                    match render_result {
                        Ok(_) => {}
                        Err(wgpu::SurfaceError::Lost) => {
                            state.reconfigure_surface();
                            window.request_redraw();
                        }
                        Err(e) => {
                            error!("{e}");
                            *control_flow = ControlFlow::ExitWithCode(1);
                        }
                    }
                }
                Event::WindowEvent {
                    ref event,
                    window_id,
                } if window_id == window.id() => {
                    let state = display_state.as_mut().unwrap();

                    //TODO: Handle input here
                    //FIXME: Do I need to render here?

                    match event {
                        WindowEvent::CloseRequested => {
                            *control_flow = ControlFlow::Exit;
                        }
                        WindowEvent::Resized(physical_size) => {
                            state.resize(*physical_size);
                            window.request_redraw();
                        }
                        WindowEvent::ScaleFactorChanged { new_inner_size, .. } => {
                            state.resize(**new_inner_size);
                            window.request_redraw();
                        }
                        _ => {}
                    }
                }
                Event::LoopDestroyed => {}
                _ => {}
            })
    }
}
