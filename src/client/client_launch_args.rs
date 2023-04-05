use clap::Parser;

const ABOUT: &str = "A CLI interface to Twilight Remote Desktop. The form of \
arguments may change at any time during the alpha version.";

/// Arguments that are local to one connection and not persisted.
#[derive(Parser, Debug)]
#[command(version, about = ABOUT, long_about = None)]
pub struct ClientLaunchArgs {
    /// The hostname or IP address of the server to connect to.
    pub host: String,

    /// The port number to connect to (default: 6498 with TLS, 6497 with cleartext).
    pub port: Option<u16>,

    /// Use cleartext transport (HTTP) instead of encrypted one (HTTPS).
    #[arg(long)]
    pub cleartext: bool,
}

impl ClientLaunchArgs {
    /// Get effective port number
    pub fn port(&self) -> u16 {
        self.port
            .unwrap_or(if !self.cleartext { 6498 } else { 6497 })
    }
}
