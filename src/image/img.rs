use crate::image::ColorFormat;
use std::ops::Deref;

pub struct Image<D: Deref<Target = [u8]>> {
    pub width: u32,
    pub height: u32,
    pub color_format: ColorFormat,
    pub data: D,
}

pub type ImageBuf = Image<Vec<u8>>;

impl<D> Clone for Image<D>
where
    D: Clone + Deref<Target = [u8]>,
{
    fn clone(&self) -> Self {
        Image {
            width: self.width,
            height: self.height,
            color_format: self.color_format,
            data: self.data.clone(),
        }
    }
}

impl ImageBuf {
    pub fn alloc(width: u32, height: u32, color_format: ColorFormat) -> Self {
        assert_eq!(width % 2, 0);
        assert_eq!(height % 2, 0);

        ImageBuf {
            width,
            height,
            color_format,
            data: vec![0; width as usize * height as usize * 4],
        }
    }
}

impl<D: Deref<Target = [u8]>> Image<D> {
    pub fn new(width: u32, height: u32, color_format: ColorFormat, data: D) -> Self {
        Self {
            width,
            height,
            color_format,
            data,
        }
    }

    pub fn copy_data(&self) -> ImageBuf {
        ImageBuf {
            width: self.width,
            height: self.height,
            color_format: self.color_format,
            data: Vec::from(self.data.deref()),
        }
    }

    pub fn copy_into(&self, target: &mut ImageBuf) {
        if self.data.len() <= target.data.len() {
            // fast path
            target.width = self.width;
            target.height = self.height;
            target.color_format = self.color_format;

            target.data.truncate(self.data.len());
            target.data.copy_from_slice(&self.data);
        } else {
            // slow path
            *target = self.copy_data();
        }
    }
}
