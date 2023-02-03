use anyhow::Result;
use flatbuffers::{FlatBufferBuilder, Follow, Verifiable, WIPOffset};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

pub async fn send_msg<'buf, T, W: AsyncWrite + Unpin>(
    stream: &mut W,
    builder: &mut FlatBufferBuilder<'buf>,
    msg: WIPOffset<T>,
) -> Result<()> {
    builder.finish(msg, None);

    let packet = builder.finished_data();
    let packet_len = TryInto::<u32>::try_into(packet.len())
        .unwrap()
        .to_le_bytes();

    stream.write_all(&packet_len).await?;
    stream.write_all(packet).await?;
    builder.reset();
    Ok(())
}

pub async fn send_msg_with<'buf, T, W: AsyncWrite + Unpin>(
    stream: &mut W,
    builder: &mut FlatBufferBuilder<'buf>,
    msg_fn: impl FnOnce(&mut FlatBufferBuilder<'buf>) -> WIPOffset<T>,
) -> Result<()> {
    let msg = msg_fn(builder);
    send_msg(stream, builder, msg).await
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
    Ok(flatbuffers::root::<T>(packet)?)
}
