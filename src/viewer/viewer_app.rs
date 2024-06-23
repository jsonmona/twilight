use crate::client::TwilightClientEvent;
use crate::util::NonSend;
use crate::viewer::desktop_view::DesktopView;
use crate::viewer::display_state::DisplayState;
use anyhow::Result;
use log::{error, info};
use std::io::Write;
use std::rc::Rc;
use std::time::{Duration, Instant};
use tokio::runtime::Handle;
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy};
use winit::window::{Window, WindowId};

pub struct ViewerAppBuilder {
    event_loop: EventLoop<TwilightClientEvent>,
    on_exit: Option<Box<dyn FnOnce() -> Result<()>>>,
    _guard: NonSend,
}

pub struct ViewerApp {
    window: Option<Rc<Window>>,
    on_exit: Option<Box<dyn FnOnce() -> Result<()>>>,
    display_state: Option<DisplayState>,
    desktop_view: Option<DesktopView>,
    last_log_print: Instant,
    frames_since_last_log: i32,
    _guard: NonSend,
}

impl ViewerApp {
    /// Must be called from main thread
    pub fn build(_rt: Handle) -> ViewerAppBuilder {
        //TODO: Remove rt (parameter) if not needed

        ViewerAppBuilder {
            event_loop: EventLoop::<TwilightClientEvent>::with_user_event()
                .build()
                .unwrap(),
            on_exit: None,
            _guard: Default::default(),
        }
    }
}

impl ViewerAppBuilder {
    pub fn set_on_exit(&mut self, callback: Box<dyn FnOnce() -> Result<()>>) {
        self.on_exit = Some(callback);
    }

    pub fn create_proxy(&self) -> EventLoopProxy<TwilightClientEvent> {
        self.event_loop.create_proxy()
    }

    pub fn launch(self) -> ! {
        let mut app = Box::new(ViewerApp {
            window: None,
            on_exit: self.on_exit,
            display_state: None,
            desktop_view: None,
            last_log_print: Instant::now(),
            frames_since_last_log: 0,
            _guard: Default::default(),
        });

        self.event_loop.run_app(&mut app).unwrap();

        log::info!("Clean exit after event loop termination.");
        std::mem::drop(app);
        let _ = std::io::stderr().flush();
        let _ = std::io::stdout().flush();
        std::process::exit(0);
    }
}

impl ApplicationHandler<TwilightClientEvent> for ViewerApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        event_loop.set_control_flow(ControlFlow::Wait);

        if self.window.is_none() {
            // create a new window
            let window = Rc::new(
                event_loop
                    .create_window(Window::default_attributes())
                    .unwrap(),
            );
            self.window = Some(window);
        }

        let window = Rc::clone(&self.window.as_ref().expect("assigned just above"));

        //FIXME: web needs real await
        self.display_state = Some(pollster::block_on(DisplayState::new(window)).unwrap());
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        let window = self.window.as_mut().expect("guaranteed by winit");
        assert_eq!(window.id(), window_id, "this app creates only 1 window");

        let state = self.display_state.as_mut().unwrap();

        //FIXME: Do I need to render here?

        match event {
            WindowEvent::CloseRequested => {
                event_loop.exit();
            }
            WindowEvent::Resized(physical_size) => {
                state.resize(physical_size);
                window.request_redraw();
            }
            WindowEvent::ScaleFactorChanged { .. } => {
                window.request_redraw();
            }
            WindowEvent::RedrawRequested => {
                let state = self.display_state.as_mut().unwrap();

                let elapsed = Instant::now() - self.last_log_print;
                if elapsed > Duration::from_secs(10) {
                    let fps = self.frames_since_last_log as f64 / elapsed.as_secs_f64();
                    info!("Render FPS={fps:.2}");
                    self.last_log_print = Instant::now();
                    self.frames_since_last_log = 0;
                }
                self.frames_since_last_log += 1;

                let render_result = match self.desktop_view.as_mut() {
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

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: TwilightClientEvent) {
        match event {
            TwilightClientEvent::Connected(info) => {
                info!("Connected to {info:?}");

                let width = info.resolution.width;
                let height = info.resolution.height;

                let state = self.display_state.as_mut().unwrap();
                self.desktop_view = Some(DesktopView::new(state, width, height));
            }
            TwilightClientEvent::NextFrame(update) => {
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                    self.desktop_view
                        .as_mut()
                        .expect("resolution not set before render")
                        .update(update);
                }
            }
            TwilightClientEvent::Closed(r) => {
                if let Err(e) = r {
                    log::error!("Exiting event loop due to error:\n{e:?}");
                }
                event_loop.exit();
            }
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(window) = self.window.as_ref() {
            window.request_redraw();
        }
    }

    fn exiting(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(f) = self.on_exit.take() {
            if let Err(e) = f() {
                error!("on_exit callback returned an error:\n{e:?}");
            }
        }
    }
}
