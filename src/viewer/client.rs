use std::net::IpAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use anyhow::{anyhow, Result};
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;
use crate::image::{ColorFormat, Image, ImageBuf};
use crate::util::{CursorShape, CursorState, DesktopUpdate};
use tokio::io::{AsyncRead, AsyncWrite, AsyncReadExt, BufStream};
use crate::network::util::recv_msg;
use crate::schema::video::{NotifyVideoStart, VideoFrame};

pub struct Client {
    update: mpsc::Receiver<DesktopUpdate<ImageBuf>>,
    flag_run: Arc<AtomicBool>,
    resolution: (u32, u32),
    receiver: JoinHandle<Result<()>>,
}

impl Client {
    /// Connect to a IP address
    /// Uses the default port, 6495, if port is None
    pub async fn connect_to(addr: IpAddr, port: Option<u16>) -> Result<Self> {
        let port = port.unwrap_or(6495);

        let stream = tokio::net::TcpStream::connect((addr, port)).await?;
        stream.set_nodelay(true)?;

        let stream = BufStream::new(stream);

        Self::with_stream(stream).await
    }

    /// Start a brand-new connection with provided stream.
    /// The stream should be buffered (@see connect_to)
    pub async fn with_stream<RW: AsyncRead + AsyncWrite + Unpin + Send + 'static>(stream: RW) -> Result<Self> {
        let flag_run = Arc::new(AtomicBool::new(true));
        let (resolution_tx, resolution_rx) = oneshot::channel();
        let (tx, rx) = mpsc::channel(1);
        let receiver = tokio::task::spawn(frame_receiver(stream, tx, resolution_tx, flag_run.clone()));

        let resolution = resolution_rx.await?;

        Ok(Client {
            update: rx,
            flag_run,
            resolution,
            receiver,
        })
    }

    pub fn blocking_recv(&mut self) -> Option<DesktopUpdate<ImageBuf>> {
        self.update.blocking_recv()
    }

    pub async fn async_recv(&mut self) -> Option<DesktopUpdate<ImageBuf>> {
        self.update.recv().await
    }

    pub fn is_running(&self) -> bool {
        self.flag_run.load(Ordering::Relaxed)
    }

    pub fn signal_quit(&self) {
        self.flag_run.store(false, Ordering::Relaxed);
    }

    pub async fn join(mut self) -> Result<()> {
        self.flag_run.store(false, Ordering::Relaxed);
        self.update.close();
        self.receiver.await?
    }
}

async fn frame_receiver<RW: AsyncRead + AsyncWrite + Unpin>(mut stream: RW, tx: mpsc::Sender<DesktopUpdate<ImageBuf>>, resolution_tx: oneshot::Sender<(u32, u32)>, flag_run: Arc<AtomicBool>) -> Result<()> {
    let mut frames = 0;
    let mut buffer = vec![0u8; 2 * 1024 * 1024];

    let msg: NotifyVideoStart = recv_msg(&mut buffer, &mut stream).await?;
    let w = msg.resolution().map(|x| x.width()).unwrap_or_default();
    let h = msg.resolution().map(|x| x.height()).unwrap_or_default();
    let format =
        ColorFormat::from_video_codec(msg.desktop_codec()).expect("requires uncompressed format");

    resolution_tx.send((w, h)).map_err(|_| anyhow!("resolution_tx is closed"))?;

    while flag_run.load(Ordering::Relaxed) {
        let mut img = ImageBuf::alloc(w, h, None, format);

        let frame: VideoFrame = recv_msg(&mut buffer, &mut stream).await?;
        assert_eq!(frame.video_bytes(), img.data.len() as u64);

        stream.read_exact(&mut img.data).await?;

        let update = DesktopUpdate {
            cursor: frame.cursor_update().map(|u| CursorState {
                visible: u.visible(),
                pos_x: u.pos().map(|c| c.x()).unwrap_or_default(),
                pos_y: u.pos().map(|c| c.y()).unwrap_or_default(),
                shape: u.shape().and_then(|s| {
                    let w = s.resolution()?.width();
                    let h = s.resolution()?.height();
                    let img = Vec::from(s.image()?.bytes());
                    Some(CursorShape {
                        image: Image::new(w, h, w * 4, ColorFormat::Bgra8888, img),
                        hotspot_x: 0.0,
                        hotspot_y: 0.0,
                    })
                }),
            }),
            desktop: img,
        };

        if tx.send(update).await.is_err() {
            break;
        }
        frames += 1;
    }

    Ok(())
}