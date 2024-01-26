use actix_web::{
    web::{self, ServiceConfig},
    App, HttpServer,
};
use anyhow::Result;

use super::{handler_auth::handler_auth, session::SessionStorage};

pub async fn serve_web() -> Result<()> {
    let base_path = "/twilight";

    let session_storage = web::Data::new(SessionStorage::new());

    HttpServer::new(move || {
        App::new()
            .app_data(session_storage.clone())
            .service(web::scope(base_path).configure(all_handlers))
    })
    .bind(("127.0.0.1", 3000))?
    .run()
    .await?;

    Ok(())
}

fn all_handlers(config: &mut ServiceConfig) {
    config.configure(handler_auth);
}
