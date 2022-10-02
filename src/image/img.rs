use crate::image::ColorFormat;

pub struct ImageRef<'a> {
    pub color_format: ColorFormat,
    pub width: u32,
    pub height: u32,
    pub data: &'a [u8],
}
