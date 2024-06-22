use crate::client::native_server_connection::NativeServerConnection;
use crate::client::server_connection::{FetchResponse, MessageRead, ServerConnection};
use crate::client::ClientLaunchArgs;
use crate::image::{ColorFormat, ImageBuf};
use crate::network::dto::auth::AuthSuccessResponse;
use crate::network::dto::channel::OpenChannelResponse;
use crate::network::dto::video::{DesktopInfo, MonitorInfo, StartCapture};
use crate::schema::video::{Coord2f, Coord2u, VideoCodec, VideoFrame};
use crate::schema::{parse_msg, parse_msg_payload};
use crate::util::ThreadManager;
use crate::util::{CursorShape, CursorState, DesktopUpdate};
use crate::video::decoder::jpeg::JpegDecoder;
use crate::video::decoder::DecoderStage;
use anyhow::{anyhow, Result};
use hyper::body::Bytes;
use hyper::Method;
use std::rc::Rc;
use tokio::sync::{mpsc, watch};
use tokio::task::JoinHandle;

#[derive(Debug)]
pub enum TwilightClientEvent {
    Connected(MonitorInfo),
    NextFrame(DesktopUpdate<ImageBuf>),
    Closed(Result<()>),
}

type EventCb = Rc<dyn Fn(TwilightClientEvent)>;

/// Represents connection to a single server.
pub struct TwilightClient {
    shutdown: watch::Sender<bool>,
    worker: JoinHandle<()>,
}

impl TwilightClient {
    pub fn new(callback: EventCb, args: ClientLaunchArgs) -> Self {
        let (tx, rx) = watch::channel(false);

        if !args.url.cleartext {
            panic!("Only cleartext transport is supported for now");
        }
        let origin = args.url.clone();

        let worker = tokio::task::spawn_local(async move {
            let result = match NativeServerConnection::new(origin).await {
                Ok(c) => worker(c, rx, Rc::clone(&callback)).await,
                Err(e) => Err(e),
            };
            callback(TwilightClientEvent::Closed(result));
        });

        TwilightClient {
            shutdown: tx,
            worker,
        }
    }

    pub fn close(&self) {
        self.shutdown.send_replace(true);
    }
}

fn decoder_pipeline(
    w: u32,
    h: u32,
    codec: VideoCodec,
    thread_manager: &mut ThreadManager,
) -> (
    mpsc::Sender<DesktopUpdate<Bytes>>,
    mpsc::Receiver<DesktopUpdate<ImageBuf>>,
) {
    assert_eq!(codec, VideoCodec::Jpeg);

    let (data_tx, mut data_rx) = mpsc::channel::<DesktopUpdate<Bytes>>(1);
    let (img_tx, img_rx) = mpsc::channel(1);

    thread_manager
        .spawn_named("decoder_pipeline", move || {
            let mut decoder = JpegDecoder::new(w, h)?;
            while let Some(update) = data_rx.blocking_recv() {
                let update = update.and_then_desktop(|x| decoder.decode(&x))?;
                img_tx
                    .blocking_send(update)
                    .map_err(|_| anyhow!("img_rx closed"))?;
            }
            Ok(())
        })
        .unwrap();

    (data_tx, img_rx)
}

async fn worker(
    mut conn: impl ServerConnection,
    mut shutdown: watch::Receiver<bool>,
    callback: EventCb,
) -> Result<()> {
    let mut thread_manager = ThreadManager::new();

    let res = conn.fetch(Method::POST, "/auth/username", b"testuser"[..].into());

    let res = tokio::select! {
        biased;
        _ = shutdown.changed() => return Ok(()),
        x = res => x
    }?;

    if !res.status().is_success() {
        return Err(anyhow!("Auth status is not ok ({})", res.status().as_u16()));
    }

    let res = res.body().await?;
    let res: AuthSuccessResponse = serde_json::from_slice(&res)?;

    conn.set_auth(res.token);

    let res = conn.fetch(Method::GET, "/capture/desktop", Bytes::new());

    let res = tokio::select! {
        biased;
        _ = shutdown.changed() => return Ok(()),
        x = res => x
    }?;

    if !res.status().is_success() {
        return Err(anyhow!(
            "Capture list status is not ok ({})",
            res.status().as_u16()
        ));
    }

    let res = res.body().await?;
    let res: DesktopInfo = serde_json::from_slice(&res)?;

    if res.monitor.is_empty() {
        return Err(anyhow!("No monitor available in server!"));
    }

    let monitor = &res.monitor[0];
    log::info!("Connecting to monitor {:?}", monitor);

    let ch = open_channel(&mut conn).await?;
    log::info!("Using channel {ch}");
    let mut stream = conn.stream_read(ch).await?;

    let payload = serde_json::to_string(&StartCapture {
        ch,
        id: monitor.id.clone(),
    })?;

    let res = conn
        .fetch(Method::POST, "/capture/desktop", payload.into())
        .await?;

    if !res.status().is_success() {
        let status = res.status().as_u16();
        let body = res.body().await.unwrap();
        return Err(anyhow!(
            "failed to start desktop capture (status={}) {:?}",
            status,
            body
        ));
    }

    let width = monitor.resolution.width;
    let height = monitor.resolution.height;
    let desktop_codec = VideoCodec::Jpeg;
    callback(TwilightClientEvent::Connected(monitor.clone()));

    let (data_tx, mut img_rx) = decoder_pipeline(width, height, desktop_codec, &mut thread_manager);

    let callback_inner = Rc::clone(&callback);
    let decoder = tokio::task::spawn_local(async move {
        let callback = callback_inner;

        while let Some(img) = img_rx.recv().await {
            callback(TwilightClientEvent::NextFrame(img));
        }
    });

    loop {
        let msg = tokio::select! {
            biased;
            _ = shutdown.changed() => break,
            x = stream.read() => x,
        }?;

        // None => normal close
        let msg = match msg {
            Some(x) => x,
            None => break,
        };

        let frame: VideoFrame = parse_msg(&msg)?;
        let payload = parse_msg_payload(&msg);

        let update = DesktopUpdate {
            cursor: frame.cursor_update().map(|x| {
                let pos = x.pos().cloned().unwrap_or_else(|| Coord2u::new(0, 0));
                CursorState {
                    visible: x.visible(),
                    pos_x: pos.x(),
                    pos_y: pos.y(),
                    shape: x.shape().and_then(|s| {
                        let res = s.resolution()?;
                        let img = s.image()?;
                        let hotspot = s
                            .hotspot()
                            .cloned()
                            .unwrap_or_else(|| Coord2f::new(0.0, 0.0));
                        Some(CursorShape {
                            image: ImageBuf::new(
                                res.width(),
                                res.height(),
                                res.width() * 4,
                                ColorFormat::Bgra8888,
                                img.iter().collect(),
                            ),
                            xor: s.xor(),
                            hotspot_x: hotspot.x(),
                            hotspot_y: hotspot.y(),
                        })
                    }),
                }
            }),
            desktop: payload,
        };

        data_tx.send(update).await?;
    }

    drop(data_tx);
    decoder.await?;

    thread_manager.join_all();

    Ok(())
}

async fn open_channel(conn: &mut impl ServerConnection) -> Result<u16> {
    let res = conn.fetch(Method::PUT, "/channel", Bytes::new()).await?;

    if !res.status().is_success() {
        return Err(anyhow!(
            "failed to open channel (status={})",
            res.status().as_u16()
        ));
    }

    let res = res.body().await?;
    let res: OpenChannelResponse = serde_json::from_slice(&res)?;

    Ok(res.ch)
}
