use anyhow::{anyhow, Context, Result};
use crate::network::util::send_msg_with;
use crate::platform::win32::capture_gdi::CaptureGdi;
use crate::schema::video::{NotifyVideoStart, NotifyVideoStartArgs, Size2u, VideoFrame, VideoFrameArgs};
use flatbuffers::FlatBufferBuilder;
use std::time::{Duration, Instant};
use tokio::io::{AsyncWriteExt, BufStream};
use tokio::sync::{mpsc, oneshot};
use crate::image::ImageBuf;

pub async fn serve() -> Result<()> {
    let mut builder = FlatBufferBuilder::with_capacity(2 * 1024 * 1024);

    let (resolution_tx, resolution) = oneshot::channel();
    let (image_tx, mut image_rx) = mpsc::channel(1);

    let _capture = tokio::task::spawn_blocking(move || -> Result<()> {
        let mut cap = CaptureGdi::new()?;
        resolution_tx.send(cap.resolution()).map_err(|_| anyhow!("capture receiver dropped"))?;

        let mut log_time = Instant::now();
        let mut accumulated_time = Duration::from_secs(0);
        let mut frames = 0;

        loop {
            let old_time = Instant::now();
            let img = cap.capture()?.copy_data();
            let now_time = Instant::now();

            frames += 1;
            accumulated_time += now_time - old_time;
            if now_time - log_time > Duration::from_secs(10) {
                log_time = now_time;
                let fps = frames as f64 / accumulated_time.as_secs_f64();
                println!("Capture FPS={fps:.2}");
                frames = 0;
                accumulated_time = Duration::from_secs(0);
            }

            if let Err(_) = image_tx.blocking_send(img) {
                break;
            }
        }
        Ok(())
    });

    let (w, h) = resolution.await?;

    let listener =
        tokio::net::TcpListener::bind((std::net::Ipv4Addr::new(127, 0, 0, 1), 6495)).await?;

    let (stream, client_addr) = listener.accept().await?;
    println!("Connected to {client_addr}");

    stream.set_nodelay(true)?;
    let mut stream = BufStream::new(stream);

    send_msg_with(&mut stream, &mut builder, |builder| {
        NotifyVideoStart::create(
            builder,
            &NotifyVideoStartArgs {
                resolution: Some(&Size2u::new(w, h)),
            },
        )
    })
    .await?;
    stream.flush().await?;

    loop {
        let img = image_rx.recv().await.context("iamge capture stopped")?;

        anyhow::ensure!(img.width == w);
        anyhow::ensure!(img.height == h);

        let video_bytes = img.data.len().try_into()?;
        send_msg_with(&mut stream, &mut builder, |builder| {
            VideoFrame::create(builder, &VideoFrameArgs {
                video_bytes,
                cursor_update: None,
            })
        }).await?;
        stream.write_all(&img.data).await?;
        stream.flush().await?;
    }
}
