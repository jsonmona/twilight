fn main() {
    twilight::platform::win32::init_dpi();
    let mut capture = twilight::platform::win32::capture_gdi::CaptureGdi::new().unwrap();
}
