mod capture_dxgi;
mod capture_gdi;
mod stage;

pub use capture_dxgi::DxgiCaptureStage;
pub use capture_gdi::GdiCaptureStage;
pub use stage::CaptureStage;
