use tokio::task::LocalSet;

#[tokio::main]
async fn main() {
    twilight::platform::win32::init_dpi();
    env_logger::init();

    let server = tokio::spawn(twilight::server::serve());

    let main_thread = LocalSet::new();
    let client = main_thread.run_until(async move {
        twilight::viewer::launch().await;
    });
    let (_, r) = tokio::join!(client, server);
    r.unwrap().unwrap();

    main_thread.await;
}
