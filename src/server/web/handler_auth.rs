use actix_web::{post, web, HttpResponse, Responder};
use lazy_static::lazy_static;
use regex::Regex;
use serde::Serialize;

use crate::server::web::Sessions;

use super::SessionId;

pub fn handler_auth(cfg: &mut web::ServiceConfig) {
    cfg.service((auth_username,));
}

#[post("/auth/username")]
async fn auth_username(body: web::Bytes, sessions: web::Data<Sessions>) -> impl Responder {
    lazy_static! {
        static ref USERNAME_REGEX: Regex = Regex::new("^[A-Za-z0-9\\-_]+$").expect("valid regex");
    }

    let username = std::str::from_utf8(&body).unwrap();
    if !USERNAME_REGEX.is_match(username) {
        panic!("invalid username");
    }

    let session = sessions.lock().create_session().unwrap();

    HttpResponse::Ok().json(AuthSuccessResponse {
        token: session.sid().clone(),
    })
}

#[derive(Serialize)]
struct AuthSuccessResponse {
    token: SessionId,
}
