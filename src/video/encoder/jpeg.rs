use std::io::Cursor;
use crate::image::{ColorFormat, Image};
use crate::util::AsUsize;
use crate::video::encoder::stage::EncoderStage;
use anyhow::{ensure, Context, Result};
use jpeg_encoder::{ColorType, Encoder, SamplingFactor};

#[derive(Debug)]
pub struct JpegEncoder {
    width: u16,
    height: u16,
    yuv444: bool,
}

impl JpegEncoder {
    pub fn new(w: u32, h: u32, yuv444: bool) -> Result<Self> {
        ensure!(w <= u16::MAX as u32 && h <= u16::MAX as u32, "image dimension cannot be larger than 65535");

        Ok(JpegEncoder {
            width: w as u16,
            height: h as u16,
            yuv444,
        })
    }
}

impl EncoderStage for JpegEncoder {
    fn resolution(&self) -> (u32, u32) {
        (self.width as u32, self.height as u32)
    }

    fn encode(&mut self, img: Image<&[u8]>) -> Result<Vec<u8>> {
        let buf_len = max_buffer_size(self.width, self.height).context("image too large")?;
        let buf = vec![0u8; buf_len];
        let mut cursor = Cursor::new(buf);

        let mut encoder = Encoder::new(&mut cursor, 90);

        encoder.set_sampling_factor(if self.yuv444 { SamplingFactor::R_4_4_4 } else { SamplingFactor::R_4_2_0 });

        ensure!(
            self.width as u32 == img.width && self.height as u32 == img.height,
            "image resolution changed from {}x{} to {}x{}",
            self.width,
            self.height,
            img.width,
            img.height
        );

        let color_type = color_format_to_color_type(img.color_format).context("unknown color_format")?;

        encoder.encode(img.data, self.width, self.height, color_type)?;

        let output_len = cursor.position() as usize;
        let mut buf = cursor.into_inner();
        buf.truncate(output_len);

        Ok(buf)
    }
}

fn color_format_to_color_type(fmt: ColorFormat) -> Option<ColorType> {
    match fmt {
        ColorFormat::Bgra8888 => Some(ColorType::Bgra),
        ColorFormat::Rgba8888 => Some(ColorType::Rgba),
        ColorFormat::Rgb24 => Some(ColorType::Rgb),
        _ => None,
    }
}

fn max_buffer_size(width: u16, height: u16) -> Option<usize> {
    let padded_w = (width as usize).checked_next_multiple_of(16)?;
    let padded_h = (height as usize).checked_next_multiple_of(16)?;

    padded_w.checked_mul(padded_h)?.checked_mul(6)?.checked_add(2048)
}
