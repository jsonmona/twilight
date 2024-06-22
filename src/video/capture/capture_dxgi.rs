use crate::image::{ColorFormat, Image, ImageBuf};
use crate::util::{AsUsize, CursorShape, CursorState, DesktopUpdate, NonSend};
use crate::video::capture::CaptureStage;
use crate::video::encoder::EncoderStage;
use anyhow::{anyhow, ensure, Context, Result};
use log::{error, info};
use parking_lot::RwLock;
use std::ffi::c_void;
use std::marker::PhantomData;
use std::mem::zeroed;
use std::ptr::slice_from_raw_parts;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};
use std::thread::JoinHandle;
use windows::core::*;
use windows::Win32::Graphics::Direct3D::*;
use windows::Win32::Graphics::Direct3D11::*;
use windows::Win32::Graphics::Dxgi::Common::*;
use windows::Win32::Graphics::Dxgi::*;

use crate::network::dto::video::{RefreshRate, Resolution};

#[derive(Debug)]
pub struct CaptureDxgi {
    factory: IDXGIFactory1,
    dev_id: Vec<u16>,
    shutdown: AtomicBool,
    next_stage: OnceLock<Arc<dyn EncoderStage>>,
    worker: RwLock<Option<JoinHandle<Result<()>>>>,
    desc: OnceLock<DXGI_OUTDUPL_DESC>,
}

impl CaptureDxgi {
    /// dev_id: The slice from WCHAR szDevice[16]. Must end with a NULL character.
    pub fn new(factory: IDXGIFactory1, dev_id: Vec<u16>) -> Result<Arc<Self>> {
        assert_eq!(
            dev_id.last(),
            Some(&0),
            "dev_id must end with a NULL character"
        );

        Ok(Arc::new(Self {
            factory,
            dev_id,
            shutdown: AtomicBool::new(false),
            next_stage: Default::default(),
            worker: Default::default(),
            desc: Default::default(),
        }))
    }

    fn read_desc<T: 'static>(&self, f: impl FnOnce(&DXGI_OUTDUPL_DESC) -> T) -> T {
        loop {
            match self.desc.get() {
                Some(x) => {
                    return f(x);
                }
                None => {}
            }

            std::thread::yield_now();
        }
    }
}

impl CaptureStage for CaptureDxgi {
    fn configured(&self) -> bool {
        self.desc.get().is_some()
    }

    fn resolution(&self) -> Result<Resolution> {
        Ok(self.read_desc(|x| Resolution {
            width: x.ModeDesc.Width,
            height: x.ModeDesc.Height,
        }))
    }

    fn refresh_rate(&self) -> Result<RefreshRate> {
        Ok(self.read_desc(|x| RefreshRate {
            num: x.ModeDesc.RefreshRate.Numerator,
            den: x.ModeDesc.RefreshRate.Denominator,
        }))
    }

    fn set_next_stage(&self, encoder: Arc<dyn EncoderStage>) -> Result<()> {
        self.next_stage
            .set(encoder)
            .map_err(|_| anyhow!("next_stage already set"))
    }

    fn configure(self: Arc<Self>) -> Result<()> {
        if self.next_stage.get().is_none() {
            return Err(anyhow!("next_stage not set"));
        }

        let this = Arc::clone(&self);

        let mut guard = self.worker.write();
        if guard.is_some() {
            return Err(anyhow!("worker already running"));
        }

        *guard = Some(std::thread::spawn(move || capture_loop(this)));

        Ok(())
    }

    fn shutdown(&self) {
        self.shutdown.store(true, Ordering::Relaxed);
    }
}

struct Resources {
    _guard: PhantomData<NonSend>,
    adapter: IDXGIAdapter1,
    output: IDXGIOutput1,
    device: ID3D11Device,
    ctx: ID3D11DeviceContext,
    outdupl: IDXGIOutputDuplication,
    desc: DXGI_OUTDUPL_DESC,
    staging_tex: ID3D11Texture2D,
}

fn capture_loop(stage: Arc<CaptureDxgi>) -> Result<()> {
    let next_stage = Arc::clone(stage.next_stage.get().context("next_stage not set")?);

    unsafe {
        let mut res = init_capture(&stage)?;
        stage.desc.set(res.desc.clone());

        let mut is_first = true;

        while !stage.shutdown.load(Ordering::Relaxed) {
            if let Some(update) = next_img(&mut res, is_first)? {
                let update = update.and_then_desktop(|tex| download_image(&mut res, &tex))?;

                next_stage.push(update);

                is_first = false;
            }
        }
    }

    Ok(())
}

unsafe fn init_capture(stage: &CaptureDxgi) -> Result<Resources> {
    let adapter;
    let output;
    let device;
    let ctx;
    let outdupl;
    let desc;
    let staging_tex;

    let factory = &stage.factory;

    unsafe {
        let adapters = list_adapters(factory)?;
        ensure!(!adapters.is_empty(), "No D3D adapter found");

        // Select primary adapter
        adapter = adapters
            .into_iter()
            .next()
            .expect("No primary adapter found");

        let mut adapter_desc = zeroed();
        adapter.GetDesc1(&mut adapter_desc)?;

        let adapter_name = u16_to_string(&adapter_desc.Description);
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

        let output_name = u16_to_string(&output_desc.DeviceName);
        info!("Selected output {output_name}");

        let mut flags = D3D11_CREATE_DEVICE_SINGLETHREADED | D3D11_CREATE_DEVICE_BGRA_SUPPORT;

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

        outdupl = output.DuplicateOutput(&device)?;
        let mut outdupl_desc = zeroed();
        outdupl.GetDesc(&mut outdupl_desc);
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
                BindFlags: 0,
                CPUAccessFlags: D3D11_CPU_ACCESS_READ.0 as u32,
                MiscFlags: 0,
            },
            None,
            Some(&mut p_staging_tex),
        )?;
        staging_tex =
            p_staging_tex.expect("A successful CreateTexture2D call must return a valid texture");
    }

    //TODO: Implement optimization using DesktopImageInSystemMemory. I'm yet to encounter
    //      any system with that flag true. Please create a GitHub issue if you have one.

    Ok(Resources {
        _guard: Default::default(),
        adapter,
        output,
        device,
        ctx,
        outdupl,
        desc,
        staging_tex,
    })
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

fn u16_to_string(s: &[u16]) -> String {
    for (i, ch) in s.iter().copied().enumerate() {
        if ch == 0 {
            return String::from_utf16_lossy(&s[..i]);
        }
    }

    // no NULL found
    String::from_utf16_lossy(s)
}

unsafe fn next_img(
    res: &mut Resources,
    is_first: bool,
) -> Result<Option<DesktopUpdate<ID3D11Texture2D>>> {
    let mut frame_info = zeroed();
    let mut desktop = None;

    match res.outdupl.ReleaseFrame() {
        Ok(_) => {}
        Err(e) => match e.code() {
            DXGI_ERROR_INVALID_CALL => {}
            _ => return Err(e.into()),
        },
    }

    match res
        .outdupl
        .AcquireNextFrame(250, &mut frame_info, &mut desktop)
    {
        Ok(_) => {
            let mut cursor = None;
            if frame_info.LastMouseUpdateTime != 0 || is_first {
                let shape = if frame_info.PointerShapeBufferSize != 0 {
                    let mut buf = vec![0u8; frame_info.PointerShapeBufferSize.as_usize()];
                    let mut buf_size = 0;
                    let mut shape_info = zeroed();
                    res.outdupl.GetFramePointerShape(
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
            if frame_info.LastPresentTime != 0 || is_first {
                //TODO: Some optimization may happen here
            }

            Ok(Some(DesktopUpdate {
                cursor,
                desktop: desktop
                    .context("a successful AcquireNextFrame did not return texture")?
                    .cast()?,
            }))
        }
        Err(e) => match e.code() {
            DXGI_ERROR_WAIT_TIMEOUT => Ok(None),
            _ => Err(e.into()),
        },
    }
}

unsafe fn download_image(res: &mut Resources, tex: &ID3D11Texture2D) -> Result<ImageBuf> {
    let mut src_desc = zeroed();
    res.staging_tex.GetDesc(&mut src_desc);
    assert_eq!(
        src_desc.Format, DXGI_FORMAT_B8G8R8A8_UNORM,
        "Only B8G8R8A8 is supported"
    );

    let width = src_desc.Width;
    let height = src_desc.Height;

    res.ctx
        .CopySubresourceRegion(&res.staging_tex, 0, 0, 0, 0, tex, 0, None);

    let mut dst = ImageBuf::alloc(width, height, None, ColorFormat::Bgra8888);

    let mut info = zeroed();
    res.ctx
        .Map(&res.staging_tex, 0, D3D11_MAP_READ, 0, Some(&mut info))?;

    //TODO: Handle desktop rotation
    let slice =
        &*slice_from_raw_parts(info.pData as *const u8, (info.RowPitch * height).as_usize());
    let img_ref = Image::new(width, height, info.RowPitch, ColorFormat::Bgra8888, slice);

    img_ref.copy_into(&mut dst);

    res.ctx.Unmap(&res.staging_tex, 0);

    Ok(dst)
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
