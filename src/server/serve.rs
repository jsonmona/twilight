use crate::platform::win32::capture_gdi::CaptureGdi;
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

    loop {
        let img = capture.capture()?;
        anyhow::ensure!(img.width == w);
        anyhow::ensure!(img.height == h);
        stream.write_all(img.data).await?;
    }
}
