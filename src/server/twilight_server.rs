use anyhow::Result;
use flatbuffers::FlatBufferBuilder;
use parking_lot::RwLock;

use std::{
    mem::MaybeUninit,
    sync::{Arc, Weak},
};

use crate::{schema::video::*, util::DesktopUpdate, video::capture_pipeline};

use super::Channel;

/// The type that's carried around
pub type SharedTwilightServer = RwLock<TwilightServer>;

/// Represents the server as whole.
#[derive(Debug)]
pub struct TwilightServer {
    channels: Box<[Weak<Channel>; u16::MAX as usize]>,
    next_channel: u16,
}

impl TwilightServer {
    pub fn new() -> SharedTwilightServer {
        RwLock::new(Self {
            channels: boxed_array_of_weak(),
            next_channel: 0,
        })
    }

    /// This function is called from async context. Never perform too much work.
    pub fn recv_message(&self, msg: &[u8]) {
        println!("unexpected message received from {msg:?}");
    }

    pub fn subscribe_desktop(&mut self, monitor: &str) -> Result<Arc<Channel>> {
        let channel = self.create_channel();

        println!("subscribe to desktop on monitor {monitor}");

        let (_, mut output) = capture_pipeline()?;

        let ch = Arc::clone(&channel);
        tokio::spawn(async move {
            let mut builder = FlatBufferBuilder::with_capacity(8192);

            loop {
                let update = match output.recv().await {
                    Some(x) => x,
                    None => break,
                };

                //FIXME: needs some locking mechanism to prevent messages interleaving

                match send_desktop_update(&ch, &mut builder, &update).await {
                    Ok(_) => {}
                    Err(e) => {
                        log::error!("unexpected error whild sending message: {}", e);
                        break;
                    }
                }

                ch.send_bytes(update.desktop.into()).await;
            }
        });

        Ok(channel)
    }

    fn create_channel(&mut self) -> Arc<Channel> {
        for _ in 0..(u16::MAX as u32) {
            let ch = self.next_channel;
            self.next_channel = ch.wrapping_add(1);

            if let Some(x) = self.channels[ch as usize].upgrade() {
                assert_eq!(ch, x.ch, "channel number has modified");
                continue;
            }

            let channel = Arc::new(Channel::new(ch));
            self.channels[ch as usize] = Arc::downgrade(&channel);
            return channel;
        }

        unreachable!("already checked for remaining channel");
    }
}

/// Creates a boxed array of Weak<T> by filling them with `Weak::new()`.
/// Compiler will optimize it to single memset.
///
/// T: Sized is just to be safe. Check out comment of `Weak.ptr`.
fn boxed_array_of_weak<T: Sized, const LEN: usize>() -> Box<[Weak<T>; LEN]> {
    let mut boxed: Box<[MaybeUninit<Weak<T>>; LEN]> = bytemuck::zeroed_box();

    // safe because it is written only once
    for item in boxed.iter_mut() {
        item.write(Weak::new());
    }

    //TODO: Somehow remove this unsafe
    // safe because all elements are written
    unsafe { std::mem::transmute::<_, Box<[Weak<T>; LEN]>>(boxed) }
}

async fn send_desktop_update(
    ch: &Channel,
    builder: &mut FlatBufferBuilder<'_>,
    update: &DesktopUpdate<Vec<u8>>,
) -> Result<()> {
    ch.send_msg_with(builder, |builder| {
        let cursor_update = update.cursor.as_ref().map(|cursor| {
            let shape = cursor.shape.as_ref().map(|shape| {
                let image = builder.create_vector(&shape.image.data);

                CursorShape::create(
                    builder,
                    &CursorShapeArgs {
                        image: Some(image),
                        codec: VideoCodec::Jpeg,
                        xor: shape.xor,
                        hotspot: Some(&Coord2f::new(shape.hotspot_x, shape.hotspot_y)),
                        resolution: Some(&Size2u::new(shape.image.width, shape.image.height)),
                    },
                )
            });

            CursorUpdate::create(
                builder,
                &CursorUpdateArgs {
                    shape,
                    pos: Some(&Coord2u::new(cursor.pos_x, cursor.pos_y)),
                    visible: cursor.visible,
                },
            )
        });

        VideoFrame::create(
            builder,
            &VideoFrameArgs {
                video_bytes: update.desktop.len().try_into().unwrap(),
                cursor_update,
            },
        )
    })
    .await;

    Ok(())
}
