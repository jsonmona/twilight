use actix::Addr;
use bytes::Bytes;
use flatbuffers::{FlatBufferBuilder, WIPOffset};
use futures_util::{stream::FuturesUnordered, StreamExt};
use parking_lot::RwLock;
use smallvec::SmallVec;

use super::web::{OutgoingMessage, WebsocketActor};

/// Represents a single channel
#[derive(Debug)]
pub struct Channel {
    pub ch: u16,
    clients: RwLock<SmallVec<[Addr<WebsocketActor>; 2]>>,
}

impl Channel {
    pub fn new(ch: u16) -> Self {
        Self {
            ch,
            clients: Default::default(),
        }
    }

    pub fn add_client(&self, addr: Addr<WebsocketActor>) {
        //TODO: Remove closed actors
        self.clients.write().push(addr);
    }

    pub async fn send_bytes(&self, msg: Bytes) {
        let mut futures = FuturesUnordered::new();

        for client in self.clients.read().iter() {
            futures.push(client.send(OutgoingMessage(msg.clone())));
        }

        while let Some(x) = futures.next().await {
            match x {
                Ok(_) => {}
                Err(e) => log::error!("Ignoring error while sending message via channel: {:?}", e),
            }
        }
    }

    pub async fn send_msg_with<'builder, T>(
        &self,
        builder: &mut FlatBufferBuilder<'builder>,
        f: impl FnOnce(&mut FlatBufferBuilder<'builder>) -> WIPOffset<T>,
    ) {
        builder.reset();

        let msg = f(builder);
        builder.finish(msg, None);

        let packet = builder.finished_data();

        let mut buf = Vec::with_capacity(2 + packet.len());
        buf.extend_from_slice(&self.ch.to_le_bytes());
        buf.extend_from_slice(packet);

        self.send_bytes(buf.into()).await;
    }
}
