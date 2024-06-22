use anyhow::Result;
use std::sync::Arc;
use tokio::sync::mpsc;

use super::capture::CaptureFactoryWin32;

use crate::network::dto::video::Resolution;
use crate::util::DesktopUpdate;
use crate::video::encoder::jpeg::JpegEncoder;
use crate::video::encoder::EncoderStage;

pub type CapturePipelineOutput = (Resolution, mpsc::Receiver<DesktopUpdate<Vec<u8>>>);

pub fn capture_pipeline() -> Result<CapturePipelineOutput> {
    let mut capture_factory = CaptureFactoryWin32::new()?;

    let capture = capture_factory.start("dxgi", "")?;
    let encode: Arc<dyn EncoderStage> = JpegEncoder::new(false)?;

    capture.set_next_stage(Arc::clone(&encode))?;

    Arc::clone(&capture).configure()?;
    Arc::clone(&encode).configure()?;

    let resolution = capture.resolution()?;

    let (tx, rx) = mpsc::channel(1);

    std::thread::spawn(move || {
        loop {
            let update = match encode.pop() {
                Ok(x) => x,
                Err(e) => {
                    log::error!("error while encoding: {}", e);
                    break;
                }
            };

            if tx.blocking_send(update).is_err() {
                break;
            }
        }

        capture.shutdown();
        encode.shutdown();
    });

    Ok((resolution, rx))
}
