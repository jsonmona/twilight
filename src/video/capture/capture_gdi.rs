use crate::image::{ColorFormat, Image, ImageBuf};
use crate::network::dto::video::{RefreshRate, Resolution};
use crate::util::{CursorShape, CursorState, DesktopUpdate, Timings};
use crate::video::capture::CaptureStage;
use crate::video::encoder::EncoderStage;
use anyhow::{anyhow, bail, ensure, Result};
use std::ffi::c_void;
use std::mem::{size_of, zeroed};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock, RwLock};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::UI::WindowsAndMessaging::*;

// https://github.com/obsproject/obs-studio/blob/6fb83abaeb711d1e12054d2ef539da5c43237c58/plugins/win-capture/dc-capture.c#L38

#[derive(Debug)]
pub struct CaptureGdi {
    dev_id: Vec<u16>,
    shutdown: AtomicBool,
    next_stage: OnceLock<Arc<dyn EncoderStage>>,
    worker: RwLock<Option<JoinHandle<Result<()>>>>,
    resolution: OnceLock<Resolution>,
}

impl CaptureGdi {
    /// dev_id: The slice from WCHAR szDevice[16]. Must end with a NULL character.
    pub fn new(dev_id: Vec<u16>) -> Result<Arc<CaptureGdi>> {
        assert_eq!(
            dev_id.last(),
            Some(&0),
            "dev_id must end with a NULL character"
        );

        Ok(Arc::new(CaptureGdi {
            dev_id,
            shutdown: AtomicBool::new(false),
            next_stage: Default::default(),
            worker: Default::default(),
            resolution: Default::default(),
        }))
    }
}

impl CaptureStage for CaptureGdi {
    fn configured(&self) -> bool {
        self.resolution.get().is_some()
    }

    fn resolution(&self) -> Result<Resolution> {
        self.resolution
            .get()
            .cloned()
            .ok_or_else(|| anyhow!("CaptureGdi not initialized yet"))
    }

    fn refresh_rate(&self) -> Result<RefreshRate> {
        //FIXME: Report actual refresh rate (if possible at all)
        log::warn!("STUB: reporting refresh rate of 1Hz");
        Ok(RefreshRate { num: 1, den: 1 })
    }

    fn set_next_stage(&self, encoder: Arc<dyn EncoderStage>) -> Result<()> {
        self.next_stage
            .set(encoder)
            .map_err(|_| anyhow!("next_stage already set"))
    }

    fn configure(self: Arc<Self>) -> Result<()> {
        let this = Arc::clone(&self);
        let next_stage = Arc::clone(
            self.next_stage
                .get()
                .ok_or_else(|| anyhow!("next_stage not set"))?,
        );

        *self.worker.write().unwrap() = Some(std::thread::spawn(move || {
            let mut resources = init_resources(&this)?;
            while !this.shutdown.load(Ordering::Acquire) {
                let update = capture_once(&mut resources)?;
                next_stage.push(update);
            }
            Ok(())
        }));

        //TODO: Convert into a proper event based one
        //      And what the hell is going on with all that unwraps?
        while self.resolution.get().is_none() {
            if self.worker.read().unwrap().as_ref().unwrap().is_finished() {
                self.worker
                    .write()
                    .unwrap()
                    .take()
                    .unwrap()
                    .join()
                    .unwrap()?;
                bail!("worker has silently finished without configuring");
            }
            std::thread::sleep(Duration::from_millis(1));
        }

        Ok(())
    }

    fn shutdown(&self) {
        self.shutdown.store(true, Ordering::Release);
    }
}

fn init_resources(this: &CaptureGdi) -> Result<Resources> {
    //TODO: Use dev_id to decide which monitor to capture

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

        this.resolution
            .set(Resolution { width, height })
            .map_err(|_| anyhow!("resolution already set"))?;

        // negate height to produce top-bottom bitmap
        let bitmapinfo = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width as i32,
                biHeight: -(height as i32),
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
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

        Ok(Resources {
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

struct Resources {
    hdc: HDC,
    memdc: HDC,
    width: u32,
    height: u32,
    bitmap: HBITMAP,
    bitmap_data: *mut u8,
    old_bitmap: HGDIOBJ,
    last_cursor: HCURSOR,
}

impl Drop for Resources {
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

fn capture_once(res: &mut Resources) -> Result<DesktopUpdate<ImageBuf>> {
    let cursor_state;

    let slice = unsafe {
        BitBlt(
            res.memdc,
            0,
            0,
            res.width as i32,
            res.height as i32,
            res.hdc,
            0,
            0,
            SRCCOPY,
        )?;

        // Similar code in OBS: https://github.com/obsproject/obs-studio/blob/2ff210acfdf9f72ee6c845c9eacceae1886c275f/plugins/win-capture/cursor-capture.c#L201
        let mut cursor_info: CURSORINFO = zeroed();
        cursor_info.cbSize = size_of::<CURSORINFO>() as u32;
        let ret = GetCursorInfo(&mut cursor_info);

        cursor_state = if ret.is_ok() {
            // only when GetCursorInfo succeeded

            let shape = if res.last_cursor != cursor_info.hCursor {
                res.last_cursor = cursor_info.hCursor;

                let mut iconinfo = zeroed();
                let ret = GetIconInfo(cursor_info.hCursor, &mut iconinfo);
                if ret.is_ok() {
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

        let bytes = res.width as usize * res.height as usize * 4;
        std::slice::from_raw_parts(res.bitmap_data, bytes)
    };

    let mut timings = Timings::new();
    timings.capture = Instant::now().into();

    Ok(DesktopUpdate {
        cursor: cursor_state,
        timings,
        desktop: Image {
            color_format: ColorFormat::Bgra8888,
            width: res.width,
            height: res.height,
            stride: res.width * 4,
            data: slice,
        }
        .copied(),
    })
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
