use serde::{Deserialize, Serialize};

/// Server config common to all platforms
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ServerConfig {
    pub desktop_capture_method: Option<DesktopCaptureMethod>,
    pub windows: Win32ServerConfig,
}

/// Method to capture the desktop
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[non_exhaustive]
pub enum DesktopCaptureMethod {
    // windows
    Dxgi,
    Gdi,
}

/// Server confg specific to Windows
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Win32ServerConfig {}

/// "Normal" default values for current platform.
pub fn normal_defaults() -> ServerConfig {
    ServerConfig {
        desktop_capture_method: Some(DesktopCaptureMethod::Dxgi),
        windows: Win32ServerConfig {},
    }
}

/// "Fallback" default values for current platform.
pub fn fallback_defaults() -> ServerConfig {
    ServerConfig {
        desktop_capture_method: Some(DesktopCaptureMethod::Gdi),
        windows: Win32ServerConfig {},
    }
}
