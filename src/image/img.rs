use crate::image::ColorFormat;

pub struct Image {
    pub width: u32,
    pub height: u32,
    pub color_format: ColorFormat,
    pub data: Vec<u8>,
}

pub struct ImageRef<'a> {
    pub width: u32,
    pub height: u32,
    pub color_format: ColorFormat,
    pub data: &'a [u8],
}

impl From<ImageRef<'_>> for Image {
    fn from(x: ImageRef<'_>) -> Self {
        Image {
            width: x.width,
            height: x.height,
            color_format: x.color_format,
            data: Vec::from(x.data),
        }
    }
}

impl Image {
    pub fn new(width: u32, height: u32, color_format: ColorFormat) -> Self {
        assert_eq!(width % 2, 0);
        assert_eq!(height % 2, 0);

        Image {
            width,
            height,
            color_format,
            data: vec![0; width as usize * height as usize * 4],
        }
    }

    pub fn as_ref(&self) -> ImageRef<'_> {
        ImageRef {
            width: self.width,
            height: self.height,
            color_format: self.color_format,
            data: &self.data,
        }
    }
}
