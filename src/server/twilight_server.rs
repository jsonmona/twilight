use crate::network::util::send_msg_with;
use crate::schema::video::*;
use crate::server::session_id::SessionId;
use crate::util::{DesktopUpdate, UnwrappedRefMut};
use crate::video::capture_pipeline;
use anyhow::{anyhow, bail, Context, Result};
use cookie::{Cookie, SameSite};
use flatbuffers::FlatBufferBuilder;
use futures_util::stream::StreamExt;
use futures_util::SinkExt;
use http_body_util::combinators::BoxBody;
use http_body_util::{BodyExt, Full};
use hyper::body::{Body, Bytes, Incoming};
use hyper::http::uri::Scheme;
use hyper::http::HeaderValue;
use hyper::service::service_fn;
use hyper::{header, Method, Request, Response, StatusCode};
use hyper_tungstenite::tungstenite::Message;
use hyper_tungstenite::HyperWebsocket;
use hyper_util::rt::TokioIo;
use lazy_static::lazy_static;
use log::{debug, error, info};
use rand::prelude::*;
use regex::Regex;
use rustc_hash::FxHashMap;
use std::cell::RefCell;
use std::collections::hash_map::Entry;
use std::convert::Infallible;
use std::net::SocketAddr;
use std::rc::Rc;
use std::time::Duration;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, watch};
use tokio::task::{JoinHandle, JoinSet};

// for now, stream 0 is control, stream 1 is video, stream 2 is audio

pub struct TwilightServer {
    random: RefCell<Option<StdRng>>,
    capture_worker: RefCell<Option<JoinHandle<Result<()>>>>,
    workers: RefCell<JoinSet<Result<()>>>,
    sessions: RefCell<FxHashMap<SessionId, Rc<Session>>>,
    shutdown_tx: watch::Sender<bool>,
    shutdown_rx: watch::Receiver<bool>,
}

impl TwilightServer {
    pub fn new() -> Result<Rc<Self>> {
        let (shutdown_tx, shutdown_rx) = watch::channel(false);

        Ok(Rc::new(TwilightServer {
            random: RefCell::new(None),
            capture_worker: RefCell::new(None),
            workers: RefCell::new(JoinSet::new()),
            sessions: RefCell::new(Default::default()),
            shutdown_tx,
            shutdown_rx,
        }))
    }

    pub async fn add_listener(self: &Rc<Self>, port: u16) -> Result<()> {
        let listener = TcpListener::bind(("127.0.0.1", port)).await?;

        let this = Rc::clone(self);
        let mut shutdown = self.shutdown_rx.clone();

        if *shutdown.borrow() {
            return Ok(());
        }

        self.workers.borrow_mut().spawn_local(async move {
            while !*shutdown.borrow() {
                let x = tokio::select! {
                    biased;
                    _ = shutdown.changed() => break,
                    x = listener.accept() => x,
                };

                if *shutdown.borrow() {
                    return Ok(());
                }

                let inner_this = Rc::clone(&this);
                this.workers.borrow_mut().spawn_local(async move {
                    let this = inner_this;

                    // CLion can't infer type from tokio::select!
                    let (stream, _addr): (TcpStream, SocketAddr) = x?;

                    hyper_util::server::conn::auto::Builder::new(LocalExec)
                        .serve_connection_with_upgrades(
                            TokioIo::new(stream),
                            service_fn(move |x| service(x, Rc::clone(&this))),
                        )
                        .await
                        .map_err(|e| anyhow!(e))
                });
            }

            Ok(())
        });

        Ok(())
    }

    #[allow(clippy::await_holding_refcell_ref)]
    pub async fn shutdown(&self) {
        //TODO: Come up with more clever way to do this (without that warning)
        self.shutdown_tx.send_replace(true);
        let mut workers = self.workers.borrow_mut();
        while let Some(task) = workers.join_next().await {
            let task = task.expect("task failed");
            if let Err(e) = task {
                error!("{e:?}");
            }
        }
    }

    fn random(&self) -> UnwrappedRefMut<Option<StdRng>> {
        let mut rng = self.random.borrow_mut();
        if rng.is_none() {
            *rng = Some(StdRng::from_entropy());
        }
        UnwrappedRefMut::new(rng).expect("checked above")
    }

    fn assign_session(&self, session: Rc<Session>) -> SessionId {
        let mut sessions = self.sessions.borrow_mut();
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

impl Drop for TwilightServer {
    fn drop(&mut self) {
        assert!(
            self.workers.borrow_mut().is_empty(),
            "TwilightServer dropped without shutdown"
        );
    }
}

#[derive(Clone, Copy, Debug)]
struct LocalExec;

impl<F> hyper::rt::Executor<F> for LocalExec
where
    F: std::future::Future + 'static,
{
    fn execute(&self, fut: F) {
        tokio::task::spawn_local(fut);
    }
}

async fn service(
    req: Request<Incoming>,
    server: Rc<TwilightServer>,
) -> Result<Response<Full<Bytes>>, Infallible> {
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

    let set_cookie = Cookie::build(("session", session_str))
        .http_only(true)
        .expires(None)
        .same_site(SameSite::Strict)
        .secure(using_https)
        .finish()
        .to_string();

    HeaderValue::from_str(&set_cookie)
        .expect("set-cookie directive for session contains non-ascii character")
}

async fn read_body_with_maximum(body: &mut Incoming, max_len: usize) -> Result<Option<Vec<u8>>> {
    let size_hint = match body.size_hint().lower().try_into().ok() {
        Some(x) => x,
        None => return Ok(None),
    };

    if max_len < size_hint {
        return Ok(None);
    }

    let mut buf = Vec::with_capacity(size_hint);

    while let Some(frame) = body.frame().await {
        let frame = match frame?.into_data() {
            Ok(x) => x,
            Err(_) => continue,
        };

        if max_len < buf.len() + frame.len() {
            return Ok(None);
        }

        buf.extend_from_slice(&frame);
    }

    Ok(Some(buf))
}

fn handle_error(code: StatusCode) -> Response<Full<Bytes>> {
    let mut res = Response::new(Default::default());
    *res.status_mut() = code;
    res
}

async fn handle_auth(req: Request<Incoming>, server: Rc<TwilightServer>) -> Response<Full<Bytes>> {
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

async fn handle_auth_username(
    req: Request<Incoming>,
    server: Rc<TwilightServer>,
) -> Response<Full<Bytes>> {
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

    let username = match read_body_with_maximum(&mut body, MAX_LEN).await.unwrap() {
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

    let session = Rc::new(Session { username });
    let session_id = server.assign_session(session);
    let set_session = make_set_cookie_session(&session_id, using_https);

    let mut res = Response::new(Default::default());
    *res.status_mut() = StatusCode::OK;
    res.headers_mut().insert(header::SET_COOKIE, set_session);
    res
}

async fn handle_stream(
    mut req: Request<Incoming>,
    server: Rc<TwilightServer>,
) -> Response<Full<Bytes>> {
    if *req.method() != Method::GET {
        return handle_error(StatusCode::METHOD_NOT_ALLOWED);
    }

    if !hyper_tungstenite::is_upgrade_request(&req) {
        return handle_error(StatusCode::BAD_REQUEST);
    }

    if req.uri().query().unwrap_or("") != "version=1" {
        //TODO: Need to actually parse it
        return handle_error(StatusCode::BAD_REQUEST);
    }

    // initialize capture_worker
    let prev_worker = server.capture_worker.borrow_mut().take();
    if let Some(handle) = prev_worker {
        if !handle.is_finished() {
            //TODO: Allow multiple connections
            let mut res = Response::new("client already connected".into());
            *res.status_mut() = StatusCode::INTERNAL_SERVER_ERROR;
            return res;
        } else {
            handle.await.unwrap().unwrap();
        }
    }

    //TODO: Verify origin header
    let (response, websocket) = match hyper_tungstenite::upgrade(&mut req, None) {
        Ok(x) => x,
        Err(e) => {
            error!("{}", e);
            return handle_error(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };

    let (w, h, pipeline) = capture_pipeline().unwrap();

    tokio::task::spawn_local(async move {
        let result = websocket_io(websocket, server, (w, h), pipeline).await;

        // log error if necessary
        if let Err(e) = result {
            match e.downcast::<tungstenite::Error>() {
                Ok(e) => {
                    // tungstenite error
                    match e {
                        tungstenite::Error::ConnectionClosed => {
                            info!("WebSocket closed");
                        }
                        tungstenite::Error::Io(e) => {
                            // Probably not our fault; Use short form
                            info!("WebSocket io error: {}", e);
                        }
                        _ => {
                            // Maybe our fault
                            error!("WebSocket io error: {:?}", e);
                        }
                    }
                }
                Err(e) => {
                    // non-tungstenite error
                    error!("WebSocket io error: {:?}", e);
                }
            }
        }
    });

    response
}

async fn websocket_io(
    sock: HyperWebsocket,
    server: Rc<TwilightServer>,
    resolution: (u32, u32),
    mut rx: mpsc::Receiver<DesktopUpdate<Vec<u8>>>,
) -> Result<()> {
    let sock = sock.await?;
    let (mut writer, mut reader) = sock.split();

    let (w, h) = resolution;
    let mut shutdown = server.shutdown_rx.clone();
    let shutdown2 = shutdown.clone();

    let receiver = tokio::task::spawn(async move {
        let mut shutdown = shutdown2;

        loop {
            let msg: Option<Result<Message, tungstenite::Error>> = tokio::select! {
                biased;
                _ = shutdown.changed() => break,
                x = reader.next() => x,
            };
            let msg = msg.context("image capture stopped")?;
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
                    debug!("Received binary message {msg:?}");
                }
                Message::Pong(msg) => {
                    debug!("Received pong message {msg:?}");
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

        //FIXME: Fixed stream id
        send_msg_with(0, &mut writer, &mut builder, |builder| {
            NotifyVideoStart::create(
                builder,
                &NotifyVideoStartArgs {
                    stream: 1,
                    resolution: Some(&Size2u::new(w, h)),
                    desktop_codec: VideoCodec::Jpeg,
                },
            )
        })
        .await?;
        writer.flush().await?;

        while !*shutdown.borrow() {
            let update: Option<DesktopUpdate<Vec<u8>>> = tokio::select! {
                biased;
                _ = shutdown.changed() => break,
                x = rx.recv() => x,
            };
            let update = update.context("image capture stopped")?;
            let desktop = update.desktop;
            let cursor = update.cursor;

            let video_bytes = desktop.len().try_into()?;
            //FIXME: Fixed stream id
            send_msg_with(1, &mut writer, &mut builder, |builder| {
                let cursor_update = cursor.map(|cursor_state| {
                    let shape = cursor_state.shape.map(|cursor_shape| {
                        let image = Some(builder.create_vector(&cursor_shape.image.data));

                        CursorShape::create(
                            builder,
                            &CursorShapeArgs {
                                image,
                                codec: VideoCodec::Bgra8888,
                                xor: cursor_shape.xor,
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
