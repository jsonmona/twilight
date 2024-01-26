use anyhow::Result;

use super::web::serve_web;

pub async fn serve() -> Result<()> {
    serve_web().await?;

    Ok(())
}
