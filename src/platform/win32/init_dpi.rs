use windows::Win32::UI::HiDpi::*;

pub fn init_dpi() {
    unsafe {
        SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }
}
