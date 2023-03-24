use crate::image::ImageBuf;
use anyhow::Result;
use std::fmt::Debug;

pub trait EncoderStage: Send + Debug {
    fn resolution(&self) -> (u32, u32);
    fn encode(&mut self, img: ImageBuf) -> Result<Vec<u8>>;

    fn width(&self) -> u32 {
        self.resolution().0
    }

    fn height(&self) -> u32 {
        self.resolution().1
    }
}
