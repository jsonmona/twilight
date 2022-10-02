pub enum ColorFormat {
    // 0xAABBGGRR in little-endian
    Rgba8888,

    // 0xAARRGGBB in little-endian
    Bgra8888,

    // A typical YUV420 format
    Nv12,
}
