use crate::util::DesktopUpdate;
use crate::video::capture::{CaptureStage, GdiCaptureStage};
use crate::video::encoder::jpeg::JpegEncoder;
use crate::video::encoder::EncoderStage;
use anyhow::Result;
use std::sync::mpsc;
use std::sync::mpsc::TrySendError;

pub type CapturePipelineOutput = (
    u32,
    u32,
    tokio::sync::mpsc::Receiver<DesktopUpdate<Vec<u8>>>,
);

pub fn capture_pipeline() -> Result<CapturePipelineOutput> {
    let (resolution_tx, resolution_rx) = mpsc::channel();
    let (img_tx, img_rx) = mpsc::sync_channel(1);
    let (encoded_tx, encoded_rx) = tokio::sync::mpsc::channel(1);

    std::thread::spawn(move || {
        let mut capture = GdiCaptureStage::new()?;

        resolution_tx.send(capture.resolution())?;
        drop(resolution_tx);

        let mut prev = None;
        loop {
            let update = capture.next()?;
            let (update, desktop) = update.split();
            let desktop = desktop.copied();
            let update = update.with_desktop(desktop);
            match img_tx.try_send(update) {
                Ok(_) => continue,
                Err(e) => match e {
                    TrySendError::Full(mut update) => {
                        prev = match prev {
                            Some(prev_update) => {
                                update.collapse_from(prev_update);
                                Some(update)
                            }
                            None => Some(update),
                        };
                    }
                    TrySendError::Disconnected(_) => break,
                },
            }
        }
        anyhow::Ok(())
    });

    let resolution = resolution_rx.recv()?;
    let (w, h) = resolution;

    std::thread::spawn(move || -> Result<()> {
        let mut encoder = JpegEncoder::new(w, h, true)?;

        while let Ok(update) = img_rx.recv() {
            let (update, img) = update.split();
            let img = encoder.encode(img)?;
            let update = update.with_desktop(img);
            encoded_tx.blocking_send(update)?;
        }

        anyhow::Ok(())
    });

    Ok((w, h, encoded_rx))
}
