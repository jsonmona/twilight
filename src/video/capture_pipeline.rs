use anyhow::Result;
use std::sync::Arc;

use super::capture::CaptureFactoryWin32;

use crate::network::dto::video::Resolution;
use crate::server::{normal_defaults, ServerConfig};
use crate::util::DesktopUpdate;
use crate::video::encoder::jpeg::JpegEncoder;
use crate::video::encoder::EncoderStage;

pub type CapturePipelineOutput = (Resolution, flume::Receiver<DesktopUpdate<Vec<u8>>>);

pub fn capture_pipeline(config: &ServerConfig) -> Result<CapturePipelineOutput> {
    let mut capture_factory = CaptureFactoryWin32::new()?;

    let capture_method = config
        .desktop_capture_method
        .unwrap_or_else(|| normal_defaults().desktop_capture_method.unwrap());

    let (tx, rx) = flume::bounded(1);

    let capture = capture_factory.start(capture_method, "")?;
    let encode: Arc<dyn EncoderStage> = JpegEncoder::new(false)?;

    capture.set_next_stage(Arc::clone(&encode))?;
    encode.set_output(tx);

    Arc::clone(&capture).configure()?;
    Arc::clone(&encode).configure()?;

    let resolution = capture.resolution()?;

    Ok((resolution, rx))
}
