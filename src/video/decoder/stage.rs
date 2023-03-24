use crate::image::ImageBuf;
use anyhow::Result;
use std::fmt::Debug;

pub trait DecoderStage: Send + Debug {
    fn resolution(&self) -> (u32, u32);
    fn decode(&mut self, data: &[u8]) -> Result<ImageBuf>;

    fn width(&self) -> u32 {
        self.resolution().0
    }

    fn height(&self) -> u32 {
        self.resolution().1
    }
}
