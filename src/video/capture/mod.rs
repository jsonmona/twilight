mod capture_dxgi;
//mod capture_gdi;
mod factory_win32;
mod info;
mod stage;

pub use capture_dxgi::CaptureDxgi;
//pub use capture_gdi::GdiCaptureStage;
pub use factory_win32::*;
pub use info::*;
pub use stage::CaptureStage;
