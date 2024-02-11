use crate::client::native_server_connection::NativeServerConnection;
use crate::client::server_connection::{FetchResponse, MessageStream, ServerConnection};
use crate::client::ClientLaunchArgs;
use crate::image::{ColorFormat, ImageBuf};
use crate::network::dto::auth::AuthSuccessResponse;
use crate::network::dto::video::{DesktopInfo, MonitorInfo};
use crate::schema::video::{Coord2f, Coord2u, NotifyVideoStart, VideoCodec, VideoFrame};
use crate::util::AsUsize;
use crate::util::{CursorShape, CursorState, DesktopUpdate};
use crate::video::decoder::jpeg::JpegDecoder;
use crate::video::decoder::DecoderStage;
use anyhow::{anyhow, bail, ensure, Context, Result};
use hyper::body::Bytes;
use hyper::Method;
use std::rc::Rc;
use std::sync::Arc;
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
) -> (
    mpsc::Sender<DesktopUpdate<Bytes>>,
    mpsc::Receiver<DesktopUpdate<ImageBuf>>,
) {
    assert_eq!(codec, VideoCodec::Jpeg);

    let (data_tx, mut data_rx) = mpsc::channel::<DesktopUpdate<Bytes>>(1);
    let (img_tx, img_rx) = mpsc::channel(1);

    std::thread::spawn(move || -> Result<()> {
        let mut decoder = JpegDecoder::new(w, h)?;
        while let Some(update) = data_rx.blocking_recv() {
            let update = update.and_then_desktop(|x| decoder.decode(&x))?;
            img_tx
                .blocking_send(update)
                .map_err(|_| anyhow!("img_rx closed"))?;
        }
        Ok(())
    });

    (data_tx, img_rx)
}

async fn worker(
    mut conn: impl ServerConnection,
    mut shutdown: watch::Receiver<bool>,
    callback: EventCb,
) -> Result<()> {
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

    panic!("Not implemented");

    /*
    callback(TwilightClientEvent::Connected {
        width: resolution.width(),
        height: resolution.height(),
    });

    let (data_tx, mut img_rx) =
        decoder_pipeline(resolution.width(), resolution.height(), desktop_codec);

    let callback_inner = Rc::clone(&callback);
    let decoder = tokio::task::spawn_local(async move {
        let callback = callback_inner;

        while let Some(img) = img_rx.recv().await {
            callback(TwilightClientEvent::NextFrame(img))
        }
    });

    loop {
        let msg = tokio::select! {
            biased;
            _ = shutdown.changed() => break,
            x = rx.recv() => x,
        };

        // None => normal close
        let msg = match msg {
            Some(x) => x?,
            None => break,
        };

        let stream = u16::from_le_bytes((&msg[..2]).try_into().unwrap());
        assert_eq!(stream, 1);

        let frame: VideoFrame = parse_msg(&msg[2..])?;

        let payload = tokio::select! {
            biased;
            _ = shutdown.changed() => break,
            x = rx.recv() => x,
        };

        let payload: Bytes = match payload {
            Some(x) => x?,
            None => break,
        };

        ensure!(
            payload.len() == frame.video_bytes().as_usize(),
            "Video frame length does not match"
        );

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
    */

    Ok(())
}
