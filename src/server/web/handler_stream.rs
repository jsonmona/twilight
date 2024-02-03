use std::sync::Arc;

use actix::{Actor, ActorContext, AsyncContext, Handler, Message, StreamHandler};
use actix_web::{get, http::header, web, HttpRequest, HttpResponse, Responder};
use actix_web_actors::ws;
use bytes::Bytes;
use serde::Deserialize;

use super::{SessionId, Sessions, WebSession};

pub fn handler_stream(cfg: &mut web::ServiceConfig) {
    cfg.service((stream_v1,));
}

#[derive(Debug, Deserialize)]
struct AuthQuery {
    auth: String,
}

#[get("/stream/v1")]
async fn stream_v1(
    query: web::Query<AuthQuery>,
    sessions: web::Data<Sessions>,
    req: HttpRequest,
    stream: web::Payload,
) -> impl Responder {
    // Require Sec-WebSocket-Key header for enhanced security
    if !req.headers().contains_key(header::SEC_WEBSOCKET_KEY) {
        return Ok(HttpResponse::BadRequest().finish());
    }

    let sid = match SessionId::from_hex(&query.auth) {
        Some(x) => x,
        None => return Ok(HttpResponse::Forbidden().finish()),
    };

    let session = match sessions.lock().access(&sid) {
        Some(x) => x,
        None => return Ok(HttpResponse::Forbidden().finish()),
    };

    let actor = WebsocketActor {
        session,
        did_register: false,
    };

    ws::start(actor, &req, stream)
}

pub struct WebsocketActor {
    session: Arc<WebSession>,

    /// True if called `session.open_stream` successfully.
    did_register: bool,
}

impl Actor for WebsocketActor {
    type Context = ws::WebsocketContext<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        match self.session.open_stream(ctx.address().downgrade()) {
            Ok(_) => {
                self.did_register = true;
            }
            Err(e) => {
                println!("stopping stream due to error: {e:?}");
                ctx.stop();
            }
        }
    }

    fn stopped(&mut self, _ctx: &mut Self::Context) {
        if self.did_register {
            self.session.close_stream().unwrap();
        }
    }
}

impl Handler<OutgoingMessage> for WebsocketActor {
    type Result = ();

    fn handle(&mut self, msg: OutgoingMessage, ctx: &mut Self::Context) -> Self::Result {
        ctx.binary(msg.0);
    }
}

/// Handler for ws::Message message
impl StreamHandler<Result<ws::Message, ws::ProtocolError>> for WebsocketActor {
    fn handle(&mut self, msg: Result<ws::Message, ws::ProtocolError>, ctx: &mut Self::Context) {
        match msg {
            Ok(ws::Message::Ping(msg)) => ctx.pong(&msg),
            Ok(ws::Message::Binary(msg)) => println!("unexpected message received: {msg:?}"),
            _ => (),
        }
    }
}

pub struct OutgoingMessage(pub Bytes);

impl Message for OutgoingMessage {
    type Result = ();
}
