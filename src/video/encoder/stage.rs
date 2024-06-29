use crate::{image::ImageBuf, util::DesktopUpdate};
use anyhow::Result;
use std::{fmt::Debug, sync::Arc};

pub trait EncoderStage: Debug + Send + Sync {
    fn configured(&self) -> bool;
    fn configure(self: Arc<Self>) -> Result<()>;

    fn input(&self) -> flume::Sender<DesktopUpdate<ImageBuf>>;
    fn set_output(&self, tx: flume::Sender<DesktopUpdate<Vec<u8>>>);

    fn shutdown(&self);
}
