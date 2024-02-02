use actix::Addr;
use bytes::Bytes;
use futures_util::{stream::FuturesUnordered, StreamExt};
use parking_lot::RwLock;
use smallvec::SmallVec;

use super::web::{OutgoingMessage, WebsocketActor};

/// Represents a single channel
#[derive(Debug)]
pub struct Channel {
    pub ch: u16,
    clients: RwLock<SmallVec<[Addr<WebsocketActor>; 4]>>,
}

impl Channel {
    pub fn new(ch: u16) -> Self {
        Self {
            ch,
            clients: Default::default(),
        }
    }

    pub async fn send_msg(&self, msg: Bytes) {
        let mut futures = FuturesUnordered::new();

        for client in self.clients.read().iter() {
            futures.push(client.send(OutgoingMessage(msg.clone())));
        }

        while let Some(x) = futures.next().await {
            match x {
                Ok(_) => {}
                Err(e) => println!("Ignoring error while sending message via channel: {:?}", e),
            }
        }
    }
}
