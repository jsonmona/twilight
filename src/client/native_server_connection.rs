use std::str::FromStr;
use std::{fmt::Debug, sync::Arc};

use anyhow::Result;
use bytes::Bytes;
use fastwebsockets::{handshake, FragmentCollectorRead, Frame, OpCode, Payload};
use futures_util::Future;
use http_body_util::Empty;
use hyper::{header, Method, Request, StatusCode};
use parking_lot::RwLock;
use rustc_hash::FxHashMap;
use tokio::net::TcpStream;
use tokio::sync::mpsc::error::TrySendError;

use crate::client::server_connection::{FetchResponse, MessageRead, Origin, ServerConnection};

use super::server_connection::MessageWrite;

#[derive(Debug)]
pub struct NativeServerConnection {
    origin: Origin,
    auth: Option<String>,
    client: reqwest::Client,
    stream_read: Arc<RwLock<FxHashMap<u16, tokio::sync::mpsc::Sender<Bytes>>>>,
    stream_write: Option<Arc<tokio::sync::mpsc::Sender<Frame<'static>>>>,
}

impl NativeServerConnection {
    pub async fn new(origin: Origin) -> Result<Self> {
        let client = reqwest::Client::builder();

        Ok(NativeServerConnection {
            origin,
            auth: Default::default(),
            client: client.build()?,
            stream_read: Arc::new(RwLock::new(Default::default())),
            stream_write: None,
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
    type MessageReadImpl = NativeMessageRead;
    type MessageWriteImpl = NativeMessageWrite;

    async fn close(self) {}

    fn origin(&self) -> &Origin {
        &self.origin
    }

    fn set_auth(&mut self, token: String) {
        self.auth = Some(token);
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

        let builder = self
            .client
            .request(method, url)
            .body(data)
            .header(reqwest::header::CONTENT_TYPE, "application/json");

        // attach bearer token if available
        let builder = if let Some(bearer) = self.auth.as_ref() {
            builder.bearer_auth(bearer)
        } else {
            builder
        };

        Ok(NativeFetchResponse(builder.send().await?))
    }

    async fn stream_read(&mut self, channel: u16) -> Result<NativeMessageRead> {
        if self.stream_write.is_none() {
            self.open_conn().await?;
        }

        //TODO: Vary bound by channel type
        let (tx, rx) = tokio::sync::mpsc::channel(64);

        let mut target = self.stream_read.write();
        target.insert(channel, tx);
        std::mem::drop(target);

        Ok(NativeMessageRead {
            ch: channel,
            is_open: true,
            stream: rx,
        })
    }

    async fn stream_write(&mut self, channel: u16) -> Result<NativeMessageWrite> {
        if self.stream_write.is_none() {
            self.open_conn().await?;
        }

        let stream = Arc::clone(self.stream_write.as_ref().expect("created above"));

        Ok(NativeMessageWrite {
            ch: channel,
            stream,
        })
    }
}

impl NativeServerConnection {
    async fn open_conn(&mut self) -> Result<()> {
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

        let (mut ws, _) = handshake::client(&SpawnExecutor, req, stream).await?;

        // Copied from https://github.com/denoland/fastwebsockets/issues/76
        ws.set_auto_pong(false);
        ws.set_auto_close(false);

        let (rx, mut tx) = ws.split(tokio::io::split);
        let mut rx = FragmentCollectorRead::new(rx);

        let (msg_send_tx, mut msg_send_rx) = tokio::sync::mpsc::channel(16);
        let msg_send_tx = Arc::new(msg_send_tx);

        self.stream_write = Some(Arc::clone(&msg_send_tx));

        let receiver_inner = Arc::clone(&self.stream_read);

        tokio::task::spawn(async move {
            loop {
                let frame = match rx.read_frame(&mut |f| msg_send_tx.send(f)).await {
                    Ok(x) => x,
                    Err(e) => {
                        panic!("{:?}", e);
                    }
                };

                match frame.opcode {
                    OpCode::Ping => {
                        msg_send_tx.send(Frame::pong(frame.payload)).await.unwrap();
                    }
                    OpCode::Binary => {
                        if frame.payload.len() < 2 {
                            log::warn!("Ignoring too short message (len={})", frame.payload.len());
                            continue;
                        }

                        let ch = u16::from_le_bytes(
                            frame.payload[..2].try_into().expect("checked above"),
                        );

                        let payload = match frame.payload {
                            Payload::BorrowedMut(x) => Bytes::from(&x[2..]),
                            Payload::Borrowed(x) => Bytes::from(&x[2..]),
                            Payload::Owned(x) => Bytes::from(x).slice(2..),
                            Payload::Bytes(x) => Bytes::from(x).slice(2..),
                        };

                        let target = receiver_inner.read();
                        let tx = match target.get(&ch) {
                            Some(x) => x,
                            None => {
                                log::warn!("Ignoring message for non-existing channel {ch}");
                                continue;
                            }
                        };

                        if let Err(e) = tx.try_send(payload) {
                            if let TrySendError::Full(_) = e {
                                log::error!("Removing channel {ch} because buffer is full");
                            }

                            // Receiver is dead or unresponsive. Remove the channel.
                            std::mem::drop(target);
                            receiver_inner.write().remove(&ch);
                        }
                    }
                    OpCode::Close => {
                        panic!("STUB: needs to close connection");
                    }
                    _ => { /* ignore */ }
                }
            }
        });

        tokio::task::spawn(async move {
            while let Some(frame) = msg_send_rx.recv().await {
                tx.write_frame(frame).await.unwrap();
            }
        });

        Ok(())
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

pub struct NativeMessageRead {
    ch: u16,
    is_open: bool,
    stream: tokio::sync::mpsc::Receiver<Bytes>,
}

impl MessageRead for NativeMessageRead {
    fn is_open(&self) -> bool {
        self.is_open
    }

    fn channel(&self) -> u16 {
        self.ch
    }

    async fn read(&mut self) -> Result<Option<Bytes>> {
        let data = self.stream.recv().await;
        if data.is_none() {
            self.is_open = false;
        }

        Ok(data)
    }
}

impl Debug for NativeMessageRead {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("NativeMessageRead").finish()
    }
}

pub struct NativeMessageWrite {
    ch: u16,
    stream: Arc<tokio::sync::mpsc::Sender<Frame<'static>>>,
}

impl MessageWrite for NativeMessageWrite {
    fn is_open(&self) -> bool {
        !self.stream.is_closed()
    }

    fn channel(&self) -> u16 {
        self.ch
    }

    async fn write(&mut self, data: Bytes) -> Result<()> {
        let mut buf = Vec::with_capacity(2 + data.len());
        buf.extend_from_slice(&self.ch.to_le_bytes());
        buf.extend_from_slice(&data);

        self.stream.send(Frame::binary(Payload::Owned(buf))).await?;

        Ok(())
    }
}

impl Debug for NativeMessageWrite {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("NativeMessageWrite").finish()
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
