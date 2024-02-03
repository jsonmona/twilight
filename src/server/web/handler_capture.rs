use actix_web::{get, post, web, HttpResponse, Responder};
use serde::Serialize;
use smallvec::SmallVec;

use crate::server::{web::SessionGuard, SharedTwilightServer};

pub fn handler_capture(cfg: &mut web::ServiceConfig) {
    cfg.service((capture_desktop_get, capture_desktop_post));
}

#[derive(Debug, Serialize)]
struct DesktopInfo {
    monitor: SmallVec<[MonitorInfo; 2]>,
}

#[derive(Debug, Serialize)]
struct MonitorInfo {
    id: String,
    name: String,
    resolution: String,
    refresh_rate: String,
}

#[get("/capture/desktop")]
async fn capture_desktop_get(_session: SessionGuard) -> impl Responder {
    HttpResponse::Ok().json(DesktopInfo {
        monitor: [MonitorInfo {
            id: "dummy".into(),
            name: "dummy monitor".into(),
            resolution: "0x0".into(),
            refresh_rate: "0/0".into(),
        }]
        .into_iter()
        .collect(),
    })
}

#[derive(Debug, Serialize)]
struct ChannelCreation {
    ch: u16,
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
