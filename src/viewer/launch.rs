use crate::image::{ColorFormat, Image};
use crate::viewer::display_state::DisplayState;
use anyhow::Result;
use std::future::Future;
use std::net::Ipv4Addr;
use tokio::io::AsyncReadExt;
use winit::event;
use winit::event::{Event, WindowEvent};
use winit::event_loop::{ControlFlow, EventLoop};
use winit::window::WindowBuilder;

fn eval_local_future<F: Future>(future: F) -> F::Output {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let localset = tokio::task::LocalSet::new();
    localset.block_on(&runtime, future)
}

async fn receiver(tx: tokio::sync::mpsc::Sender<Image>) -> Result<u64> {
    let mut frames = 0;
    let mut stream = tokio::net::TcpStream::connect((Ipv4Addr::new(127, 0, 0, 1), 6495)).await?;
    println!("Connected to {}", stream.peer_addr().unwrap());

    stream.set_nodelay(true)?;
    let w = stream.read_u32_le().await?;
    let h = stream.read_u32_le().await?;
    println!("Receiving {}x{} image", w, h);

    loop {
        let mut img = Image::new(w, h, ColorFormat::Bgra8888);
        stream.read_exact(&mut img.data).await?;
        if tx.send(img).await.is_err() {
            break;
        }
        frames += 1;
    }

    Ok(frames)
}

// must be called from main thread (because EventLoop requires to do so)
pub fn launch() -> ! {
    let event_loop = EventLoop::new();
    let window = WindowBuilder::new().build(&event_loop).unwrap();
    let mut state_box = None;

    let rt = tokio::runtime::Runtime::new().unwrap();
    let (tx, rx) = tokio::sync::mpsc::channel(1);
    let mut tx = Some(tx);
    let mut rx = Some(rx);
    let mut receiver_box = None;

    event_loop.run(move |event, _, control_flow| match event {
        Event::NewEvents(event::StartCause::Init) => {
            let _guard = rt.enter();
            let tx = tx.take().unwrap();
            receiver_box = Some(tokio::spawn(receiver(tx)));
        }
        Event::Resumed => {
            // Initialize graphic state
            //TODO: What to do when state_box is not none? (relevant on mobile platforms)
            if state_box.is_none() {
                state_box = Some(eval_local_future(DisplayState::new(&window)).unwrap());
            }
        }
        Event::MainEventsCleared => {
            //TODO: Is it better to redraw on requests?
            // See https://docs.rs/winit/latest/winit/event_loop/enum.ControlFlow.html#variant.WaitUntil
            let rx = rx.as_mut().unwrap();
            let img = rx.blocking_recv().unwrap();
            let state = state_box.as_mut().unwrap();
            state.update(img);
            match state.render() {
                Ok(_) => {}
                Err(wgpu::SurfaceError::Lost) => state.reconfigure_surface(),
                Err(wgpu::SurfaceError::OutOfMemory) => {
                    eprintln!("{:?}", wgpu::SurfaceError::OutOfMemory);
                    *control_flow = ControlFlow::Exit;
                }
                Err(e) => eprintln!("{:?}", e),
            }
        }
        Event::WindowEvent {
            ref event,
            window_id,
        } if window_id == window.id() => {
            let state = state_box.as_mut().unwrap();
            if !state.input(event) {
                match event {
                    WindowEvent::CloseRequested => {
                        *control_flow = ControlFlow::Exit;
                    }
                    WindowEvent::Resized(physical_size) => {
                        state.resize(*physical_size);
                    }
                    WindowEvent::ScaleFactorChanged { new_inner_size, .. } => {
                        state.resize(**new_inner_size);
                    }
                    _ => {}
                }
            }
        }
        Event::LoopDestroyed => {
            let mut rx = rx.take().unwrap();
            rx.close();
            if let Some(x) = receiver_box.take() {
                println!("{:?}", rt.block_on(x).unwrap());
            }
        }
        _ => {}
    });
}
