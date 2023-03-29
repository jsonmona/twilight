use crate::image::{ColorFormat, Image, ImageBuf};
use crate::util::DesktopUpdate;
use crate::video::capture::CaptureStage;
use anyhow::{ensure, Context, Result};
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
            println!("Selected adapter {adapter_name}");

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
            println!("Selected output {output_name}");

            let flags = D3D11_CREATE_DEVICE_SINGLETHREADED
                | D3D11_CREATE_DEVICE_BGRA_SUPPORT
                | D3D11_CREATE_DEVICE_DEBUG;
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
            (info.RowPitch * self.height()).try_into()?,
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

            match self
                .output_duplication
                .AcquireNextFrame(60000, &mut frame_info, &mut desktop)
            {
                Ok(_) => {
                    let desktop = desktop.unwrap().cast()?;
                    self.copy_desktop_tex(&desktop)?;
                    self.output_duplication.ReleaseFrame()?;
                    let curr_img = self
                        .curr_img
                        .as_ref()
                        .expect("Must be a valid image after copying into");
                    let image = Image::new(
                        curr_img.width,
                        curr_img.height,
                        curr_img.stride,
                        curr_img.color_format,
                        curr_img.data.as_slice(),
                    );
                    Ok(DesktopUpdate {
                        cursor: None,
                        desktop: image,
                    })
                }
                Err(e) => match e.code() {
                    DXGI_ERROR_WAIT_TIMEOUT => {
                        let desktop = desktop.unwrap().cast()?;
                        self.copy_desktop_tex(&desktop)?;
                        self.output_duplication.ReleaseFrame()?;
                        let curr_img = self
                            .curr_img
                            .as_ref()
                            .expect("Must be a valid image after copying into");
                        let image = Image::new(
                            curr_img.width,
                            curr_img.height,
                            curr_img.stride,
                            curr_img.color_format,
                            curr_img.data.as_slice(),
                        );
                        Ok(DesktopUpdate {
                            cursor: None,
                            desktop: image,
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
