use std::io::Cursor;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use jpeg_encoder::{ColorType, Encoder, SamplingFactor};
use parking_lot::{Condvar, Mutex, MutexGuard};

use crate::image::{ColorFormat, Image, ImageBuf};
use crate::util::DesktopUpdate;
use crate::video::encoder::stage::EncoderStage;

#[derive(Debug)]
pub struct JpegEncoder {
    output: Mutex<Option<DesktopUpdate<ImageBuf>>>,
    event: Condvar,
    yuv444: bool,
}

impl JpegEncoder {
    pub fn new(yuv444: bool) -> Result<Arc<Self>> {
        Ok(Arc::new(JpegEncoder {
            output: Mutex::new(None),
            event: Condvar::new(),
            yuv444,
        }))
    }
}

impl EncoderStage for JpegEncoder {
    fn configured(&self) -> bool {
        true
    }

    fn configure(self: Arc<Self>) -> Result<()> {
        Ok(())
    }

    fn push(&self, update: DesktopUpdate<ImageBuf>) {
        let mut guard = self.output.lock();
        while guard.is_some() {
            if self
                .event
                .wait_for(&mut guard, Duration::from_secs(1))
                .timed_out()
            {
                log::warn!("JpegEncoder::push waited 1s");
            }
        }

        assert!(guard.is_none());
        *guard = Some(update);

        self.event.notify_all();
        MutexGuard::unlock_fair(guard);
    }

    fn pop(&self) -> Result<DesktopUpdate<Vec<u8>>> {
        let mut guard = self.output.lock();
        while guard.is_none() {
            if self
                .event
                .wait_for(&mut guard, Duration::from_secs(1))
                .timed_out()
            {
                log::warn!("JpegEncoder::pop waited 1s");
            }
        }

        let output = guard.take().expect("checked");

        self.event.notify_all();
        MutexGuard::unlock_fair(guard);

        output.and_then_desktop(|img| encode_img(img.as_data_ref(), self.yuv444))
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
