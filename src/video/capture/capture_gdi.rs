use crate::image::{ColorFormat, Image, ImageBuf};
use crate::util::{CursorShape, CursorState, DesktopUpdate};
use crate::video::capture::CaptureStage;
use anyhow::{ensure, Result};
use std::ffi::c_void;
use std::mem::{size_of, zeroed};
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::UI::WindowsAndMessaging::*;

// https://github.com/obsproject/obs-studio/blob/6fb83abaeb711d1e12054d2ef539da5c43237c58/plugins/win-capture/dc-capture.c#L38

#[derive(Debug)]
pub struct GdiCaptureStage {
    is_open: bool,

    hdc: HDC,
    memdc: CreatedHDC,
    width: u32,
    height: u32,
    bitmap: HBITMAP,
    bitmap_data: *mut u8,
    old_bitmap: HGDIOBJ,
    last_cursor: HCURSOR,
}

impl GdiCaptureStage {
    pub fn new() -> Result<GdiCaptureStage> {
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

            Ok(GdiCaptureStage {
                is_open: true,
                hdc,
                memdc,
                width,
                height,
                bitmap,
                bitmap_data: bitmap_data as *mut u8,
                old_bitmap,
                last_cursor: HCURSOR(0),
            })
        }
    }
}

impl CaptureStage for GdiCaptureStage {
    fn resolution(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    fn next(&mut self) -> Result<DesktopUpdate<Image<&[u8]>>> {
        assert!(self.is_open, "tried to capture from closed CaptureGdi");

        let cursor_state;

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

            // Similar code in OBS: https://github.com/obsproject/obs-studio/blob/2ff210acfdf9f72ee6c845c9eacceae1886c275f/plugins/win-capture/cursor-capture.c#L201
            let mut cursor_info: CURSORINFO = zeroed();
            cursor_info.cbSize = size_of::<CURSORINFO>() as u32;
            let ret = GetCursorInfo(&mut cursor_info);

            cursor_state = if ret.as_bool() {
                // only when GetCursorInfo succeeded

                let shape = if self.last_cursor != cursor_info.hCursor {
                    self.last_cursor = cursor_info.hCursor;

                    let mut iconinfo = zeroed();
                    let ret = GetIconInfo(cursor_info.hCursor, &mut iconinfo);
                    if ret.as_bool() {
                        let mut xor = false;
                        let cursor = get_cursor_color(&iconinfo, &mut xor)
                            .or_else(|| get_cursor_monochrome(&iconinfo, &mut xor));

                        DeleteObject(iconinfo.hbmMask);
                        DeleteObject(iconinfo.hbmColor);

                        cursor.map(|image| CursorShape {
                            image,
                            xor,
                            hotspot_x: 0.0,
                            hotspot_y: 0.0,
                        })
                    } else {
                        None
                    }
                } else {
                    None
                };

                Some(CursorState {
                    visible: (cursor_info.flags.0 & CURSOR_SHOWING.0) != 0,
                    pos_x: cursor_info.ptScreenPos.x as u32,
                    pos_y: cursor_info.ptScreenPos.y as u32,
                    shape,
                })
            } else {
                None
            };

            let ret = GdiFlush();
            ensure!(ret.as_bool(), "failed to flush gdi");

            let bytes = self.width as usize * self.height as usize * 4;
            std::slice::from_raw_parts(self.bitmap_data, bytes)
        };

        Ok(DesktopUpdate {
            cursor: cursor_state,
            desktop: Image {
                color_format: ColorFormat::Bgra8888,
                width: self.width,
                height: self.height,
                stride: self.width * 4,
                data: slice,
            },
        })
    }

    fn close(&mut self) {
        self.is_open = false;
    }
}

impl Drop for GdiCaptureStage {
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

unsafe fn get_bitmap_data(hbmp: HBITMAP) -> Option<(BITMAP, Vec<u8>)> {
    let mut bmp: BITMAP = zeroed();
    let ret = GetObjectW(
        hbmp,
        size_of::<BITMAP>() as i32,
        Some(&mut bmp as *mut BITMAP as *mut c_void),
    );
    if ret != 0 {
        let size = bmp.bmHeight * bmp.bmWidthBytes;
        let mut buf = vec![0u8; size as usize];

        let ret = GetBitmapBits(hbmp, size, buf.as_mut_ptr() as *mut c_void);

        if ret != 0 {
            Some((bmp, buf))
        } else {
            None
        }
    } else {
        None
    }
}

unsafe fn get_cursor_color(iconinfo: &ICONINFO, xor: &mut bool) -> Option<ImageBuf> {
    let (bmp_color, mut color) = get_bitmap_data(iconinfo.hbmColor)?;

    if bmp_color.bmBitsPixel < 32 {
        return None;
    }

    if let Some((bmp_mask, mask)) = get_bitmap_data(iconinfo.hbmMask) {
        let bitmap_has_alpha = color.iter().skip(3).step_by(4).any(|&alpha| alpha != 0);
        //TODO: Find a way to detect masked color icons (DXGI_OUTDUPL_POINTER_SHAPE_TYPE_MASKED_COLOR)

        if !bitmap_has_alpha {
            *xor = true;
            let mut mask_values = mask.iter().copied();

            for y in 0..bmp_mask.bmHeight as usize {
                let mut step: u8 = 8;
                let mut val = mask_values.next().unwrap_or(0);

                for x in 0..bmp_mask.bmWidth as usize {
                    if step == 0 {
                        step = 8;
                        val = mask_values.next().unwrap_or(0);
                    }

                    //TODO: Lift this implicit bound check to outside loop
                    color[(y * bmp_mask.bmWidth as usize + x) * 4 + 3] =
                        if val & 0x80 != 0 { 255 } else { 0 };
                    val <<= 1;
                    step -= 1;
                }
            }
        }
    }

    Some(Image::new(
        bmp_color.bmWidth as u32,
        bmp_color.bmHeight as u32,
        bmp_color.bmWidthBytes as u32,
        ColorFormat::Bgra8888,
        color,
    ))
}

unsafe fn get_cursor_monochrome(iconinfo: &ICONINFO, xor: &mut bool) -> Option<ImageBuf> {
    *xor = true;

    let (bmp, mask) = get_bitmap_data(iconinfo.hbmMask)?;

    let height = bmp.bmHeight.unsigned_abs() / 2;

    let pixels = height * bmp.bmWidth as u32;

    let bottom = (bmp.bmWidthBytes as u32 * height) as usize;

    let and_image = &mask[..bottom];
    let xor_image = &mask[bottom..];

    let mut output = Vec::with_capacity(pixels as usize * 4);

    output.extend(
        and_image
            .iter()
            .copied()
            .zip(xor_image.iter().copied())
            .flat_map(|(mut and_mask, mut xor_mask)| {
                let mut output = [0; 8 * 4];
                for i in 0..8 {
                    let and_value = if and_mask & 0x80 != 0 { 255 } else { 0 };
                    let xor_value = if xor_mask & 0x80 != 0 { 255 } else { 0 };
                    output[i * 4] = and_value;
                    output[i * 4 + 1] = and_value;
                    output[i * 4 + 2] = and_value;
                    output[i * 4 + 3] = xor_value;
                    and_mask <<= 1;
                    xor_mask <<= 1;
                }
                output
            }),
    );

    Some(Image::new(
        bmp.bmWidth as u32,
        height,
        bmp.bmWidth as u32 * 4,
        ColorFormat::Bgra8888,
        output,
    ))
}
