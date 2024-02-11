use clap::Parser;

use super::server_connection::Origin;

const ABOUT: &str = "A CLI interface to Twilight Remote Desktop. The form of \
arguments may change at any time during the alpha version.";

/// Arguments that are local to one connection and not persisted.
#[derive(Parser, Debug)]
#[command(version, about = ABOUT, long_about = None)]
pub struct ClientLaunchArgs {
    /// The URL to connect to. Example: twilight://127.0.0.1:1234/base/path
    ///
    /// If no scheme is given, it defaults to twilight.
    /// If no port is given, the default value varies by the scheme.
    /// If no base path (no slash at all), it defaults to "/twilight".
    /// End with a slash to use empty base path.
    ///
    /// Available schemes: http, https, twilight, twilightc
    ///
    /// http and twilightc uses cleartext. Default port is 80 and 1518 respectively.  
    /// https and twilight uses TLS. Default port is 443 and 1517 respectively.
    ///
    /// Current default value is for ease of debugging
    #[clap(default_value = "twilightc://localhost/twilight")]
    pub url: Origin,
}
