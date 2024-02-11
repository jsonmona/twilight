use std::fmt::Debug;
use std::str::FromStr;

use anyhow::Result;
use bytes::Bytes;
use fastwebsockets::{handshake, FragmentCollector, Frame, OpCode, WebSocketError};
use futures_util::Future;
use http_body_util::Empty;
use hyper::{header, upgrade::Upgraded, Method, Request, StatusCode};
use hyper_util::rt::TokioIo;
use tokio::net::TcpStream;

use crate::client::server_connection::{FetchResponse, MessageStream, Origin, ServerConnection};

#[derive(Debug)]
pub struct NativeServerConnection {
    origin: Origin,
    auth: Option<String>,
    client: reqwest::Client,
}

impl NativeServerConnection {
    pub async fn new(origin: Origin) -> Result<Self> {
        let client = reqwest::Client::builder();

        Ok(NativeServerConnection {
            origin,
            auth: Default::default(),
            client: client.build()?,
        })
    }

    fn get_url(&self, path: &str) -> String {
        if self.origin.path.is_empty() {
            format!("http://{}:{}{path}", self.origin.host, self.origin.port)
        } else {
            format!(
                "http://{}:{}{}{path}",
                self.origin.host, self.origin.port, self.origin.path
            )
        }
    }
}

impl ServerConnection for NativeServerConnection {
    type FetchResponseImpl = NativeFetchResponse;
    type MessageStreamImpl = NativeMessageStream;

    async fn close(self) {}

    fn origin(&self) -> &Origin {
        &self.origin
    }

    fn set_auth(&mut self, token: String) {
        self.auth = Some(token);
    }

    fn clear_auth(&mut self) {
        self.auth = None;
    }

    async fn fetch(
        &mut self,
        method: Method,
        path: &str,
        data: Bytes,
    ) -> Result<NativeFetchResponse> {
        let url = self.get_url(path);

        // FIXME: Remove this when reqwest updates to use hyper 1.0
        let method = reqwest::Method::from_str(method.as_str()).unwrap();

        let builder = self.client.request(method, url).body(data);

        // attach bearer token if available
        let builder = if let Some(bearer) = self.auth.as_ref() {
            builder.bearer_auth(bearer)
        } else {
            builder
        };

        Ok(NativeFetchResponse(builder.send().await?))
    }

    async fn stream(&mut self, version: i32) -> Result<NativeMessageStream> {
        assert_eq!(version, 1);

        let stream = TcpStream::connect((self.origin.host.as_str(), self.origin.port)).await?;

        let mut url = self.get_url("/stream/v1?auth=");
        url.push_str(self.auth.as_ref().map(|x| x.as_str()).unwrap_or(""));

        let key = handshake::generate_key();

        let req = Request::builder()
            .method(Method::GET)
            .uri(url)
            .header(header::HOST, &self.origin.host)
            .header(header::UPGRADE, "websocket")
            .header(header::CONNECTION, "upgrade")
            .header("Sec-WebSocket-Key", key)
            .header("Sec-WebSocket-Version", "13")
            .body(Empty::<Bytes>::new())?;

        let (ws, _) = handshake::client(&SpawnExecutor, req, stream).await?;
        let ws = FragmentCollector::new(ws);
        Ok(NativeMessageStream(ws.into()))
    }
}

#[derive(Debug)]
pub struct NativeFetchResponse(reqwest::Response);

impl FetchResponse for NativeFetchResponse {
    fn status(&self) -> StatusCode {
        //FIXME: Simplify this when reqwest uses hyper 1.0
        StatusCode::from_u16(self.0.status().as_u16()).unwrap()
    }

    async fn next(&mut self) -> Result<Option<Bytes>> {
        Ok(self.0.chunk().await?)
    }

    async fn body(self) -> Result<Bytes> {
        Ok(self.0.bytes().await?)
    }
}

pub struct NativeMessageStream(tokio::sync::Mutex<FragmentCollector<TokioIo<Upgraded>>>);

impl MessageStream for NativeMessageStream {
    async fn read(&self) -> Result<Option<Bytes>> {
        let mut stream = self.0.lock().await;

        loop {
            let frame = match stream.read_frame().await {
                Ok(x) => x,
                Err(e) => match e {
                    WebSocketError::ConnectionClosed => return Ok(None),
                    WebSocketError::UnexpectedEOF => return Ok(None),
                    _ => return Err(e.into()),
                },
            };

            match frame.opcode {
                OpCode::Binary => return Ok(Some(frame.payload.to_owned().into())),
                OpCode::Close => return Ok(None),
                OpCode::Ping | OpCode::Pong => {}
                OpCode::Continuation => panic!("received continuation frame"),
                unknown => {
                    log::warn!("Ignoring unknown websocket message type: {:?}", unknown);
                }
            }
        }
    }

    async fn write(&self, data: Bytes) -> Result<()> {
        let mut stream = self.0.lock().await;
        stream.write_frame(Frame::binary((*data).into())).await?;
        Ok(())
    }
}

impl Debug for NativeMessageStream {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("NativeMessageStream").finish()
    }
}

struct SpawnExecutor;

impl<Fut> hyper::rt::Executor<Fut> for SpawnExecutor
where
    Fut: Future + Send + 'static,
    Fut::Output: Send + 'static,
{
    fn execute(&self, fut: Fut) {
        tokio::task::spawn(fut);
    }
}
