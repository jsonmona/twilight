use std::time::Duration;

use crate::client::server_connection::{
    FetchResponse, MessageSink, MessageStream, ServerConnection,
};
use anyhow::{ensure, Result};
use async_trait::async_trait;
use futures_util::stream::{SplitSink, SplitStream};
use futures_util::{SinkExt, StreamExt};
use http_body_util::{BodyExt, Full};
use hyper::body::{Bytes, Incoming};
use hyper::header::{CONNECTION, HOST, UPGRADE};
use hyper::upgrade::Upgraded;
use hyper::{Method, Request, StatusCode};
use hyper_util::client::legacy::connect::HttpConnector;
use hyper_util::client::legacy::Client;
use hyper_util::rt::{TokioExecutor, TokioIo};
use log::error;
use tokio::net::TcpStream;
use tokio::task::JoinHandle;
use tokio_tungstenite::WebSocketStream;
use tungstenite::protocol::Role;
use tungstenite::Message;

pub struct NativeServerConnection {
    client: Client<HttpConnector, Full<Bytes>>,
    host: (String, u16),
}

impl NativeServerConnection {
    pub async fn new(host: &str, port: u16) -> Result<Self> {
        let client = hyper_util::client::legacy::Builder::new(TokioExecutor::new())
            .pool_idle_timeout(Duration::from_secs(30))
            .build_http();

        Ok(NativeServerConnection {
            client,
            host: (host.into(), port),
        })
    }

    pub async fn close(self) -> Result<()> {
        Ok(())
    }
}

#[async_trait]
impl ServerConnection for NativeServerConnection {
    type FetchResponseImpl = NativeFetchResponse;
    type MessageSinkImpl = NativeMessageSink;
    type MessageStreamImpl = NativeMessageStream;

    fn host(&self) -> &str {
        "debug.test"
    }

    async fn fetch(
        &mut self,
        method: Method,
        path: &str,
        data: &[u8],
    ) -> Result<NativeFetchResponse> {
        let uri = format!("http://{}:{}{path}", self.host.0, self.host.1);

        let req = hyper::Request::builder()
            .method(method)
            .uri(&uri)
            .header(HOST, "debug.test")
            .body(Vec::from(data).into())?;

        let res = self.client.request(req).await?;

        Ok(NativeFetchResponse(res))
    }

    async fn upgrade(mut self, version: i32) -> Result<(NativeMessageSink, NativeMessageStream)> {
        let key = tungstenite::handshake::client::generate_key();
        let accept = tungstenite::handshake::derive_accept_key(key.as_bytes());

        assert_eq!(version, 1);

        let uri = format!("http://{}:{}/stream?version=1", self.host.0, self.host.1);

        let req = Request::builder()
            .method(Method::GET)
            .uri(&uri)
            .header(HOST, "debug.test")
            .header(CONNECTION, "Upgrade")
            .header(UPGRADE, "websocket")
            .header("sec-websocket-version", "13")
            .header("sec-websocket-key", &key)
            .body(Default::default())?;

        let res = self.client.request(req).await?;
        ensure!(
            res.status() == StatusCode::SWITCHING_PROTOCOLS,
            "Not upgrading websocket"
        );

        let accept_key = res
            .headers()
            .get("sec-websocket-accept")
            .and_then(|x| x.to_str().ok());
        ensure!(accept_key == Some(&accept), "Invalid websocket accept key");

        let upgraded = hyper::upgrade::on(res).await?;
        let upgraded = upgraded
            .downcast::<TokioIo<TcpStream>>()
            .unwrap()
            .io
            .into_inner();
        //FIXME: Do not ignore read_buf
        let stream = WebSocketStream::from_raw_socket(upgraded, Role::Client, None).await;

        let (tx, rx) = stream.split();
        Ok((NativeMessageSink(tx), NativeMessageStream(rx)))
    }
}

pub struct NativeFetchResponse(hyper::Response<Incoming>);

#[async_trait]
impl FetchResponse for NativeFetchResponse {
    fn status(&self) -> StatusCode {
        self.0.status()
    }

    async fn next(&mut self) -> Option<Result<Bytes>> {
        self.0.body_mut().frame().await.and_then(|x| match x {
            Ok(frame) => frame.data_ref().map(|b| Ok(b.clone())),
            Err(e) => Some(Err(e.into())),
        })
    }
}

pub struct NativeMessageSink(SplitSink<WebSocketStream<TcpStream>, Message>);

#[async_trait]
impl MessageSink for NativeMessageSink {
    async fn send(&mut self, data: Bytes) -> Result<()> {
        Ok(self.0.send(Message::Binary(data.into())).await?)
    }
}

pub struct NativeMessageStream(SplitStream<WebSocketStream<TcpStream>>);

#[async_trait]
impl MessageStream for NativeMessageStream {
    async fn recv(&mut self) -> Option<Result<Bytes>> {
        loop {
            match self.0.next().await {
                Some(x) => match x {
                    Ok(msg) => {
                        if let Message::Binary(data) = msg {
                            return Some(Ok(data.into()));
                        } else {
                            error!("Received message of wrong type: {msg:?}");
                        }
                    }
                    Err(e) => return Some(Err(e.into())),
                },
                None => return None,
            }
        }
    }
}
