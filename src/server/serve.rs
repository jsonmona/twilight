use crate::server::TwilightServer;
use anyhow::Result;
use std::time::Duration;

pub async fn serve() -> Result<()> {
    let server = TwilightServer::new().unwrap();
    server.add_listener(6497).await?;
    tokio::time::sleep(Duration::from_secs(3600)).await;
    server.shutdown().await;
    Ok(())
}
