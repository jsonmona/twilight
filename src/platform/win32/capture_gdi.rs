use std::ffi::c_void;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::UI::WindowsAndMessaging::*;

pub fn capture_gdi() {
    let content = unsafe {
        // SAFETY: Call to FFI functions according to their specification

        let hdc = GetDC(HWND(0));
        let memdc = CreateCompatibleDC(hdc);
        assert!(!memdc.is_invalid());

        let width = GetSystemMetrics(SM_CXVIRTUALSCREEN);
        let height = GetSystemMetrics(SM_CYVIRTUALSCREEN);

        let bitmap = CreateCompatibleBitmap(hdc, width, height);

        SelectObject(memdc, bitmap);

        BitBlt(memdc, 0, 0, width, height, hdc, 0, 0, SRCCOPY);

        let mut bitmapinfo = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width,
                biHeight: height,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB,
                biSizeImage: 0,
                biXPelsPerMeter: 0,
                biYPelsPerMeter: 0,
                biClrUsed: 0,
                biClrImportant: 0,
            },
            bmiColors: std::mem::zeroed(),
        };
        let mut image_content = vec![0u8; (width * height * 4) as usize];
        let copied_lines = GetDIBits(
            memdc,
            bitmap,
            0,
            height as u32,
            image_content.as_mut_ptr() as *mut c_void,
            &mut bitmapinfo,
            DIB_RGB_COLORS,
        );
        assert!(copied_lines >= 0);
        assert!(copied_lines == height);

        println!("captured {}x{}", width, height);

        ReleaseDC(HWND(0), hdc);

        DeleteObject(bitmap);
        DeleteDC(memdc);

        image_content
    };

    std::fs::write("image.rgb", &content).unwrap();
}
