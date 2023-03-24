use anyhow::Result;
use flatbuffers::{FlatBufferBuilder, Follow, Verifiable, WIPOffset};
use futures_util::{Sink, SinkExt};
use tokio::io::{AsyncRead, AsyncReadExt};

pub async fn send_msg<'buf, T, W>(
    stream: &mut W,
    builder: &mut FlatBufferBuilder<'buf>,
    msg: WIPOffset<T>,
) -> Result<()>
where
    W: Sink<tungstenite::Message> + Unpin,
    <W as Sink<tungstenite::Message>>::Error: std::error::Error + Send + Sync + 'static,
{
    builder.finish(msg, None);

    let packet = builder.finished_data();

    stream
        .feed(tungstenite::Message::Binary(Vec::from(packet)))
        .await?;
    builder.reset();
    Ok(())
}

pub async fn send_msg_with<'buf, T, W>(
    stream: &mut W,
    builder: &mut FlatBufferBuilder<'buf>,
    msg_fn: impl FnOnce(&mut FlatBufferBuilder<'buf>) -> WIPOffset<T>,
) -> Result<()>
where
    W: Sink<tungstenite::Message> + Unpin,
    <W as Sink<tungstenite::Message>>::Error: std::error::Error + Send + Sync + 'static,
{
    let msg = msg_fn(builder);
    send_msg(stream, builder, msg).await
}

pub fn parse_msg<'data, T>(data: &'data [u8]) -> Result<T, flatbuffers::InvalidFlatbuffer>
where
    T: 'data + Follow<'data, Inner = T> + Verifiable,
{
    flatbuffers::root::<T>(data)
}

pub async fn recv_msg<'buf, T, R>(buf: &'buf mut Vec<u8>, stream: &mut R) -> Result<T>
where
    T: 'buf + Follow<'buf, Inner = T> + Verifiable,
    R: AsyncRead + Unpin,
{
    let packet_len: usize = stream.read_u32_le().await?.try_into().unwrap();

    if buf.len() < packet_len {
        buf.resize(packet_len, 0);
    }

    let packet = &mut buf[..packet_len];
    stream.read_exact(packet).await?;
    Ok(parse_msg(packet)?)
}
