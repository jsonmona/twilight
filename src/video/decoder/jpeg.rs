use crate::image::{ColorFormat, ImageBuf};
use crate::util::AsUsize;
use crate::video::decoder::DecoderStage;
use anyhow::{ensure, Result};
use turbojpeg::{Decompressor, PixelFormat};

#[derive(Debug)]
pub struct JpegDecoder {
    width: u32,
    height: u32,
    decompressor: Decompressor,
}

impl JpegDecoder {
    pub fn new(w: u32, h: u32) -> Result<Self> {
        let decompressor = Decompressor::new()?;

        Ok(JpegDecoder {
            width: w,
            height: h,
            decompressor,
        })
    }
}

impl DecoderStage for JpegDecoder {
    fn resolution(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    fn decode(&mut self, data: &[u8]) -> Result<ImageBuf> {
        let header = self.decompressor.read_header(data)?;

        ensure!(
            self.width.equals_usize(header.width) && self.height.equals_usize(header.height),
            "image resolution changed from {}x{} to {}x{}",
            self.width,
            self.height,
            header.width,
            header.height
        );

        let mut img = ImageBuf::alloc(self.width, self.height, None, ColorFormat::Bgra8888);

        let image = turbojpeg::Image {
            pixels: img.data.as_mut_slice(),
            width: header.width,
            pitch: img.stride.as_usize(),
            height: header.height,
            format: PixelFormat::BGRA,
        };

        self.decompressor.decompress(data, image)?;
        Ok(img)
    }
}
