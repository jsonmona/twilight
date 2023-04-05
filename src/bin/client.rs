use clap::Parser;
use tokio::runtime::Runtime;
use twilight::client::ClientLaunchArgs;

fn main() {
    env_logger::init();

    let args = ClientLaunchArgs::parse();

    let runtime = Runtime::new().expect("starting tokio runtime");
    let rt = runtime.handle().clone();

    twilight::viewer::launch(rt, args);
}
