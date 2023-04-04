use clap::Parser;
use tokio::runtime::Runtime;

/// A CLI interface to Twilight Remote Desktop. The form of arguments may
/// change at any time during the alpha version.
#[derive(Parser, Debug)]
#[command(version, about, long_about=None)]
struct CommandArgs {
    /// The hostname or IP address of the server to connect to.
    host: String,

    /// The port number to connect to (default: 6498 with TLS, 6497 with cleartext).
    port: Option<u16>,

    /// Use cleartext transport (HTTP) instead of encrypted one (HTTPS).
    #[arg(long)]
    cleartext: bool,
}

fn main() {
    env_logger::init();

    let args = CommandArgs::parse();
    if !args.cleartext {
        panic!("Only cleartext transport is supported for now");
    }

    let port = args
        .port
        .unwrap_or_else(|| if !args.cleartext { 6498 } else { 6497 });

    let runtime = Runtime::new().expect("starting tokio runtime");
    let rt = runtime.handle().clone();

    twilight::viewer::launch(rt, &args.host, port);
}
