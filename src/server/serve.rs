use crate::server::TwilightServer;
use anyhow::Result;
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncWrite, DuplexStream};
use tokio::net::TcpListener;

pub async fn serve() -> Result<()> {
    serve_inner::<DuplexStream>(None).await
}

pub async fn serve_debug<RW>(stream: RW) -> Result<()>
where
    RW: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    serve_inner(Some(stream)).await
}

async fn serve_inner<RW>(stream: Option<RW>) -> Result<()>
where
    RW: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    let server = TwilightServer::new().unwrap();
    if let Some(conn) = stream {
        server.add_conn(conn).await?;
    } else {
        let listener = TcpListener::bind("127.0.0.1:6497").await?;
        let (st, ad) = listener.accept().await?;
        println!("connected to {ad:?}");
        server.add_conn(st).await?;
    }
    tokio::time::sleep(Duration::from_secs(3600)).await;
    server.join_all().await;
    Ok(())
}
