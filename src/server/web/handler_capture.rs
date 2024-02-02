use actix_web::{get, post, web, HttpResponse, Responder};
use serde::Serialize;
use smallvec::SmallVec;

use crate::server::web::SessionGuard;

pub fn handler_capture(cfg: &mut web::ServiceConfig) {
    cfg.service((capture_desktop_get,));
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
    todo!();
    HttpResponse::ImATeapot().finish()
}

#[post("/capture/desktop")]
async fn capture_desktop_post(_session: SessionGuard) -> impl Responder {
    todo!();
    HttpResponse::ImATeapot().finish()
}
