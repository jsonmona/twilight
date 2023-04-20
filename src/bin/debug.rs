use tokio::runtime::Runtime;
use tokio::task::LocalSet;
use twilight::client::ClientLaunchArgs;

fn main() {
    twilight::platform::win32::init_dpi();
    env_logger::init();

    let runtime = Runtime::new().expect("starting tokio runtime");
    let rt = runtime.handle().clone();

    std::thread::spawn(move || {
        let local = LocalSet::new();

        local.spawn_local(async { twilight::server::serve().await.expect("launching server") });
        rt.block_on(local);
    });

    let rt = runtime.handle().clone();

    twilight::viewer::launch(
        rt,
        ClientLaunchArgs {
            host: "127.0.0.1".into(),
            port: None,
            cleartext: true,
        },
    );
}
