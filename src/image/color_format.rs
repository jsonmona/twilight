#[derive(Copy, Clone)]
pub enum ColorFormat {
    // 0xAABBGGRR in little-endian
    Rgba8888,

    // 0xAARRGGBB in little-endian
    Bgra8888,

    // laid out as RR, GG, BB, RR, GG, BB, ...
    Rgb24,

    // A typical YUV420 format
    Nv12,
}
