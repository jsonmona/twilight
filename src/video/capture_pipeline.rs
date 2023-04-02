use crate::util::{spawn_thread_asyncify, DesktopUpdate, PerformanceMonitor, Timer};
use crate::video::capture::{CaptureStage, DxgiCaptureStage};
use crate::video::encoder::jpeg::JpegEncoder;
use crate::video::encoder::EncoderStage;
use anyhow::Result;
use log::info;
use std::sync::mpsc;
use std::sync::mpsc::TrySendError;
use std::time::Duration;

pub type CapturePipelineOutput = (
    u32,
    u32,
    tokio::sync::mpsc::Receiver<DesktopUpdate<Vec<u8>>>,
);

pub fn capture_pipeline() -> Result<CapturePipelineOutput> {
    let (resolution_tx, resolution_rx) = mpsc::channel();
    let (img_tx, img_rx) = mpsc::sync_channel(1);
    let (encoded_tx, encoded_rx) = tokio::sync::mpsc::channel(1);

    let capture_stage = spawn_thread_asyncify(move || {
        let mut capture = DxgiCaptureStage::new()?;

        resolution_tx.send(capture.resolution())?;
        drop(resolution_tx);

        let mut perf = PerformanceMonitor::new();
        let mut pref_timer = Timer::new(Duration::from_secs(10));

        let mut prev = None;
        loop {
            if pref_timer.poll() {
                if let Some(x) = perf.get() {
                    info!("Capture {} ms", x.avg.as_millis());
                }
            }

            let mut update = {
                let _zone = perf.start_zone();

                let update = capture.next()?;
                update.map_desktop(|x| x.copied())
            };

            if let Some(x) = prev.take() {
                update.collapse_from(x);
            }

            match img_tx.try_send(update) {
                Ok(_) => continue,
                Err(e) => match e {
                    TrySendError::Full(update) => {
                        prev = Some(update);
                    }
                    TrySendError::Disconnected(_) => break,
                },
            }
        }
        anyhow::Ok(())
    });

    tokio::task::spawn_local(async move {
        capture_stage.await.unwrap();
    });

    let resolution = resolution_rx.recv()?;
    let (w, h) = resolution;

    let encode_stage = spawn_thread_asyncify(move || {
        let mut encoder = JpegEncoder::new(w, h, true)?;

        let mut perf = PerformanceMonitor::new();
        let mut pref_timer = Timer::new(Duration::from_secs(10));

        while let Ok(update) = img_rx.recv() {
            if pref_timer.poll() {
                if let Some(x) = perf.get() {
                    info!("Encoder {} ms", x.avg.as_millis());
                }
            }

            let update = {
                let _guard = perf.start_zone();

                update.and_then_desktop(|x| encoder.encode(x.as_data_ref()))?
            };

            if encoded_tx.blocking_send(update).is_err() {
                // channel closed is a normal exit
                break;
            }
        }

        anyhow::Ok(())
    });

    tokio::task::spawn_local(async move {
        encode_stage.await.unwrap();
    });

    Ok((w, h, encoded_rx))
}
