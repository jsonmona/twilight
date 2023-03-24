use crate::network::util::send_msg_with;
use crate::schema::video::{
    Coord2f, Coord2u, CursorShape, CursorShapeArgs, CursorUpdate, CursorUpdateArgs,
    NotifyVideoStart, NotifyVideoStartArgs, Size2u, VideoCodec, VideoFrame, VideoFrameArgs,
};
use crate::server::new_capture_pipeline;
use crate::server::session_id::SessionId;
use crate::util::{try_block_in_place, DesktopUpdate};
use crate::video::encoder::jpeg::JpegEncoder;
use crate::video::encoder::EncoderStage;
use anyhow::{bail, Context, Result};
use cookie::{Cookie, SameSite};
use flatbuffers::FlatBufferBuilder;
use futures_util::stream::StreamExt;
use futures_util::SinkExt;
use hyper::body::HttpBody;
use hyper::http::uri::Scheme;
use hyper::http::HeaderValue;
use hyper::server::conn::Http;
use hyper::service::service_fn;
use hyper::{header, Body, Method, Request, Response, StatusCode};
use hyper_tungstenite::tungstenite::Message;
use hyper_tungstenite::HyperWebsocket;
use lazy_static::lazy_static;
use rand::prelude::*;
use regex::Regex;
use rustc_hash::FxHashMap;
use std::collections::hash_map::Entry;
use std::convert::Infallible;
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

pub struct TwilightServer {
    random: parking_lot::Mutex<Option<StdRng>>,
    looper: tokio::sync::Mutex<Option<JoinHandle<Result<()>>>>,
    sessions: Mutex<FxHashMap<SessionId, Arc<Session>>>,
}

impl TwilightServer {
    pub fn new() -> Result<Arc<Self>> {
        Ok(Arc::new(TwilightServer {
            random: parking_lot::Mutex::new(None),
            looper: tokio::sync::Mutex::new(None),
            sessions: Mutex::new(Default::default()),
        }))
    }

    pub async fn add_conn<RW>(self: &Arc<Self>, stream: RW) -> Result<()>
    where
        RW: AsyncRead + AsyncWrite + Send + Unpin + 'static,
    {
        let this = Arc::clone(self);

        let _worker = tokio::task::spawn(async move {
            Http::new()
                .http2_enable_connect_protocol()
                .serve_connection(stream, service_fn(move |x| service(x, Arc::clone(&this))))
                .with_upgrades()
                .await
        });

        Ok(())
    }

    pub async fn is_running(&self) -> bool {
        false
    }

    pub async fn close(self) -> Result<()> {
        todo!()
    }

    pub async fn join_all(&self) {
        let mut guard = self.looper.lock().await;
        if let Some(x) = guard.as_mut() {
            x.await.unwrap().unwrap();
            guard.take();
        }
    }
}

impl TwilightServer {
    fn random(&self) -> parking_lot::MappedMutexGuard<StdRng> {
        let mut mutex = self.random.lock();
        if mutex.is_none() {
            *mutex = Some(StdRng::from_entropy());
        }
        parking_lot::MutexGuard::map(mutex, |x| {
            x.as_mut()
                .expect("safe because the value was just initialized")
        })
    }

    fn assign_session(&self, session: Arc<Session>) -> SessionId {
        let mut sessions = self.sessions.lock().unwrap();
        loop {
            let session_id = SessionId::from_random(&mut *self.random());
            match sessions.entry(session_id.clone()) {
                Entry::Occupied(_) => continue,
                Entry::Vacant(x) => {
                    x.insert(session);
                    return session_id;
                }
            }
        }
    }
}

async fn service(
    req: Request<Body>,
    server: Arc<TwilightServer>,
) -> Result<Response<Body>, Infallible> {
    let prefix = "";

    let path = match req.uri().path().strip_prefix(prefix) {
        Some(x) => x,
        None => return Ok(handle_error(StatusCode::NOT_FOUND)),
    };

    // remove leading slashes
    let path = path.trim_start_matches('/');

    Ok(match path {
        "auth" => handle_auth(req, server).await,
        "stream" => handle_stream(req, server).await,
        _ => handle_error(StatusCode::NOT_FOUND),
    })
}

struct Session {
    username: String,
}

fn make_set_cookie_session(session_id: &SessionId, using_https: bool) -> HeaderValue {
    let session_str = session_id.to_hex();

    let set_cookie = Cookie::build("session", session_str)
        .http_only(true)
        .expires(None)
        .same_site(SameSite::Strict)
        .secure(using_https)
        .finish()
        .to_string();

    HeaderValue::from_str(&set_cookie)
        .expect("set-cookie directive for session contains non-ascii character")
}

async fn read_body_with_maximum(body: &mut Body, max_len: usize) -> Option<Vec<u8>> {
    let size_hint = body.size_hint().lower().try_into().ok()?;

    if max_len < size_hint {
        return None;
    }

    let mut buf = Vec::with_capacity(size_hint);

    while let Some(segment) = body.data().await.and_then(|x| x.ok()) {
        if max_len < buf.len() + segment.len() {
            return None;
        }

        buf.extend_from_slice(&segment);
    }

    Some(buf)
}

fn handle_error(code: StatusCode) -> Response<Body> {
    let mut res = Response::new(Body::empty());
    *res.status_mut() = code;
    res
}

async fn handle_auth(req: Request<Body>, server: Arc<TwilightServer>) -> Response<Body> {
    let query = req.uri().query().unwrap_or("");

    let mut types = query.split('&').filter_map(|seg| {
        let (k, v) = seg.split_once('=')?;
        if k == "type" {
            Some(v)
        } else {
            None
        }
    });

    let ty = types.next();
    if ty.is_none() || types.next().is_some() {
        //TODO: Add error message saying type query was missing
        return handle_error(StatusCode::BAD_REQUEST);
    }

    let ty = ty.expect("already checked");

    match ty {
        "username" => handle_auth_username(req, server).await,
        _invalid => {
            //TODO: Add error message saying type is unknown
            handle_error(StatusCode::BAD_REQUEST)
        }
    }
}

async fn handle_auth_username(req: Request<Body>, server: Arc<TwilightServer>) -> Response<Body> {
    const MAX_LEN: usize = 256;

    if *req.method() != Method::POST {
        return handle_error(StatusCode::METHOD_NOT_ALLOWED);
    }

    let using_https = req
        .uri()
        .scheme()
        .map(|x| *x == Scheme::HTTPS)
        .unwrap_or(false);

    let mut body = req.into_body();

    let username = match read_body_with_maximum(&mut body, MAX_LEN).await {
        Some(x) => x,
        None => return handle_error(StatusCode::PAYLOAD_TOO_LARGE),
    };

    if username.is_empty() {
        return handle_error(StatusCode::BAD_REQUEST);
    }

    let username = match String::from_utf8(username) {
        Ok(x) => x,
        Err(_) => return handle_error(StatusCode::BAD_REQUEST),
    };

    lazy_static! {
        static ref USERNAME_REGEX: Regex = Regex::new("^[A-Za-z0-9]+$").unwrap();
    }

    if !USERNAME_REGEX.is_match(&username) {
        return handle_error(StatusCode::BAD_REQUEST);
    }

    let session = Arc::new(Session { username });
    let session_id = server.assign_session(session);
    let set_session = make_set_cookie_session(&session_id, using_https);

    let mut res = Response::new(Body::empty());
    *res.status_mut() = StatusCode::OK;
    res.headers_mut().insert(header::SET_COOKIE, set_session);
    res
}

async fn handle_stream(mut req: Request<Body>, server: Arc<TwilightServer>) -> Response<Body> {
    if *req.method() != Method::GET {
        return handle_error(StatusCode::METHOD_NOT_ALLOWED);
    }

    if !hyper_tungstenite::is_upgrade_request(&req) {
        return handle_error(StatusCode::BAD_REQUEST);
    }

    let resolution;
    let (tx, rx) = mpsc::channel(1);

    // initialize looper
    {
        let mut looper = server.looper.lock().await;
        if let Some(handle) = looper.as_mut() {
            if !handle.is_finished() {
                //TODO: Allow multiple connections
                let mut res = Response::new(Body::from("client already connected"));
                *res.status_mut() = StatusCode::INTERNAL_SERVER_ERROR;
                return res;
            } else {
                looper
                    .take()
                    .expect("checked above")
                    .await
                    .unwrap()
                    .unwrap();
            }
        }

        let mut pipeline = try_block_in_place(new_capture_pipeline)
            .await
            .unwrap()
            .unwrap();
        resolution = pipeline.resolution();

        let mut encoder = JpegEncoder::new(resolution.0, resolution.1, true).unwrap();

        *looper = Some(tokio::task::spawn(async move {
            while let Some(reader) = pipeline.reader() {
                match reader.recv().await {
                    None => break,
                    Some(update) => {
                        let (update, desktop) = update.split();
                        let desktop = encoder.encode(desktop).unwrap();
                        let update = update.with_desktop(desktop);
                        if tx.send(update).await.is_err() {
                            break;
                        }
                    }
                }
            }
            pipeline.close();
            Ok(())
        }));
    }

    //TODO: Verify origin header
    let (response, websocket) = match hyper_tungstenite::upgrade(&mut req, None) {
        Ok(x) => x,
        Err(e) => {
            eprintln!("{e}");
            return handle_error(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };

    tokio::task::spawn(async move {
        websocket_io(websocket, server, resolution, rx)
            .await
            .unwrap();
    });

    response
}

async fn websocket_io(
    sock: HyperWebsocket,
    _server: Arc<TwilightServer>,
    resolution: (u32, u32),
    mut rx: mpsc::Receiver<DesktopUpdate<Vec<u8>>>,
) -> Result<()> {
    let sock = sock.await?;
    let (mut writer, mut reader) = sock.split();

    let (w, h) = resolution;

    let receiver = tokio::task::spawn(async move {
        while let Some(msg) = reader.next().await {
            let msg = match msg {
                Ok(x) => x,
                Err(e) => match e {
                    tungstenite::Error::ConnectionClosed => break,
                    tungstenite::Error::AlreadyClosed => bail!("tried to read after close"),
                    _ => bail!(e),
                },
            };

            match msg {
                Message::Binary(msg) => {
                    println!("Received binary message {msg:?}");
                }
                Message::Pong(msg) => {
                    println!("Received pong message {msg:?}");
                }
                Message::Ping(_) => { /* handled automatically; ignore */ }
                Message::Text(_) => bail!("received text message"),
                Message::Close(_) => break,
                Message::Frame(_) => unreachable!(),
            }
        }
        anyhow::Ok(())
    });

    let sender = tokio::task::spawn(async move {
        let mut builder = FlatBufferBuilder::with_capacity(8192);

        send_msg_with(&mut writer, &mut builder, |builder| {
            NotifyVideoStart::create(
                builder,
                &NotifyVideoStartArgs {
                    resolution: Some(&Size2u::new(w, h)),
                    desktop_codec: VideoCodec::Jpeg,
                },
            )
        })
        .await?;
        writer.flush().await?;

        loop {
            let update = rx.recv().await.context("image capture stopped")?;
            let desktop = update.desktop;
            let cursor = update.cursor;

            let video_bytes = desktop.len().try_into()?;
            send_msg_with(&mut writer, &mut builder, |builder| {
                let cursor_update = cursor.map(|cursor_state| {
                    let shape = cursor_state.shape.map(|cursor_shape| {
                        let image = Some(builder.create_vector(&cursor_shape.image.data));

                        CursorShape::create(
                            builder,
                            &CursorShapeArgs {
                                image,
                                codec: VideoCodec::Bgra8888,
                                xor: false,
                                hotspot: Some(&Coord2f::new(0.0, 0.0)),
                                resolution: Some(&Size2u::new(
                                    cursor_shape.image.width,
                                    cursor_shape.image.height,
                                )),
                            },
                        )
                    });

                    CursorUpdate::create(
                        builder,
                        &CursorUpdateArgs {
                            shape,
                            pos: Some(&Coord2u::new(cursor_state.pos_x, cursor_state.pos_y)),
                            visible: cursor_state.visible,
                        },
                    )
                });

                VideoFrame::create(
                    builder,
                    &VideoFrameArgs {
                        video_bytes,
                        cursor_update,
                    },
                )
            })
            .await?;
            writer.feed(Message::Binary(desktop)).await?;
            writer.flush().await?;
        }

        anyhow::Ok(())
    });

    let (sender, receiver) = tokio::join!(sender, receiver);
    sender??;
    receiver??;
    Ok(())
}
