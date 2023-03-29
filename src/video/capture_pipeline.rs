use crate::util::{DesktopUpdate, PerformanceMonitor};
use crate::video::capture::{CaptureStage, DxgiCaptureStage};
use crate::video::encoder::jpeg::JpegEncoder;
use crate::video::encoder::EncoderStage;
use anyhow::Result;
use std::sync::mpsc;
use std::sync::mpsc::TrySendError;
use std::time::{Duration, Instant};

pub type CapturePipelineOutput = (
    u32,
    u32,
    tokio::sync::mpsc::Receiver<DesktopUpdate<Vec<u8>>>,
);

pub fn capture_pipeline() -> Result<CapturePipelineOutput> {
    let (resolution_tx, resolution_rx) = mpsc::channel();
    let (img_tx, img_rx) = mpsc::sync_channel(1);
    let (encoded_tx, encoded_rx) = tokio::sync::mpsc::channel(1);

    let capture_stage = std::thread::spawn(move || {
        let mut capture = DxgiCaptureStage::new()?;

        resolution_tx.send(capture.resolution())?;
        drop(resolution_tx);

        let mut perf = PerformanceMonitor::new();
        let mut last_print = Instant::now();

        let mut prev = None;
        loop {
            let curr_time = Instant::now();
            if Duration::from_millis(10000) < curr_time - last_print {
                last_print = curr_time;
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

    let resolution = resolution_rx.recv()?;
    let (w, h) = resolution;

    let encode_stage = std::thread::spawn(move || {
        let mut encoder = JpegEncoder::new(w, h, true)?;

        let mut perf = PerformanceMonitor::new();
        let mut last_print = Instant::now();

        while let Ok(update) = img_rx.recv() {
            let curr_time = Instant::now();
            if Duration::from_millis(10000) < curr_time - last_print {
                last_print = curr_time;
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
        while !capture_stage.is_finished() {
            tokio::time::sleep(Duration::from_millis(150)).await;
        }
        capture_stage.join().unwrap().unwrap();
    });

    tokio::task::spawn_local(async move {
        while !encode_stage.is_finished() {
            tokio::time::sleep(Duration::from_millis(150)).await;
        }
        encode_stage.join().unwrap().unwrap();
    });

    Ok((w, h, encoded_rx))
}
