use crate::video::encoder::EncoderStage;
use anyhow::Result;
use std::fmt::Debug;
use std::sync::Arc;

use super::{RefreshRate, Resolution};

pub trait CaptureStage: Debug + Send + Sync {
    fn configured(&self) -> bool;
    fn resolution(&self) -> Result<Resolution>;
    fn refresh_rate(&self) -> Result<RefreshRate>;

    fn set_next_stage(&self, encoder: Arc<dyn EncoderStage>) -> Result<()>;

    fn configure(self: Arc<Self>) -> Result<()>;

    fn shutdown(&self);
}

// ensure object safety
const _: Option<&dyn CaptureStage> = None;
