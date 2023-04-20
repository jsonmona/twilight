use crate::util::NonSend;
use anyhow::{anyhow, bail, ensure, Result};
use log::{error, info};
use std::cell::RefCell;
use std::ffi::c_void;
use std::ptr::null_mut;
use std::slice;
use std::sync::mpsc;
use std::thread::JoinHandle;
use std::time::Duration;
use wave_format_extensible::WaveFormatExtensible;
use windows::Win32::Media::Audio::*;
use windows::Win32::Media::KernelStreaming::*;
use windows::Win32::Media::Multimedia::*;
use windows::Win32::System::Com::*;

pub struct CaptureWasapi {
    worker: RefCell<Option<JoinHandle<Result<()>>>>,
    queue: mpsc::Receiver<Vec<f32>>,
    _guard: NonSend,
}

impl CaptureWasapi {
    pub fn new() -> Result<Self> {
        let (tx, rx) = mpsc::sync_channel(1);

        let worker = std::thread::spawn(|| unsafe { capture_worker_wrapper(tx) });

        Ok(CaptureWasapi {
            worker: RefCell::new(Some(worker)),
            queue: rx,
            _guard: Default::default(),
        })
    }

    pub fn next(&self) -> Result<Vec<f32>> {
        match self.queue.recv() {
            Ok(x) => Ok(x),
            Err(_) => match self.worker.borrow_mut().take() {
                Some(x) => {
                    x.join().unwrap()?;
                    Err(anyhow!("audio capture worker terminated"))
                }
                None => Err(anyhow!("audio capture worker already terminated")),
            },
        }
    }
}

unsafe fn capture_worker_wrapper(tx: mpsc::SyncSender<Vec<f32>>) -> Result<()> {
    // STA thread must be used for audio: https://learn.microsoft.com/en-us/windows/win32/api/audioclient/nf-audioclient-iaudioclient-initialize#remarks
    CoInitializeEx(None, COINIT_APARTMENTTHREADED)?;

    let result = capture_worker(tx);

    CoUninitialize();

    result
}

unsafe fn capture_worker(tx: mpsc::SyncSender<Vec<f32>>) -> Result<()> {
    let enumerator: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;

    let endpoint = enumerator.GetDefaultAudioEndpoint(eRender, eConsole)?;

    let client: IAudioClient = endpoint.Activate(CLSCTX_ALL, None)?;

    //TODO: Use low latency audio whenever possible: https://learn.microsoft.com/en-us/windows-hardware/drivers/audio/low-latency-audio
    let mix_format = {
        let mix_format = client.GetMixFormat()?;
        ensure!(mix_format != null_mut());
        if (*mix_format).cbSize < 22 {
            let cb_size = (*mix_format).cbSize;
            CoTaskMemFree(Some(mix_format as *const c_void));
            bail!("GetMixFormat returned cbSize={}", cb_size);
        }

        WaveFormatExtensible::from_raw(mix_format as *mut WAVEFORMATEXTENSIBLE)
    };
    info!("Audio capture format: {:?}", mix_format);
    ensure!(
        mix_format.Format.nChannels == 2,
        "Only stereo is supported for now"
    );

    let audio_copier = audio_copier_factory(&mix_format)?;

    //FIXME: Is ceil_div required here? (e.g. 24 bits sample)
    let samples_per_frame =
        mix_format.Format.nChannels as usize * (mix_format.Format.wBitsPerSample / 8) as usize;
    info!("Audio capture samples per frame: {}", samples_per_frame);

    client.Initialize(
        AUDCLNT_SHAREMODE_SHARED,
        AUDCLNT_STREAMFLAGS_LOOPBACK,
        0,
        0,
        &mix_format.Format,
        None,
    )?;

    let buffer_size = client.GetBufferSize()?;
    info!(
        "Audio capture maximum buffer length: {} ms",
        buffer_size as f64 / mix_format.Format.nSamplesPerSec as f64 * 1000.
    );

    //TODO: check format here

    let capture: IAudioCaptureClient = client.GetService()?;
    client.Start()?;

    loop {
        let mut data = null_mut();
        let mut frames_to_read = 0;
        let mut flags = 0;
        let mut qpc_pos = 0;
        if let Err(e) = capture.GetBuffer(
            &mut data,
            &mut frames_to_read,
            &mut flags,
            None,
            Some(&mut qpc_pos),
        ) {
            error!("Stopping audio capture by error at GetBuffer: {e}");
            break;
        }

        if frames_to_read > 0 {
            println!("{data:?}");
            println!("{flags:#010x}");
            println!("{frames_to_read}");
            let data_slice =
                slice::from_raw_parts(data, frames_to_read as usize * samples_per_frame);

            //TODO: Detect AUDCLNT_BUFFERFLAGS_DATA_DISCONTINUITY and react (show a warning?)

            let audio_data = if flags & AUDCLNT_BUFFERFLAGS_SILENT.0 as u32 == 0 {
                audio_copier(data_slice)
            } else {
                // fill with silence
                let sample_cnt = frames_to_read * mix_format.Format.nChannels as u32;
                vec![0.; usize::try_from(sample_cnt)?]
            };

            if let Err(e) = capture.ReleaseBuffer(frames_to_read) {
                error!("Stopping audio capture by error at ReleaseBuffer: {e}");
                break;
            }

            if tx.send(audio_data).is_err() {
                // Terminate normally
                break;
            }
        } else {
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    client.Stop()?;

    Ok(())
}

/// Choose which audio_copier_* to use.
/// They convert bytes to `[f32]` by converting each sample individually.
///
/// The returned function is: `fn (source: &[u8]) -> Vec<f32>`.
/// Source must be properly aligned for its actual type.
fn audio_copier_factory(mix_format: &WaveFormatExtensible) -> Result<impl Fn(&[u8]) -> Vec<f32>> {
    let format = if mix_format.Format.wFormatTag != WAVE_FORMAT_EXTENSIBLE as u16 {
        mix_format.Format.wFormatTag as u32
    } else {
        match mix_format.SubFormat {
            KSDATAFORMAT_SUBTYPE_PCM => WAVE_FORMAT_PCM,
            KSDATAFORMAT_SUBTYPE_IEEE_FLOAT => WAVE_FORMAT_IEEE_FLOAT,
            x => bail!("unknown wave SubFormat: {:?}", x),
        }
    };

    // looks like f32 is the most common format

    Ok(match format {
        WAVE_FORMAT_PCM => match mix_format.Format.wBitsPerSample {
            16 => audio_copier_i16,
            x => bail!("WAVE_FORMAT_PCM with {x} bits is not supported"),
        },
        WAVE_FORMAT_IEEE_FLOAT => audio_copier_f32,
        x => bail!("unknown wave wFormatTag: {}", x),
    })
}

fn audio_copier_i16(source: &[u8]) -> Vec<f32> {
    const MULTIPLIER: f32 = 1. / -(i16::MIN as f32);

    let src: &[i16] = bytemuck::cast_slice(source);
    let mut dst = vec![0.; src.len()];

    // I trust compiler for vectorization
    for (i, sample) in src.iter().enumerate() {
        dst[i] = *sample as f32 * MULTIPLIER;
    }

    dst
}

fn audio_copier_f32(source: &[u8]) -> Vec<f32> {
    let src: &[f32] = bytemuck::cast_slice(source);
    let mut dst = vec![0.; src.len()];

    dst.copy_from_slice(src);

    dst
}

//FIXME: Too ugly structure
mod wave_format_extensible {
    use std::ffi::c_void;
    use std::fmt::{Debug, Formatter};
    use std::ops::{Deref, DerefMut};
    use windows::Win32::Media::Audio::WAVEFORMATEXTENSIBLE;
    use windows::Win32::System::Com::CoTaskMemFree;

    pub struct WaveFormatExtensible(*mut WAVEFORMATEXTENSIBLE);

    impl WaveFormatExtensible {
        /// Will panic if the pointer is not properly aligned.
        ///
        /// Safety: Pointer must be compatible with `CoTaskMemFree`.
        pub unsafe fn from_raw(ptr: *mut WAVEFORMATEXTENSIBLE) -> Self {
            assert_eq!(ptr as usize % 4, 0, "pointer must be aligned to 4 bytes");
            Self(ptr)
        }
    }

    impl Drop for WaveFormatExtensible {
        fn drop(&mut self) {
            unsafe {
                CoTaskMemFree(Some(self.0 as *const c_void));
            }
        }
    }

    impl Debug for WaveFormatExtensible {
        fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("WaveFormatExtensible")
                .field("wFormatTag", &{ self.Format.wFormatTag })
                .field("nChannels", &{ self.Format.nChannels })
                .field("nSamplesPerSec", &{ self.Format.nSamplesPerSec })
                .field("nAvgBytesPerSec", &{ self.Format.nAvgBytesPerSec })
                .field("nBlockAlign", &{ self.Format.nBlockAlign })
                .field("wBitsPerSample", &{ self.Format.wBitsPerSample })
                .field("cbSize", &{ self.Format.cbSize })
                .field("SubFormat", &{ self.SubFormat })
                .finish_non_exhaustive()
        }
    }

    impl Deref for WaveFormatExtensible {
        type Target = WAVEFORMATEXTENSIBLE;

        fn deref(&self) -> &Self::Target {
            unsafe { &*self.0 }
        }
    }

    impl DerefMut for WaveFormatExtensible {
        fn deref_mut(&mut self) -> &mut Self::Target {
            unsafe { &mut *self.0 }
        }
    }
}
