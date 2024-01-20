use crate::client::TwilightClientEvent;
use crate::util::NonSend;
use crate::viewer::desktop_view::DesktopView;
use crate::viewer::display_state::DisplayState;
use anyhow::Result;
use cfg_if::cfg_if;
use log::{error, info};
use std::time::{Duration, Instant};
use tokio::runtime::Handle;
use winit::event;
use winit::event::{Event, WindowEvent};
use winit::event_loop::{ControlFlow, EventLoop, EventLoopBuilder, EventLoopProxy};
use winit::window::WindowBuilder;

pub struct ViewerApp {
    event_loop: EventLoop<TwilightClientEvent>,
    on_exit: Option<Box<dyn FnOnce() -> Result<()>>>,
    _rt: Handle,
    _guard: NonSend,
}

impl ViewerApp {
    /// Must be called from main thread
    pub fn new(rt: Handle) -> Self {
        ViewerApp {
            event_loop: EventLoopBuilder::<TwilightClientEvent>::with_user_event()
                .build()
                .unwrap(),
            on_exit: None,
            _rt: rt,
            _guard: Default::default(),
        }
    }

    pub fn set_on_exit(&mut self, callback: Box<dyn FnOnce() -> Result<()>>) {
        self.on_exit = Some(callback);
    }

    pub fn create_proxy(&self) -> EventLoopProxy<TwilightClientEvent> {
        self.event_loop.create_proxy()
    }

    /// On native platform, use pollster to drive this function
    pub async fn launch(mut self) -> ! {
        let window = WindowBuilder::new().build(&self.event_loop).unwrap();

        let mut display_state: Option<DisplayState<'static>>;
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
            .run(move |event, event_loop| match event {
                Event::NewEvents(event::StartCause::Init) => {
                    event_loop.set_control_flow(ControlFlow::Wait);
                }
                Event::Resumed => {
                    // Initialize graphic state here
                    //TODO: What to do when display_state is not none? (relevant on mobile platforms)
                    if display_state.is_none() {
                        log::error!("Unsound transmute of Window.");
                        //FIXME: Killing lifetime to maintain previous code structure.
                        //       Will remove this when refactoring.
                        display_state = unsafe {
                            let ds = pollster::block_on(DisplayState::new(&window)).unwrap();
                            let ds = std::mem::transmute(ds);
                            Some(ds)
                        };
                    }
                }
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
                        //FIXME: Anything better to do than unwrap?
                        r.unwrap();
                        event_loop.exit();
                    }
                },
                Event::WindowEvent {
                    window_id,
                    ref event,
                } if window_id == window.id() => {
                    let state = display_state.as_mut().unwrap();

                    //TODO: Handle input here
                    //FIXME: Do I need to render here?

                    match event {
                        WindowEvent::CloseRequested => {
                            event_loop.exit();
                        }
                        WindowEvent::Resized(physical_size) => {
                            state.resize(*physical_size);
                            window.request_redraw();
                        }
                        WindowEvent::ScaleFactorChanged {
                            inner_size_writer, ..
                        } => {
                            //FIXME: What should I do?
                            //state.resize(**inner_size_writer);
                            window.request_redraw();
                        }
                        WindowEvent::RedrawRequested => {
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
                                    event_loop.exit();
                                }
                            }
                        }
                        _ => {}
                    }
                }
                Event::LoopExiting => {
                    if let Some(f) = self.on_exit.take() {
                        if let Err(e) = f() {
                            error!("on_exit callback returned an error:\n{e:?}");
                        }
                    }
                }
                _ => {}
            })
            .unwrap();

        panic!("I need to be refactored after library version bump");
    }
}
