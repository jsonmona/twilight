use tokio::task::LocalSet;

#[tokio::main]
async fn main() {
    twilight::platform::win32::init_dpi();
    env_logger::init();

    // Intentionally small buffer so that we test partial writes
    let (tx, rx) = tokio::io::duplex(127);

    let server = tokio::spawn(twilight::server::serve_debug(tx));

    let main_thread = LocalSet::new();
    let client = main_thread.run_until(async move {
        twilight::viewer::launch_debug(rx).await;
    });
    let (_, r) = tokio::join!(client, server);
    r.unwrap().unwrap();

    main_thread.await;
}
