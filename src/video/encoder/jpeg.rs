use std::io::Cursor;
use std::sync::{Arc, Condvar, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use anyhow::{Context, Result};
use jpeg_encoder::{ColorType, Encoder, SamplingFactor};

use crate::image::{ColorFormat, Image, ImageBuf};
use crate::util::DesktopUpdate;
use crate::video::encoder::stage::EncoderStage;

#[derive(Debug)]
pub struct JpegEncoder {
    input: flume::Sender<DesktopUpdate<ImageBuf>>,
    output: Mutex<Option<flume::Sender<DesktopUpdate<Vec<u8>>>>>,
    configured: Condvar,
    _worker: JoinHandle<Result<()>>,
}

impl JpegEncoder {
    pub fn new(yuv444: bool) -> Result<Arc<Self>> {
        let (tx, rx) = flume::bounded::<DesktopUpdate<ImageBuf>>(1);

        let instance = Arc::new_cyclic(move |ptr_outer| {
            let ptr = ptr_outer.clone();

            let worker = std::thread::spawn(move || {
                let this: Arc<Self> = loop {
                    match ptr.upgrade() {
                        Some(x) => break x,
                        None => std::thread::sleep(Duration::from_millis(10)),
                    }
                };

                let tx = {
                    let mut guard = this.output.lock().unwrap();
                    while guard.is_none() {
                        guard = this.configured.wait(guard).unwrap();
                    }

                    guard.as_ref().expect("checked above").clone()
                };

                while let Ok(mut update) = rx.recv() {
                    update.timings.encode_begin = update.timings.elapsed_since_capture().unwrap();
                    let encoded = encode_img(update.desktop.as_data_ref(), yuv444)?;
                    update.timings.encode_end = update.timings.elapsed_since_capture().unwrap();

                    if tx.send(update.with_desktop(encoded)).is_err() {
                        return Ok(());
                    }
                }

                Ok(())
            });

            JpegEncoder {
                input: tx,
                output: Mutex::new(None),
                configured: Condvar::new(),
                _worker: worker,
            }
        });

        Ok(instance)
    }
}

impl EncoderStage for JpegEncoder {
    fn configured(&self) -> bool {
        true
    }

    fn configure(self: Arc<Self>) -> Result<()> {
        Ok(())
    }

    fn input(&self) -> flume::Sender<DesktopUpdate<ImageBuf>> {
        self.input.clone()
    }

    fn set_output(&self, tx: flume::Sender<DesktopUpdate<Vec<u8>>>) {
        *self.output.lock().unwrap() = Some(tx);
        self.configured.notify_all();
    }

    fn shutdown(&self) {}
}

fn encode_img(img: Image<&[u8]>, yuv444: bool) -> Result<Vec<u8>> {
    let width: u16 = img
        .width
        .try_into()
        .context("image dimension must not exceed 65535")?;
    let height: u16 = img
        .height
        .try_into()
        .context("image dimension must not exceed 65535")?;

    let buf_len = max_buffer_size(width, height);
    let buf = vec![0u8; buf_len];
    let mut cursor = Cursor::new(buf);

    let mut encoder = Encoder::new(&mut cursor, 90);

    encoder.set_sampling_factor(if yuv444 {
        SamplingFactor::R_4_4_4
    } else {
        SamplingFactor::R_4_2_0
    });

    let color_type =
        color_format_to_color_type(img.color_format).context("unknown color_format")?;

    encoder.encode(img.data, width, height, color_type)?;

    let output_len = cursor.position() as usize;
    let mut buf = cursor.into_inner();
    buf.truncate(output_len);

    Ok(buf)
}

fn color_format_to_color_type(fmt: ColorFormat) -> Option<ColorType> {
    match fmt {
        ColorFormat::Bgra8888 => Some(ColorType::Bgra),
        ColorFormat::Rgba8888 => Some(ColorType::Rgba),
        ColorFormat::Rgb24 => Some(ColorType::Rgb),
        _ => None,
    }
}

fn max_buffer_size(width: u16, height: u16) -> usize {
    let padded_w = (width as usize).next_multiple_of(16);
    let padded_h = (height as usize).next_multiple_of(16);

    padded_w * padded_h * 6 + 2048
}
