use crate::image::{ColorFormat, ImageBuf};
use crate::video::encoder::stage::EncoderStage;
use anyhow::{ensure, Context, Result};
use turbojpeg::{Compressor, PixelFormat, Subsamp};

#[derive(Debug)]
pub struct JpegEncoder {
    width: u32,
    height: u32,
    compressor: Compressor,
}

impl JpegEncoder {
    pub fn new(w: u32, h: u32, yuv444: bool) -> Result<Self> {
        let mut compressor = Compressor::new()?;
        compressor.set_quality(100);
        if yuv444 {
            compressor.set_subsamp(Subsamp::None);
        } else {
            compressor.set_subsamp(Subsamp::Sub2x2);
        }

        Ok(JpegEncoder {
            width: w,
            height: h,
            compressor,
        })
    }
}

impl EncoderStage for JpegEncoder {
    fn resolution(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    fn encode(&mut self, img: ImageBuf) -> Result<Vec<u8>> {
        ensure!(
            self.width == img.width,
            "image width changed to {}",
            img.width
        );
        ensure!(
            self.height == img.height,
            "image height changed to {}",
            img.height
        );

        let w: usize = self.width.try_into()?;
        let h: usize = self.height.try_into()?;

        let image = turbojpeg::Image {
            pixels: img.data.as_slice(),
            width: w,
            pitch: img.stride.try_into()?,
            height: h,
            format: color_format_to_pixel_format(img.color_format)
                .with_context(|| format!("unknown color format {:?}", img.color_format))?,
        };

        let max_len = self.compressor.buf_len(w, h)?;
        let mut buf = vec![0; max_len];

        let actual_len = self.compressor.compress_to_slice(image, &mut buf)?;
        buf.truncate(actual_len);

        Ok(buf)
    }
}

fn color_format_to_pixel_format(fmt: ColorFormat) -> Option<PixelFormat> {
    match fmt {
        ColorFormat::Bgra8888 => Some(PixelFormat::BGRA),
        ColorFormat::Rgba8888 => Some(PixelFormat::RGBA),
        ColorFormat::Rgb24 => Some(PixelFormat::RGB),
        _ => None,
    }
}
