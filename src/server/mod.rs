mod capture_pipeline;
mod serve;
mod session_id;
mod twilight_server;
mod win32_capture_pipeline;

pub use capture_pipeline::CapturePipeline;
pub use serve::{serve, serve_debug};
pub use twilight_server::TwilightServer;

pub fn new_capture_pipeline() -> anyhow::Result<Box<dyn CapturePipeline>> {
    Ok(Box::new(
        win32_capture_pipeline::Win32CapturePipeline::new()?
    ))
}
