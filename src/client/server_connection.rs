use anyhow::Result;
use async_trait::async_trait;
use hyper::body::Bytes;
use hyper::{Method, StatusCode};

//TODO: Remove these async_trait when async_fn_in_traits stabilizes

#[async_trait]
pub trait ServerConnection: Send {
    type FetchResponseImpl: FetchResponse;
    type MessageSinkImpl: MessageSink;
    type MessageStreamImpl: MessageStream;

    /// Target host. For example, "http://example.com" or "https://example.com:443" or even
    /// "http+unix:///var/run/twilight.sock"
    fn host(&self) -> &str;

    /// Fetch the given path.
    /// Caller must ensure compliance with the fetch API limitations of web browsers.
    async fn fetch(
        &mut self,
        method: Method,
        path: &str,
        data: &[u8],
    ) -> Result<Self::FetchResponseImpl>;

    /// Upgrade into websocket API.
    async fn upgrade(
        self,
        version: i32,
    ) -> Result<(Self::MessageSinkImpl, Self::MessageStreamImpl)>;
}

#[async_trait]
pub trait FetchResponse: Send {
    /// Get status code of the response
    fn status(&self) -> StatusCode;

    /// Read next bytes. None if eof.
    async fn next(&mut self) -> Option<Result<Bytes>>;
}

#[async_trait]
pub trait MessageSink: Send {
    /// Send message via websocket
    async fn send(&mut self, data: Bytes) -> Result<()>;
}

#[async_trait]
pub trait MessageStream: Send {
    /// Receive message via websocket
    async fn recv(&mut self) -> Option<Result<Bytes>>;
}
