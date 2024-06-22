pub fn parse_msg<'buf, T>(data: &'buf [u8]) -> Result<T, flatbuffers::InvalidFlatbuffer>
where
    T: 'buf + flatbuffers::Follow<'buf, Inner = T> + flatbuffers::Verifiable,
{
    let end = 4 + get_prefixed_size(data);
    let content = &data[..end];
    flatbuffers::size_prefixed_root::<T>(content)
}

/// Returns the embedded "payload", buffer content after the flatbuffer message.
pub fn parse_msg_payload(data: &bytes::Bytes) -> bytes::Bytes {
    let start = 4 + get_prefixed_size(&data);
    data.slice(start..)
}

fn get_prefixed_size(data: &[u8]) -> usize {
    assert_eq!(flatbuffers::SIZE_SIZEPREFIX, 4);
    assert!(
        4 <= data.len(),
        "flatbuffer data must contain a size prefix"
    );

    //FIXME: Where is GetPrefixedSize() counterpart in Rust?
    u32::from_le_bytes(data[..4].try_into().expect("sliced and checked size above")) as usize
}

#[rustfmt::skip]
#[allow(warnings, unused)]
mod schema_generated;

pub use schema_generated::*;
