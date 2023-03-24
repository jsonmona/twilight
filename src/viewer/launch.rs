use crate::client::native_server_connection::NativeServerConnection;
use crate::client::{TwilightClient, TwilightClientEvent};

use crate::util::{DesktopUpdate, NonSend};
use crate::viewer::desktop_view::DesktopView;
use crate::viewer::display_state::DisplayState;
use cfg_if::cfg_if;
use std::net::{IpAddr, Ipv4Addr};
use std::str::FromStr;
use std::time::{Duration, Instant};
use tokio::io::DuplexStream;
use winit::event;
use winit::event::{Event, WindowEvent};
use winit::event_loop::{ControlFlow, EventLoopBuilder};
use winit::window::WindowBuilder;

/// must be called from main thread
pub async fn launch(main_thread: NonSend) -> ! {
    launch_inner(None, main_thread).await
}

/// must be called from main thread
pub async fn launch_debug(stream: DuplexStream, main_thread: NonSend) -> ! {
    launch_inner(Some(stream), main_thread).await
}

/// must be called from main thread (because EventLoop requires to do so)
async fn launch_inner(debug_stream: Option<DuplexStream>, _main_thread: NonSend) -> ! {
    let event_loop = EventLoopBuilder::<TwilightClientEvent>::with_user_event().build();
    let window = WindowBuilder::new().build(&event_loop).unwrap();

    let mut display_state = None;
    cfg_if! {
        if #[cfg(target_family = "wasm")] {
            // wasm needs real await
            state_box = Some(DisplayState::new(&window).await.unwrap());
        }
    }

    let mut desktop_view: Option<DesktopView> = None;

    let proxy = event_loop.create_proxy();
    let proxy2 = event_loop.create_proxy();
    let callback = move |e: TwilightClientEvent| {
        proxy.send_event(e).unwrap();
    };
    let callback2 = move |e: TwilightClientEvent| {
        proxy2.send_event(e).unwrap();
    };

    let _client = match debug_stream {
        Some(_) => {
            todo!();
        }
        None => {
            let addr = IpAddr::V4(Ipv4Addr::from_str("127.0.0.1").expect("valid ipv4 address"));
            TwilightClient::new(Box::new(callback), Box::new(callback2), async move {
                NativeServerConnection::new(addr, 6497).await
            })
        }
    };

    let mut old_time = Instant::now();
    let mut frames = 0u32;

    event_loop.run(move |event, _, control_flow| match event {
        Event::NewEvents(event::StartCause::Init) => {
            *control_flow = ControlFlow::Wait;
        }
        Event::Resumed => {
            // Initialize graphic state here
            //TODO: What to do when display_state is not none? (relevant on mobile platforms)
            if display_state.is_none() {
                display_state = Some(pollster::block_on(DisplayState::new(&window)).unwrap());
            }
        }
        Event::MainEventsCleared => {}
        Event::UserEvent(kind) => match kind {
            TwilightClientEvent::Connected { width, height } => {
                println!("Connected to {width}x{height}");
            }
            TwilightClientEvent::NextFrame(update) => {
                let update = DesktopUpdate {
                    cursor: update.cursor,
                    desktop: update.desktop.copied(),
                };

                window.request_redraw();
                let state = display_state.as_mut().unwrap();
                match desktop_view.as_mut() {
                    Some(view) => {
                        view.update(update);
                    }
                    None => {
                        desktop_view = Some(DesktopView::new(state, update));
                    }
                }
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
                println!("Render FPS={fps:.2}");
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
                Err(wgpu::SurfaceError::OutOfMemory) => {
                    eprintln!("{:?}", wgpu::SurfaceError::OutOfMemory);
                    *control_flow = ControlFlow::ExitWithCode(1);
                }
                Err(e) => eprintln!("{e:?}"),
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
