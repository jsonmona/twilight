use crate::image::ImageBuf;
use crate::platform::win32::capture_gdi::CaptureGdi;
use crate::server::capture_pipeline::CapturePipeline;
use crate::util::DesktopUpdate;
use anyhow::Result;
use std::sync::{Arc, Condvar, Mutex};
use std::thread::JoinHandle;
use tokio::sync::mpsc;

#[derive(Debug)]
pub struct Win32CapturePipeline {
    runner: Option<JoinHandle<Result<()>>>,
    channel: Option<mpsc::Receiver<DesktopUpdate<ImageBuf>>>,
    resolution: (u32, u32),
}

impl Drop for Win32CapturePipeline {
    fn drop(&mut self) {
        // Not closing is fine as far as safety is concerned
        //TODO: Convert this into a log
        debug_assert!(
            self.channel.is_none(),
            "Win32CapturePipeline was not closed before Drop"
        );
    }
}

impl CapturePipeline for Win32CapturePipeline {
    fn resolution(&self) -> (u32, u32) {
        self.resolution
    }

    fn reader(&mut self) -> Option<&mut mpsc::Receiver<DesktopUpdate<ImageBuf>>> {
        match &mut self.channel {
            Some(x) => Some(x),
            None => None,
        }
    }

    fn close(&mut self) {
        drop(self.channel.take());

        if let Some(x) = self.runner.take() {
            x.join().expect("capture_fn has panicked").unwrap();
        }
    }
}

impl Win32CapturePipeline {
    pub fn new() -> Result<Self> {
        let (desktop_tx, desktop_rx) = mpsc::channel(1);

        //TODO: Clean up code
        let resolution = Arc::new(Mutex::new(None));
        let resolution_inner = Arc::clone(&resolution);
        let resolution_condvar = Arc::new(Condvar::new());
        let resolution_condvar_inner = Arc::clone(&resolution_condvar);

        let runner = std::thread::spawn(move || {
            let gdi = CaptureGdi::new()?;
            {
                let mut lock = resolution_inner.lock().unwrap();
                *lock = Some(gdi.resolution());
                resolution_condvar_inner.notify_all();
            }
            drop(resolution_inner);
            drop(resolution_condvar_inner);
            capture_fn(desktop_tx, gdi)
        });

        let resolution = {
            let lock = resolution.lock().unwrap();
            let mut lock = resolution_condvar
                .wait_while(lock, |x| x.is_none())
                .unwrap();
            lock.take().unwrap()
        };

        Ok(Win32CapturePipeline {
            runner: Some(runner),
            channel: Some(desktop_rx),
            resolution,
        })
    }
}

fn capture_fn(
    desktop_tx: mpsc::Sender<DesktopUpdate<ImageBuf>>,
    mut gdi: CaptureGdi,
) -> Result<()> {
    loop {
        let update = gdi.capture()?;
        let update = DesktopUpdate {
            cursor: update.cursor,
            desktop: update.desktop.copied(),
        };

        if desktop_tx.blocking_send(update).is_err() {
            break;
        }
    }

    Ok(())
}
