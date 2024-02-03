use crate::{image::ImageBuf, util::DesktopUpdate};
use anyhow::Result;
use std::{fmt::Debug, sync::Arc};

pub trait EncoderStage: Debug + Send + Sync {
    fn configured(&self) -> bool;
    fn configure(self: Arc<Self>) -> Result<()>;

    fn push(&self, update: DesktopUpdate<ImageBuf>);
    fn pop(&self) -> Result<DesktopUpdate<Vec<u8>>>;

    fn shutdown(&self);
}
