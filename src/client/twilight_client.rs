use crate::client::server_connection::{FetchResponse, MessageStream, ServerConnection};
use crate::image::{convert_color, ColorFormat, Image, ImageBuf};
use crate::network::util::parse_msg;
use crate::schema::video::{Coord2f, Coord2u, NotifyVideoStart, VideoCodec, VideoFrame};
use crate::util::{CursorShape, CursorState, DesktopUpdate};
use anyhow::{bail, ensure, Context, Result};

use hyper::body::Bytes;
use hyper::Method;

use std::future::Future;

use tokio::sync::watch;
use tokio::task::JoinHandle;

#[derive(Debug)]
pub enum TwilightClientEvent {
    Connected { width: u32, height: u32 },
    NextFrame(DesktopUpdate<ImageBuf>),
    Closed(Result<()>),
}

type EventCb = Box<dyn Fn(TwilightClientEvent) + Send>;

pub struct TwilightClient {
    shutdown: watch::Sender<bool>,
    worker: JoinHandle<()>,
}

impl TwilightClient {
    pub fn new<Conn, ConnFut>(callback: EventCb, callback2: EventCb, conn: ConnFut) -> Self
    where
        Conn: ServerConnection,
        ConnFut: Future<Output = Result<Conn>> + Send + 'static,
    {
        let (tx, rx) = watch::channel(false);

        let worker = tokio::task::spawn(async move {
            let result = match conn.await {
                Ok(c) => worker(c, rx, callback).await,
                Err(e) => Err(e),
            };
            callback2(TwilightClientEvent::Closed(result));
        });

        TwilightClient {
            shutdown: tx,
            worker,
        }
    }
}

async fn worker<Conn: ServerConnection>(
    mut conn: Conn,
    mut shutdown: watch::Receiver<bool>,
    callback: EventCb,
) -> Result<()> {
    let res = conn.fetch(Method::POST, "/auth?type=username", b"testuser");

    let res = tokio::select! {
        biased;
        _ = shutdown.changed() => return Ok(()),
        x = res => x
    }?;

    if !res.status().is_success() {
        bail!("unable to authenticate: status={}", res.status());
    }

    let (_sink, mut stream) = conn.upgrade().await?;

    let msg = tokio::select! {
        biased;
        _ = shutdown.changed() => return Ok(()),
        x = stream.recv() => x,
    };

    let msg = match msg {
        Some(x) => x?,
        None => bail!("resolution not received"),
    };

    let start: NotifyVideoStart = parse_msg(&msg)?;
    let desktop_codec = start.desktop_codec();
    let resolution = start
        .resolution()
        .cloned()
        .context("resolution not present")?;
    callback(TwilightClientEvent::Connected {
        width: resolution.width(),
        height: resolution.height(),
    });

    loop {
        let msg = tokio::select! {
            biased;
            _ = shutdown.changed() => break,
            x = stream.recv() => x,
        };

        let msg = match msg {
            Some(x) => x?,
            None => break,
        };

        let frame: VideoFrame = parse_msg(&msg)?;

        let payload = tokio::select! {
            biased;
            _ = shutdown.changed() => break,
            x = stream.recv() => x,
        };

        let payload: Bytes = match payload {
            Some(x) => x?,
            None => break,
        };

        ensure!(
            TryInto::<u64>::try_into(payload.len())? == frame.video_bytes(),
            "Video frame length does not match"
        );

        assert_eq!(desktop_codec, VideoCodec::Rgb24);
        let payload = Image::new(
            resolution.width(),
            resolution.height(),
            resolution.width() * 3,
            ColorFormat::Rgb24,
            payload,
        );
        let mut desktop_img = ImageBuf::alloc(
            resolution.width(),
            resolution.height(),
            None,
            ColorFormat::Bgra8888,
        );
        convert_color(&payload, &mut desktop_img);

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
                            hotspot_x: hotspot.x(),
                            hotspot_y: hotspot.y(),
                        })
                    }),
                }
            }),
            desktop: desktop_img,
        };

        callback(TwilightClientEvent::NextFrame(update));
    }

    Ok(())
}
