use std::fmt::Debug;
use std::str::FromStr;

use anyhow::{bail, Context, Result};
use hyper::body::Bytes;
use hyper::{Method, StatusCode};
use url::Url;

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct Origin {
    pub cleartext: bool,
    pub host: String,
    pub port: u16,
    pub path: String,
}

impl FromStr for Origin {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self> {
        let url = Url::parse(s)?;

        if !url.username().is_empty() || url.password().is_some() {
            bail!("URL must not contain username or password");
        }

        if url.query().is_some() {
            bail!("URL must not contain query");
        }

        if url.fragment().is_some() {
            bail!("URL must not contain fragment");
        }

        let (cleartext, default_port) = match url.scheme() {
            "" | "twilight" => (false, 1517),
            "twilightc" => (true, 1518),
            "http" => (true, 80),
            "https" => (false, 443),
            _ => bail!("URL contains unknown scheme"),
        };

        // Perhaps we should remove default path thing
        let path = if s.ends_with("/") {
            "/"
        } else {
            let path = url.path();
            if path == "/" {
                "/twilight"
            } else {
                path
            }
        };

        Ok(Self {
            cleartext,
            host: url.host_str().context("URL must contain a host")?.into(),
            port: url.port().unwrap_or(default_port).into(),
            path: path.into(),
        })
    }
}

pub trait ServerConnection: Send + Debug {
    type FetchResponseImpl: FetchResponse;
    type MessageStreamImpl: MessageStream;

    async fn close(self);

    /// Target origin.
    fn origin(&self) -> &Origin;

    /// Set authorization token
    fn set_auth(&mut self, token: String);

    /// Clear authorization token
    fn clear_auth(&mut self);

    /// Fetch the given path.
    /// Caller must ensure compliance with the fetch API limitations of web browsers.
    async fn fetch(
        &mut self,
        method: Method,
        path: &str,
        data: Bytes,
    ) -> Result<Self::FetchResponseImpl>;

    /// Connect to the streaming API
    async fn stream(&mut self, version: i32) -> Result<Self::MessageStreamImpl>;
}

pub trait FetchResponse: Send + Debug {
    /// Get status code of the response
    fn status(&self) -> StatusCode;

    /// Read next bytes. None if EOF.
    async fn next(&mut self) -> Result<Option<Bytes>>;

    /// Read all bytes. (maximum: 64MiB)
    async fn body(self) -> Result<Bytes>;
}

pub trait MessageStream: Send + Sync + Debug {
    /// Receive message via websocket
    async fn read(&self) -> Result<Option<Bytes>>;

    /// Send message via websocket
    async fn write(&self, data: Bytes) -> Result<()>;
}
