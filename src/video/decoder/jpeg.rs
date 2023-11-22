use crate::image::{ColorFormat, Image, ImageBuf};
use crate::video::decoder::DecoderStage;
use anyhow::{ensure, Result};
use zune_jpeg::zune_core::colorspace::ColorSpace;
use zune_jpeg::zune_core::options::DecoderOptions;
use zune_jpeg::JpegDecoder as JDecoder;

#[derive(Debug)]
pub struct JpegDecoder {
    width: u16,
    height: u16,
}

impl JpegDecoder {
    pub fn new(w: u32, h: u32) -> Result<Self> {
        ensure!(
            w <= u16::MAX as u32 && h <= u16::MAX as u32,
            "image dimension cannot be larger than 65535"
        );

        Ok(JpegDecoder {
            width: w as u16,
            height: h as u16,
        })
    }
}

impl DecoderStage for JpegDecoder {
    fn resolution(&self) -> (u32, u32) {
        (self.width as u32, self.height as u32)
    }

    fn decode(&mut self, data: &[u8]) -> Result<ImageBuf> {
        let opts = DecoderOptions::new_fast().jpeg_set_out_colorspace(ColorSpace::BGRA);
        let mut decoder = JDecoder::new_with_options(opts, data);

        decoder.decode_headers()?;

        let (w, h) = decoder.dimensions().expect("headers already decoded");

        ensure!(
            self.width == w && self.height == h,
            "image resolution changed from {}x{} to {}x{}",
            self.width,
            self.height,
            w,
            h
        );

        let img = decoder.decode()?;

        Ok(Image::new(
            self.width as u32,
            self.height as u32,
            self.width as u32 * 4,
            ColorFormat::Bgra8888,
            img,
        ))
    }
}
