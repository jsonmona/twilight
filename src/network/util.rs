use anyhow::Result;
use flatbuffers::{FlatBufferBuilder, Follow, Verifiable, WIPOffset};
use futures_util::{Sink, SinkExt};

pub async fn send_msg<'buf, T, W>(
    stream: u16,
    sink: &mut W,
    builder: &mut FlatBufferBuilder<'buf>,
    msg: WIPOffset<T>,
) -> Result<()>
where
    W: Sink<tungstenite::Message> + Unpin,
    <W as Sink<tungstenite::Message>>::Error: std::error::Error + Send + Sync + 'static,
{
    builder.finish(msg, None);

    let packet = builder.finished_data();

    // stream id + data
    let mut buf = Vec::with_capacity(2 + packet.len());
    buf.extend_from_slice(&stream.to_le_bytes());
    buf.extend_from_slice(packet);

    sink.feed(tungstenite::Message::Binary(buf)).await?;
    builder.reset();
    Ok(())
}

pub async fn send_msg_with<'buf, T, W>(
    stream: u16,
    sink: &mut W,
    builder: &mut FlatBufferBuilder<'buf>,
    msg_fn: impl FnOnce(&mut FlatBufferBuilder<'buf>) -> WIPOffset<T>,
) -> Result<()>
where
    W: Sink<tungstenite::Message> + Unpin,
    <W as Sink<tungstenite::Message>>::Error: std::error::Error + Send + Sync + 'static,
{
    let msg = msg_fn(builder);
    send_msg(stream, sink, builder, msg).await
}

pub fn parse_msg<'data, T>(data: &'data [u8]) -> Result<T, flatbuffers::InvalidFlatbuffer>
where
    T: 'data + Follow<'data, Inner = T> + Verifiable,
{
    flatbuffers::root::<T>(data)
}
