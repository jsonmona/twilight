use crate::image::{ColorFormat, Image, ImageBuf};
use crate::util::{AsUsize, CursorShape, CursorState, DesktopUpdate};
use crate::video::capture::CaptureStage;
use anyhow::{ensure, Context, Result};
use log::{error, info};
use std::ffi::c_void;
use std::mem::zeroed;
use std::ptr::slice_from_raw_parts;
use windows::core::Interface;
use windows::Win32::Graphics::Direct3D::*;
use windows::Win32::Graphics::Direct3D11::*;
use windows::Win32::Graphics::Dxgi::Common::*;
use windows::Win32::Graphics::Dxgi::*;

#[derive(Debug)]
pub struct DxgiCaptureStage {
    _adapter: IDXGIAdapter1,
    _output: IDXGIOutput1,
    _device: ID3D11Device,
    ctx: ID3D11DeviceContext,
    output_duplication: IDXGIOutputDuplication,
    desc: DXGI_OUTDUPL_DESC,
    staging_tex: ID3D11Texture2D,
    curr_img: Option<ImageBuf>,
}

impl DxgiCaptureStage {
    pub fn new() -> Result<Self> {
        let adapter;
        let output;
        let device;
        let ctx;
        let output_duplication;
        let desc;
        let staging_tex;

        unsafe {
            let factory: IDXGIFactory1 = CreateDXGIFactory1().context("DXGI 1.1 not supported")?;

            let adapters = list_adapters(&factory)?;
            ensure!(!adapters.is_empty(), "No D3D adapter found");

            // Select primary adapter
            adapter = adapters
                .into_iter()
                .next()
                .expect("No primary adapter found");

            let mut adapter_desc = zeroed();
            adapter.GetDesc1(&mut adapter_desc)?;

            let adapter_name = String::from_utf16_lossy(trim_null(&adapter_desc.Description));
            info!("Selected adapter {adapter_name}");

            let primary_output = list_outputs(&adapter)?
                .into_iter()
                .next()
                .context("No primary output found")?;

            output = primary_output
                .cast::<IDXGIOutput1>()
                .context("Output does not support DXGI 1.1")?;

            let mut output_desc = zeroed();
            output.GetDesc(&mut output_desc)?;

            let output_name = String::from_utf16_lossy(trim_null(&output_desc.DeviceName));
            info!("Selected output {output_name}");

            let mut flags = D3D11_CREATE_DEVICE_SINGLETHREADED
                | D3D11_CREATE_DEVICE_BGRA_SUPPORT;

            if option_env!("TWILIGHT_DEBUG_D3D") == Some("1") {
                flags |= D3D11_CREATE_DEVICE_DEBUG;
            }

            let feature_levels = [D3D_FEATURE_LEVEL_10_0, D3D_FEATURE_LEVEL_9_1];
            let mut selected_feature_level = zeroed();
            let mut p_ctx = zeroed();
            let mut p_device = None;
            D3D11CreateDevice(
                &adapter,
                D3D_DRIVER_TYPE_UNKNOWN,
                None,
                flags,
                Some(&feature_levels),
                D3D11_SDK_VERSION,
                Some(&mut p_device),
                Some(&mut selected_feature_level),
                Some(&mut p_ctx),
            )?;

            device = p_device.expect("A successful D3D11CreateDevice must return a device");
            ctx = p_ctx.expect("A successful D3D11CreateDevice must return an immediate context");

            output_duplication = output.DuplicateOutput(&device)?;
            let mut outdupl_desc = zeroed();
            output_duplication.GetDesc(&mut outdupl_desc);
            desc = outdupl_desc;

            //TODO: Handle desktop rotation
            let mut p_staging_tex = None;
            device.CreateTexture2D(
                &D3D11_TEXTURE2D_DESC {
                    Width: desc.ModeDesc.Width,
                    Height: desc.ModeDesc.Height,
                    MipLevels: 1,
                    ArraySize: 1,
                    Format: DXGI_FORMAT_B8G8R8A8_UNORM,
                    SampleDesc: DXGI_SAMPLE_DESC {
                        Count: 1,
                        Quality: 0,
                    },
                    Usage: D3D11_USAGE_STAGING,
                    BindFlags: D3D11_BIND_FLAG(0),
                    CPUAccessFlags: D3D11_CPU_ACCESS_READ,
                    MiscFlags: D3D11_RESOURCE_MISC_FLAG(0),
                },
                None,
                Some(&mut p_staging_tex),
            )?;
            staging_tex = p_staging_tex
                .expect("A successful CreateTexture2D call must return a valid texture");
        }

        //TODO: Implement optimization using DesktopImageInSystemMemory. I'm yet to encounter
        //      any system with that flag true. Please create a GitHub issue if you have one.

        Ok(DxgiCaptureStage {
            _adapter: adapter,
            _output: output,
            _device: device,
            ctx,
            output_duplication,
            desc,
            staging_tex,
            curr_img: None,
        })
    }

    unsafe fn copy_desktop_tex(&mut self, tex: &ID3D11Texture2D) -> Result<()> {
        let mut src_desc = zeroed();
        tex.GetDesc(&mut src_desc);
        assert!(
            src_desc.Width == self.width() && src_desc.Height == self.height(),
            "Resolution must not change"
        );
        assert_eq!(
            src_desc.Format, DXGI_FORMAT_B8G8R8A8_UNORM,
            "Only B8G8R8A8 is supported"
        );

        self.ctx
            .CopySubresourceRegion(&self.staging_tex, 0, 0, 0, 0, tex, 0, None);

        if self.curr_img.is_none() {
            self.curr_img = Some(ImageBuf::alloc(
                self.width(),
                self.height(),
                None,
                ColorFormat::Bgra8888,
            ));
        }

        let mut info = zeroed();
        self.ctx
            .Map(&self.staging_tex, 0, D3D11_MAP_READ, 0, Some(&mut info))?;

        //TODO: Handle desktop rotation
        let slice = &*slice_from_raw_parts(
            info.pData as *const u8,
            (info.RowPitch * self.height()).as_usize(),
        );
        let img_ref = Image::new(
            self.width(),
            self.height(),
            info.RowPitch,
            ColorFormat::Bgra8888,
            slice,
        );

        let img_dst = self.curr_img.as_mut().expect("checked above");
        img_ref.copy_into(img_dst);

        self.ctx.Unmap(&self.staging_tex, 0);

        Ok(())
    }
}

impl CaptureStage for DxgiCaptureStage {
    fn resolution(&self) -> (u32, u32) {
        (self.desc.ModeDesc.Width, self.desc.ModeDesc.Height)
    }

    fn next(&mut self) -> Result<DesktopUpdate<Image<&[u8]>>> {
        unsafe {
            let mut frame_info = zeroed();
            let mut desktop = None;

            match self.output_duplication.ReleaseFrame() {
                Ok(_) => {}
                Err(e) => match e.code() {
                    DXGI_ERROR_INVALID_CALL => {}
                    _ => return Err(e.into()),
                },
            }

            match self
                .output_duplication
                .AcquireNextFrame(250, &mut frame_info, &mut desktop)
            {
                Ok(_) => {
                    let mut cursor = None;
                    if frame_info.LastMouseUpdateTime != 0 || self.curr_img.is_none() {
                        let shape = if frame_info.PointerShapeBufferSize != 0 {
                            let mut buf = vec![0u8; frame_info.PointerShapeBufferSize.as_usize()];
                            let mut buf_size = 0;
                            let mut shape_info = zeroed();
                            self.output_duplication.GetFramePointerShape(
                                frame_info.PointerShapeBufferSize,
                                buf.as_mut_ptr() as *mut c_void,
                                &mut buf_size,
                                &mut shape_info,
                            )?;
                            Some(decode_cursor(&shape_info, &buf))
                        } else {
                            None
                        };

                        cursor = Some(CursorState {
                            visible: frame_info.PointerPosition.Visible.as_bool(),
                            pos_x: frame_info.PointerPosition.Position.x as u32,
                            pos_y: frame_info.PointerPosition.Position.y as u32,
                            shape,
                        });
                    }
                    if frame_info.LastPresentTime != 0 || self.curr_img.is_none() {
                        let desktop = desktop.unwrap().cast()?;
                        self.copy_desktop_tex(&desktop)?;
                    }
                    let curr_img = self
                        .curr_img
                        .as_ref()
                        .expect("Must be a valid image after copying into");
                    Ok(DesktopUpdate {
                        cursor,
                        desktop: curr_img.as_data_ref(),
                    })
                }
                Err(e) => match e.code() {
                    DXGI_ERROR_WAIT_TIMEOUT => {
                        let curr_img = self
                            .curr_img
                            .as_ref()
                            .expect("first invocation must not be timeout");
                        Ok(DesktopUpdate {
                            cursor: None,
                            desktop: curr_img.as_data_ref(),
                        })
                    }
                    _ => Err(e.into()),
                },
            }
        }
    }

    fn close(&mut self) {
        todo!()
    }
}

/// Trims null characters at the end of string
fn trim_null(s: &[u16]) -> &[u16] {
    let null_pos = s
        .iter()
        .enumerate()
        .filter_map(|(i, x)| if *x == 0 { Some(i) } else { None })
        .next()
        .unwrap_or(s.len());
    &s[..null_pos]
}

/// SAFETY: Safe as long as `factory` is a valid IDXGIFactory1 instance
unsafe fn list_adapters(factory: &IDXGIFactory1) -> Result<Vec<IDXGIAdapter1>> {
    let mut idx = 0;
    let mut output = vec![];

    loop {
        match factory.EnumAdapters1(idx) {
            Ok(x) => output.push(x),
            Err(e) => {
                if e.code() == DXGI_ERROR_NOT_FOUND {
                    // normal exit
                    break;
                } else {
                    return Err(e.into());
                }
            }
        }
        idx += 1;
    }

    Ok(output)
}

/// SAFETY: Safe as long as `adapter` is a valid IDXGIAdapter1 instance
unsafe fn list_outputs(adapter: &IDXGIAdapter1) -> Result<Vec<IDXGIOutput>> {
    let mut idx = 0;
    let mut output = vec![];

    loop {
        match adapter.EnumOutputs(idx) {
            Ok(x) => output.push(x),
            Err(e) => {
                if e.code() == DXGI_ERROR_NOT_FOUND {
                    // normal exit
                    break;
                } else {
                    return Err(e.into());
                }
            }
        }
        idx += 1;
    }

    Ok(output)
}

fn decode_cursor(shape_info: &DXGI_OUTDUPL_POINTER_SHAPE_INFO, buf: &[u8]) -> CursorShape {
    let mut xor = false;

    let image = match DXGI_OUTDUPL_POINTER_SHAPE_TYPE(shape_info.Type as i32) {
        DXGI_OUTDUPL_POINTER_SHAPE_TYPE_MONOCHROME => {
            xor = true;
            let mut image = ImageBuf::alloc(
                shape_info.Width,
                shape_info.Height / 2,
                None,
                ColorFormat::Bgra8888,
            );

            //FIXME: Poor indexing
            let dst: &mut [u32] = bytemuck::cast_slice_mut(&mut image.data);
            let mut pos = 0;
            for i in 0..(shape_info.Height / 2).as_usize() {
                for j in 0..(shape_info.Width / 8).as_usize() {
                    let mut and_mask = buf[i * shape_info.Pitch.as_usize() + j];
                    let mut xor_mask = buf
                        [(i + shape_info.Height.as_usize() / 2) * shape_info.Pitch.as_usize() + j];

                    for _ in 0..8 {
                        let a: u32 = if (and_mask & 0x80) != 0 { 0 } else { 0xFF };
                        let rgb: u32 = if (xor_mask & 0x80) != 0 { 0xFF } else { 0 };
                        and_mask <<= 1;
                        xor_mask <<= 1;

                        // BGRA (0xAARRGGBB in little endian)
                        dst[pos] = (a << 24) | (rgb << 16) | (rgb << 8) | rgb;
                        pos += 1;
                    }
                }
            }

            image
        }
        DXGI_OUTDUPL_POINTER_SHAPE_TYPE_COLOR => copy_color_data(shape_info, buf),
        DXGI_OUTDUPL_POINTER_SHAPE_TYPE_MASKED_COLOR => {
            xor = true;
            let mut image = copy_color_data(shape_info, buf);

            // flip alpha
            //TODO: Use SIMD or at least u32
            let max_index = (image.height * image.stride).as_usize();
            assert!(max_index <= image.data.len());
            assert!(image.width * 4 <= image.stride);
            for i in 0..image.height {
                for j in 0..image.width {
                    let pos = (i * image.stride + j * 4 + 3).as_usize();
                    unsafe {
                        let alpha = image.data.get_unchecked_mut(pos);
                        *alpha = 0xFF - *alpha;
                    }
                }
            }

            image
        }
        _ => {
            error!("Unknown cursor shape type: {shape_info:?}");

            // blank image
            ImageBuf::alloc(
                shape_info.Width,
                shape_info.Height,
                None,
                ColorFormat::Bgra8888,
            )
        }
    };

    CursorShape {
        image,
        xor,
        hotspot_x: shape_info.HotSpot.x as f32,
        hotspot_y: shape_info.HotSpot.y as f32,
    }
}

fn copy_color_data(shape_info: &DXGI_OUTDUPL_POINTER_SHAPE_INFO, buf: &[u8]) -> ImageBuf {
    let mut image = ImageBuf::alloc(
        shape_info.Width,
        shape_info.Height,
        None,
        ColorFormat::Bgra8888,
    );

    if shape_info.Pitch == shape_info.Width * 4 {
        // fast path
        let total_len = (shape_info.Height * shape_info.Width * 4).as_usize();
        assert_eq!(image.data.len(), total_len);
        image.data.copy_from_slice(&buf[..total_len]);
    } else {
        // copy line by line
        let line_len = (shape_info.Width * 4).as_usize();
        let src_pitch = shape_info.Pitch.as_usize();
        let mut src_offset = 0;
        let mut dst_offset = 0;
        for _ in 0..shape_info.Height {
            image.data[dst_offset..dst_offset + line_len]
                .copy_from_slice(&buf[src_offset..src_offset + line_len]);
            src_offset += src_pitch;
            dst_offset += line_len;
        }
    }

    image
}
