use std::future::Future;
use std::marker::PhantomData;
use crate::image::{ColorFormat, Image, ImageBuf};
use crate::network::util::recv_msg;
use crate::schema::video::{NotifyVideoStart, VideoFrame};
use crate::util::{CursorShape, CursorState, DesktopUpdate};
use crate::viewer::display_state::DisplayState;
use anyhow::Result;
use cfg_if::cfg_if;
use std::net::{IpAddr, Ipv4Addr};
use std::str::FromStr;
use std::time::{Duration, Instant};
use tokio::io::AsyncReadExt;
use winit::event;
use winit::event::{Event, WindowEvent};
use winit::event_loop::{ControlFlow, EventLoop, EventLoopBuilder};
use winit::window::WindowBuilder;
use crate::viewer::client::Client;
use crate::viewer::desktop_view::DesktopView;

#[derive(Default)]
struct NonSend(PhantomData<*const usize>);

// must be called from main thread
pub async fn launch() -> ! {
    let _guard: NonSend = Default::default();

    launch_inner().await
}

enum MyEvent {
    NextUpdate(DesktopUpdate<ImageBuf>),
    Quit,
}

// must be called from main thread (because EventLoop requires to do so)
async fn launch_inner() -> ! {
    let event_loop = EventLoopBuilder::<MyEvent>::with_user_event().build();
    let window = WindowBuilder::new().build(&event_loop).unwrap();

    let proxy = event_loop.create_proxy();

    let mut display_state = None;
    cfg_if! {
        if #[cfg(target_family = "wasm")] {
            // wasm needs real await
            state_box = Some(DisplayState::new(&window).await.unwrap());
        }
    }

    let mut desktop_view: Option<DesktopView> = None;

    let addr = IpAddr::V4(Ipv4Addr::from_str("127.0.0.1").expect("valid ipv4 address"));
    let mut client = Client::connect_to(addr, None).await.expect("connection error");

    tokio::task::spawn(async move {
        while client.is_running() {
            match client.async_recv().await {
                Some(update) => {
                    if proxy.send_event(MyEvent::NextUpdate(update)).is_err() {
                        client.signal_quit();
                    }
                },
                None => {
                    let _ = proxy.send_event(MyEvent::Quit);
                    client.signal_quit();
                },
            }
        }
        client.join().await
    });

    let mut old_time = Instant::now();
    let mut frames = 0u32;

    event_loop.run(move |event, _, control_flow| match event {
        Event::NewEvents(event::StartCause::Init) => {
        }
        Event::Resumed => {
            // Initialize graphic state here
            //TODO: What to do when display_state is not none? (relevant on mobile platforms)
            if display_state.is_none() {
                display_state = Some(pollster::block_on(DisplayState::new(&window)).unwrap());
            }
        }
        Event::MainEventsCleared => {
            *control_flow = ControlFlow::Wait;
            window.request_redraw();
        }
        Event::UserEvent(kind) => {
            match kind {
                MyEvent::NextUpdate(update) => {
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
                MyEvent::Quit => {
                    *control_flow = ControlFlow::Exit;
                }
            }
        }
        Event::RedrawRequested(_) => {
            *control_flow = ControlFlow::Wait;

            let state = display_state.as_mut().unwrap();
            let view = desktop_view.as_mut().unwrap();

            let elapsed = Instant::now() - old_time;
            if elapsed > Duration::from_secs(10) {
                let fps = frames as f64 / elapsed.as_secs_f64();
                println!("Render FPS={fps:.2}");
                old_time = Instant::now();
                frames = 0;
            }
            frames += 1;

            match view.render(state) {
                Ok(_) => {}
                Err(wgpu::SurfaceError::Lost) => {
                    state.reconfigure_surface();
                    window.request_redraw();
                },
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
        Event::LoopDestroyed => {
        }
        _ => {}
    })
}
