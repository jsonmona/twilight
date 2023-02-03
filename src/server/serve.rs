use crate::network::util::send_msg_with;
use crate::platform::win32::capture_gdi::CaptureGdi;
use crate::schema::video::{NotifyVideoStart, NotifyVideoStartArgs, Size2u, VideoFrame, VideoFrameArgs};
use flatbuffers::FlatBufferBuilder;
use std::time::{Duration, Instant};
use tokio::io::{AsyncWriteExt, BufStream};

pub fn serve() -> anyhow::Result<()> {
    let rt_local = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let _guard = rt_local.enter();

    let localset = tokio::task::LocalSet::new();
    localset.block_on(&rt_local, serve_inner())
}

async fn serve_inner() -> anyhow::Result<()> {
    let mut builder = FlatBufferBuilder::with_capacity(2 * 1024 * 1024);

    let mut capture = CaptureGdi::new().unwrap();
    let listener =
        tokio::net::TcpListener::bind((std::net::Ipv4Addr::new(127, 0, 0, 1), 6495)).await?;

    let (stream, client_addr) = listener.accept().await?;
    println!("Connected to {client_addr}");

    stream.set_nodelay(true)?;
    let mut stream = BufStream::new(stream);

    let (w, h) = capture.resolution();
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

    let mut log_time = Instant::now();
    let mut frames = 0;
    let mut old_time = Instant::now();
    let mut capture_duration = Duration::from_secs(0);
    let mut write_duration = Duration::from_secs(0);

    loop {
        let img = capture.capture()?;
        let capture_time = Instant::now();

        anyhow::ensure!(img.width == w);
        anyhow::ensure!(img.height == h);

        let video_bytes = img.data.len().try_into()?;
        send_msg_with(&mut stream, &mut builder, |builder| {
            VideoFrame::create(builder, &VideoFrameArgs {
                video_bytes,
                cursor_update: None,
            })
        }).await?;
        stream.write_all(img.data).await?;
        let write_time = Instant::now();

        capture_duration += capture_time - old_time;
        write_duration += write_time - capture_time;
        old_time = write_time;
        frames += 1;

        let elapsed = write_time - log_time;
        if elapsed > Duration::from_secs(10) {
            log_time = old_time;
            let fps = (frames as f64) / elapsed.as_secs_f64();
            let capture_fps = (frames as f64) / capture_duration.as_secs_f64();
            println!("Combined FPS: {fps}    Capture FPS: {capture_fps}");

            frames = 0;
            capture_duration = Duration::from_secs(0);
            write_duration = Duration::from_secs(0);
        }
    }
}
