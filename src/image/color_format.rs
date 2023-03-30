use crate::schema::video::VideoCodec;

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
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

impl ColorFormat {
    pub fn from_video_codec(x: VideoCodec) -> Option<Self> {
        match x {
            VideoCodec::Bgra8888 => Some(ColorFormat::Bgra8888),
            VideoCodec::Rgb24 => Some(ColorFormat::Rgb24),
            _ => None,
        }
    }

    pub fn into_video_codec(self) -> Option<VideoCodec> {
        match self {
            ColorFormat::Bgra8888 => Some(VideoCodec::Bgra8888),
            ColorFormat::Rgb24 => Some(VideoCodec::Rgb24),
            _ => None,
        }
    }

    pub fn pixel_stride(self) -> u32 {
        match self {
            ColorFormat::Rgba8888 => 4,
            ColorFormat::Bgra8888 => 4,
            ColorFormat::Rgb24 => 3,
            ColorFormat::Nv12 => 1,
        }
    }
}
