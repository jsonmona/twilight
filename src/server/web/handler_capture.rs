use actix_web::{get, post, web, HttpResponse, Responder};
use serde::Serialize;

use crate::{
    network::dto::video::{ChannelCreation, DesktopInfo, MonitorInfo, RefreshRate, Resolution},
    server::{web::SessionGuard, SharedTwilightServer},
};

pub fn handler_capture(cfg: &mut web::ServiceConfig) {
    cfg.service((capture_desktop_get, capture_desktop_post));
}

#[get("/capture/desktop")]
async fn capture_desktop_get(
    _session: SessionGuard,
    _server: web::Data<SharedTwilightServer>,
) -> impl Responder {
    HttpResponse::Ok().json(DesktopInfo {
        monitor: [MonitorInfo {
            id: "dummy".into(),
            name: "dummy monitor".into(),
            resolution: Resolution {
                height: 1920,
                width: 1080,
            },
            refresh_rate: RefreshRate { den: 60, num: 1 },
        }]
        .into_iter()
        .collect(),
    })
}

#[post("/capture/desktop")]
async fn capture_desktop_post(
    session: SessionGuard,
    server: web::Data<SharedTwilightServer>,
) -> impl Responder {
    let stream = match session.stream() {
        Some(x) => x,
        None => {
            return HttpResponse::FailedDependency().finish();
        }
    };

    let channel = server.write().subscribe_desktop("dummy").unwrap();
    channel.add_client(stream);

    HttpResponse::Ok().json(ChannelCreation { ch: channel.ch })
}
