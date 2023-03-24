use crate::client::server_connection::{
    FetchResponse, MessageSink, MessageStream, ServerConnection,
};
use anyhow::{ensure, Result};
use async_trait::async_trait;
use futures_util::stream::{SplitSink, SplitStream};
use futures_util::{SinkExt, StreamExt};
use hyper::body::{Bytes, HttpBody};
use hyper::client::conn::SendRequest;
use hyper::header::{CONNECTION, HOST, UPGRADE};

use hyper::upgrade::Upgraded;
use hyper::{Body, Method, Request, StatusCode};

use std::net::IpAddr;

use tokio::net::TcpStream;
use tokio::task::JoinHandle;
use tokio_tungstenite::WebSocketStream;
use tungstenite::protocol::Role;
use tungstenite::Message;

pub struct NativeServerConnection {
    send: SendRequest<Body>,
    conn: JoinHandle<hyper::Result<()>>,
}

impl NativeServerConnection {
    pub async fn new(ip: IpAddr, port: u16) -> Result<Self> {
        let stream = TcpStream::connect((ip, port)).await?;

        let (send, conn) = hyper::client::conn::Builder::new()
            .handshake(stream)
            .await?;

        let conn = tokio::spawn(conn);

        Ok(NativeServerConnection { send, conn })
    }

    pub async fn close(self) -> Result<()> {
        self.conn.await.unwrap()?;
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
        let req = hyper::Request::builder()
            .method(method)
            .uri(path)
            .header(HOST, "debug.test")
            .body(Vec::from(data).into())?;

        let res = self.send.send_request(req).await?;

        Ok(NativeFetchResponse(res))
    }

    async fn upgrade(mut self) -> Result<(NativeMessageSink, NativeMessageStream)> {
        let key = tungstenite::handshake::client::generate_key();
        let accept = tungstenite::handshake::derive_accept_key(key.as_bytes());

        let req = Request::builder()
            .method(Method::GET)
            .uri("/stream")
            .header(HOST, "debug.test")
            .header(CONNECTION, "Upgrade")
            .header(UPGRADE, "websocket")
            .header("sec-websocket-version", "13")
            .header("sec-websocket-key", &key)
            .body(Body::empty())?;

        let res = self.send.send_request(req).await?;
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
        let stream = WebSocketStream::from_raw_socket(upgraded, Role::Client, None).await;

        let (tx, rx) = stream.split();
        Ok((NativeMessageSink(tx), NativeMessageStream(rx)))
    }
}

pub struct NativeFetchResponse(hyper::Response<Body>);

#[async_trait]
impl FetchResponse for NativeFetchResponse {
    fn status(&self) -> StatusCode {
        self.0.status()
    }

    async fn next(&mut self) -> Option<Result<Bytes>> {
        self.0
            .body_mut()
            .data()
            .await
            .map(|x| x.map_err(|e| e.into()))
    }
}

pub struct NativeMessageSink(SplitSink<WebSocketStream<Upgraded>, Message>);

#[async_trait]
impl MessageSink for NativeMessageSink {
    async fn send(&mut self, data: Bytes) -> Result<()> {
        Ok(self.0.send(Message::Binary(data.into())).await?)
    }
}

pub struct NativeMessageStream(SplitStream<WebSocketStream<Upgraded>>);

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
                            println!("Received message of wrong type: {msg:?}");
                        }
                    }
                    Err(e) => return Some(Err(e.into())),
                },
                None => return None,
            }
        }
    }
}
