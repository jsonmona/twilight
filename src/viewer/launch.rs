use crate::viewer::display_state::DisplayState;
use winit::event::{Event, WindowEvent};
use winit::event_loop::{ControlFlow, EventLoop};
use winit::window::WindowBuilder;

// must be called from main thread (because EventLoop requires to do so)
pub fn launch() -> ! {
    let event_loop = EventLoop::new();
    let window = WindowBuilder::new().build(&event_loop).unwrap();
    let mut state_box = None;

    event_loop.run(move |event, _, control_flow| match event {
        Event::Resumed => {
            // Initialize graphic state
            //TODO: What to do when state_box is not none? (relevant on mobile platforms)
            if state_box.is_none() {
                state_box = Some(pollster::block_on(DisplayState::new(&window)).unwrap());
            }
        }
        Event::MainEventsCleared => {
            //TODO: Is it better to redraw on requests?
            // See https://docs.rs/winit/latest/winit/event_loop/enum.ControlFlow.html#variant.WaitUntil
            let state = state_box.as_mut().unwrap();
            state.update();
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
        _ => {}
    });
}
