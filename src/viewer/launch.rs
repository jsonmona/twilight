use crate::image::{ColorFormat, ImageBuf};
use crate::network::util::recv_msg;
use crate::schema::video::{NotifyVideoStart, VideoFrame};
use crate::viewer::display_state::DisplayState;
use anyhow::Result;
use cfg_if::cfg_if;
use std::net::Ipv4Addr;
use tokio::io::AsyncReadExt;
use winit::event;
use winit::event::{Event, WindowEvent};
use winit::event_loop::{ControlFlow, EventLoop};
use winit::window::WindowBuilder;

async fn receiver(tx: tokio::sync::mpsc::Sender<ImageBuf>) -> Result<u64> {
    let mut buffer = vec![0u8; 2 * 1024 * 1024];

    let mut frames = 0;
    let mut stream = tokio::net::TcpStream::connect((Ipv4Addr::new(127, 0, 0, 1), 6495)).await?;
    println!("Connected to {}", stream.peer_addr().unwrap());

    stream.set_nodelay(true)?;

    let msg: NotifyVideoStart = recv_msg(&mut buffer, &mut stream).await?;
    let w = msg.resolution().map(|x| x.width()).unwrap_or_default();
    let h = msg.resolution().map(|x| x.height()).unwrap_or_default();
    let format =
        ColorFormat::from_video_codec(msg.desktop_codec()).expect("requires uncompressed format");
    println!("Receiving {w}x{h} image");

    loop {
        let mut img = ImageBuf::alloc(w, h, None, format);

        let frame: VideoFrame = recv_msg(&mut buffer, &mut stream).await?;
        assert_eq!(frame.video_bytes(), img.data.len() as u64);

        stream.read_exact(&mut img.data).await?;

        if tx.send(img).await.is_err() {
            break;
        }
        frames += 1;
    }

    Ok(frames)
}

// must be called from main thread (because EventLoop requires to do so)
pub async fn launch() -> ! {
    let event_loop = EventLoop::new();
    let window = WindowBuilder::new().build(&event_loop).unwrap();

    let mut display_state = None;
    cfg_if! {
        if #[cfg(target_family = "wasm")] {
            // wasm needs real await
            state_box = Some(DisplayState::new(&window).await.unwrap());
        }
    }

    let (tx, rx) = tokio::sync::mpsc::channel(1);
    let mut tx = Some(tx);
    let mut rx = Some(rx);
    let mut receiver_box = None;

    event_loop.run(move |event, _, control_flow| match event {
        Event::NewEvents(event::StartCause::Init) => {
            let tx = tx.take().unwrap();
            receiver_box = Some(tokio::spawn(receiver(tx)));
        }
        Event::Resumed => {
            // Initialize graphic state here
            //TODO: What to do when state_box is not none? (relevant on mobile platforms)
            if display_state.is_none() {
                display_state = Some(pollster::block_on(DisplayState::new(&window)).unwrap());
            }
        }
        Event::MainEventsCleared => {
            *control_flow = ControlFlow::Poll;

            let rx = rx.as_mut().unwrap();
            if let Ok(img) = rx.try_recv() {
                let state = display_state.as_mut().unwrap();
                state.update(img);
                window.request_redraw();
            }
        }
        Event::RedrawRequested(_) => {
            let state = display_state.as_mut().unwrap();
            match state.render() {
                Ok(_) => {}
                Err(wgpu::SurfaceError::Lost) => state.reconfigure_surface(),
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
            drop(receiver_box.take());
        }
        _ => {}
    });
}
