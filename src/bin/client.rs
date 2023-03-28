use tokio::runtime::Runtime;

fn main() {
    env_logger::init();

    let runtime = Runtime::new().expect("starting tokio runtime");
    let rt = runtime.handle().clone();

    twilight::viewer::launch(
        rt,
        "127.0.0.1".parse().expect("valid localhost address"),
        6497,
    );
}
