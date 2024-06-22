use actix_web::{delete, put, web, HttpResponse, Responder};
use serde::Deserialize;

use crate::{network::dto::channel::OpenChannelResponse, server::SharedTwilightServer};

use super::SessionGuard;

pub fn handler_channel(cfg: &mut web::ServiceConfig) {
    cfg.service((put_channel, delete_channel));
}

#[put("/channel")]
async fn put_channel(
    session: SessionGuard,
    server: web::Data<SharedTwilightServer>,
) -> impl Responder {
    let channel = session.create_channel(&mut server.write());

    HttpResponse::Ok().json(OpenChannelResponse { ch: channel.ch })
}

#[derive(Debug, Deserialize)]
struct ChannelPath {
    ch: u16,
}

#[delete("/channel/{ch}")]
async fn delete_channel(session: SessionGuard, path: web::Path<ChannelPath>) -> impl Responder {
    session.close_channel(path.ch);

    HttpResponse::Ok().finish()
}
