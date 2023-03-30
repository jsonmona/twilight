use crate::util::{spawn_thread_asyncify, DesktopUpdate, PerformanceMonitor, Timer};
use crate::video::capture::{CaptureStage, GdiCaptureStage};
use crate::video::encoder::jpeg::JpegEncoder;
use crate::video::encoder::EncoderStage;
use anyhow::Result;
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
        let mut capture = GdiCaptureStage::new()?;

        resolution_tx.send(capture.resolution())?;
        drop(resolution_tx);

        let mut perf = PerformanceMonitor::new();
        let mut pref_timer = Timer::new(Duration::from_secs(10));

        let mut prev = None;
        loop {
            if pref_timer.poll() {
                if let Some(x) = perf.get() {
                    println!("Capture {} ms", x.avg.as_millis());
                }
            }

            let update = {
                let _zone = perf.start_zone();

                let update = capture.next()?;
                let (update, desktop) = update.split();
                let desktop = desktop.copied();
                update.with_desktop(desktop)
            };

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
                    println!("Encoder {} ms", x.avg.as_millis());
                }
            }

            let update = {
                let _guard = perf.start_zone();

                let (update, img) = update.split();
                let img = encoder.encode(img)?;
                update.with_desktop(img)
            };

            encoded_tx.blocking_send(update)?;
        }

        anyhow::Ok(())
    });

    tokio::task::spawn_local(async move {
        encode_stage.await.unwrap();
    });

    Ok((w, h, encoded_rx))
}
