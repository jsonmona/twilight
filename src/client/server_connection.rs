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

#[allow(unused)]
pub trait ServerConnection: Send + Debug {
    type FetchResponseImpl: FetchResponse;
    type MessageReadImpl: MessageRead;
    type MessageWriteImpl: MessageWrite;

    async fn close(self);

    /// Target origin.
    fn origin(&self) -> &Origin;

    /// Set authorization token
    fn set_auth(&mut self, token: String);

    /// Fetch the given path.
    /// Caller must ensure compliance with the fetch API limitations of web browsers.
    async fn fetch(
        &mut self,
        method: Method,
        path: &str,
        data: Bytes,
    ) -> Result<Self::FetchResponseImpl>;

    /// Connect to the streaming API (read-only)
    async fn stream_read(&mut self, channel: u16) -> Result<Self::MessageReadImpl>;

    /// Connect to the streaming API (write-only)
    async fn stream_write(&mut self, channel: u16) -> Result<Self::MessageWriteImpl>;
}

#[allow(unused)]
pub trait FetchResponse: Send + Debug {
    /// Get status code of the response
    fn status(&self) -> StatusCode;

    /// Read next bytes. None if EOF.
    async fn next(&mut self) -> Result<Option<Bytes>>;

    /// Read all bytes. (maximum: 64MiB)
    async fn body(self) -> Result<Bytes>;
}

#[allow(unused)]
pub trait MessageRead: Send + Sync + Debug + 'static {
    fn is_open(&self) -> bool;

    fn channel(&self) -> u16;

    /// Receive message via websocket
    async fn read(&mut self) -> Result<Option<Bytes>>;
}

#[allow(unused)]
pub trait MessageWrite: Send + Sync + Debug + 'static {
    fn is_open(&self) -> bool;

    fn channel(&self) -> u16;

    /// Send message via websocket
    async fn write(&mut self, data: Bytes) -> Result<()>;
}
