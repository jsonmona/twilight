use actix_web::{
    web::{self, ServiceConfig},
    App, HttpServer,
};
use anyhow::Result;

use crate::server::TwilightServer;

use super::{
    handler_auth::handler_auth, handler_capture::handler_capture, handler_channel::handler_channel,
    handler_stream::handler_stream, SessionStorage,
};

pub async fn serve_web() -> Result<()> {
    let base_path = "/twilight";

    let session_storage = web::Data::new(SessionStorage::new());
    let twilight_server = web::Data::new(TwilightServer::new());

    HttpServer::new(move || {
        App::new()
            .app_data(session_storage.clone())
            .app_data(twilight_server.clone())
            .service(web::scope(base_path).configure(all_handlers))
    })
    .bind(("127.0.0.1", 1518))?
    .run()
    .await?;

    Ok(())
}

fn all_handlers(config: &mut ServiceConfig) {
    config.configure(handler_auth);
    config.configure(handler_capture);
    config.configure(handler_channel);
    config.configure(handler_stream);
}
