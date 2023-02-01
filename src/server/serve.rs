use crate::platform::win32::capture_gdi::CaptureGdi;
use std::time::{Duration, Instant};
use tokio::io::AsyncWriteExt;

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
    let mut capture = CaptureGdi::new().unwrap();
    let listener =
        tokio::net::TcpListener::bind((std::net::Ipv4Addr::new(127, 0, 0, 1), 6495)).await?;

    let (mut stream, client_addr) = listener.accept().await?;
    println!("Connected to {client_addr}");

    stream.set_nodelay(true)?;

    let (w, h) = capture.resolution();
    stream.write_all(&w.to_le_bytes()).await?;
    stream.write_all(&h.to_le_bytes()).await?;

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
