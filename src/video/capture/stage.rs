use crate::image::Image;
use crate::util::DesktopUpdate;
use anyhow::Result;

use std::fmt::Debug;

pub trait CaptureStage: Debug {
    fn resolution(&self) -> (u32, u32);
    fn next(&mut self) -> Result<DesktopUpdate<Image<&[u8]>>>;
    fn close(&mut self);

    fn width(&self) -> u32 {
        self.resolution().0
    }

    fn height(&self) -> u32 {
        self.resolution().1
    }
}
