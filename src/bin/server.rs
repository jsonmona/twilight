use tokio::io::AsyncWriteExt;

fn main() {
    twilight::platform::win32::init_dpi();
    env_logger::init();

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let localset = tokio::task::LocalSet::new();
    localset.block_on(&rt, serve()).unwrap();
}

async fn serve() -> anyhow::Result<()> {
    let mut capture = twilight::platform::win32::capture_gdi::CaptureGdi::new().unwrap();
    let listener =
        tokio::net::TcpListener::bind((std::net::Ipv4Addr::new(127, 0, 0, 1), 6495)).await?;

    let (mut stream, client_addr) = listener.accept().await?;
    println!("Connected to {client_addr}");

    stream.set_nodelay(true)?;

    let (w, h) = capture.resolution();
    stream.write_all(&w.to_le_bytes()).await?;
    stream.write_all(&h.to_le_bytes()).await?;

    loop {
        let img = capture.capture()?;
        anyhow::ensure!(img.width == w);
        anyhow::ensure!(img.height == h);
        stream.write_all(img.data).await?;
    }
}
