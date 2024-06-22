use std::sync::Arc;

use actix_web::{get, post, web, HttpResponse, Responder};

use crate::{
    network::dto::video::{DesktopInfo, MonitorInfo, RefreshRate, Resolution, StartCapture},
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
                height: 1080,
                width: 1920,
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
    body: web::Json<StartCapture>,
) -> impl Responder {
    let stream = match session.stream() {
        Some(x) => x,
        None => {
            return HttpResponse::FailedDependency().finish();
        }
    };

    let channel = match session.get_channel(body.ch) {
        Some(x) => x,
        None => {
            return HttpResponse::FailedDependency().finish();
        }
    };

    match server
        .write()
        .subscribe_desktop(&body.id, Arc::clone(&channel))
    {
        Ok(_) => {}
        Err(e) => {
            //FIXME: What should i do?
            log::error!("What should i do?\n{e:?}");
            return HttpResponse::InternalServerError().finish();
        }
    }

    channel.add_client(stream);

    HttpResponse::Ok().finish()
}
