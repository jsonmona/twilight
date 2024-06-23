mod capture_dxgi;
mod capture_gdi;
mod factory_win32;
mod stage;

pub use capture_dxgi::CaptureDxgi;
pub use capture_gdi::CaptureGdi;
pub use factory_win32::*;
pub use stage::CaptureStage;
