use crate::image::{ColorFormat, Image};
use anyhow::{ensure, Result};
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::UI::WindowsAndMessaging::*;

// https://github.com/obsproject/obs-studio/blob/6fb83abaeb711d1e12054d2ef539da5c43237c58/plugins/win-capture/dc-capture.c#L38

pub struct CaptureGdi {
    is_open: bool,

    hdc: HDC,
    memdc: CreatedHDC,
    width: u32,
    height: u32,
    bitmap: HBITMAP,
    bitmap_data: *mut u8,
    old_bitmap: HGDIOBJ,
}

impl CaptureGdi {
    pub fn new() -> Result<CaptureGdi> {
        // SAFETY: FFI
        unsafe {
            //FIXME: Resource leak on early return

            let hdc = GetDC(HWND(0));
            ensure!(!hdc.is_invalid(), "unable to get desktop DC");

            let memdc = CreateCompatibleDC(hdc);
            ensure!(!memdc.is_invalid(), "unable to create compatible DC");

            let width = GetSystemMetrics(SM_CXSCREEN);
            let height = GetSystemMetrics(SM_CYSCREEN);
            ensure!(width > 0 && height > 0, "unable to query screen size");

            let width = width as u32;
            let height = height as u32;

            // negate height to produce top-bottom bitmap
            let bitmapinfo = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: width as i32,
                    biHeight: -(height as i32),
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

            let mut bitmap_data = std::ptr::null_mut();

            let bitmap = CreateDIBSection(
                hdc,
                &bitmapinfo,
                DIB_RGB_COLORS,
                &mut bitmap_data,
                HANDLE(0),
                0,
            )?;
            ensure!(!bitmap.is_invalid(), "unable to create bitmap");

            let old_bitmap = SelectObject(memdc, bitmap);

            Ok(CaptureGdi {
                is_open: true,
                hdc,
                memdc,
                width,
                height,
                bitmap,
                bitmap_data: bitmap_data as *mut u8,
                old_bitmap,
            })
        }
    }

    pub fn capture(&mut self) -> Result<Image<&[u8]>> {
        assert!(self.is_open, "tried to capture from closed CaptureGdi");

        let slice = unsafe {
            // SAFETY: FFI
            let ret = BitBlt(
                self.memdc,
                0,
                0,
                self.width as i32,
                self.height as i32,
                self.hdc,
                0,
                0,
                SRCCOPY,
            );
            ensure!(ret.as_bool(), "failed to BitBlt");

            let ret = GdiFlush();
            ensure!(ret.as_bool(), "failed to flush gdi");

            let bytes = self.width as usize * self.height as usize * 4;
            std::slice::from_raw_parts(self.bitmap_data, bytes)
        };

        Ok(Image {
            color_format: ColorFormat::Bgra8888,
            width: self.width,
            height: self.height,
            stride: self.width * 4,
            data: slice,
        })
    }

    pub fn resolution(&self) -> (u32, u32) {
        (self.width, self.height)
    }
}

impl Drop for CaptureGdi {
    fn drop(&mut self) {
        // SAFETY: FFI
        unsafe {
            SelectObject(self.memdc, self.old_bitmap);

            ReleaseDC(HWND(0), self.hdc);

            DeleteObject(self.bitmap);
            DeleteDC(self.memdc);
        }
    }
}
