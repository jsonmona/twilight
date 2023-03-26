use tokio::task::LocalSet;

#[tokio::main]
async fn main() {
    twilight::platform::win32::init_dpi();
    env_logger::init();

    let local = LocalSet::new();

    local.spawn_local(async {
        twilight::server::serve().await.expect("launching server")
    });
    local.await;
}
