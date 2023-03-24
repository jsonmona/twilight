use crate::image::ImageBuf;
use crate::util::DesktopUpdate;
use std::fmt::Debug;
use tokio::sync::mpsc;

pub trait CapturePipeline: Debug + Send {
    fn resolution(&self) -> (u32, u32);
    fn reader(&mut self) -> Option<&mut mpsc::Receiver<DesktopUpdate<ImageBuf>>>;
    fn close(&mut self);
}
