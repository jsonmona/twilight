use crate::image::ColorFormat;
use std::fmt::{Debug, Formatter};
use std::ops::Deref;

pub struct Image<D: Deref<Target = [u8]>> {
    pub width: u32,
    pub height: u32,
    pub stride: u32,
    pub color_format: ColorFormat,
    pub data: D,
}

pub type ImageBuf = Image<Vec<u8>>;

impl<D> Debug for Image<D>
where
    D: Deref<Target = [u8]>,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Image")
            .field("width", &self.width)
            .field("height", &self.height)
            .field("stride", &self.stride)
            .field("color_format", &self.color_format)
            .field("data", &self.data.as_ptr())
            .finish()
    }
}

impl<D> Clone for Image<D>
where
    D: Clone + Deref<Target = [u8]>,
{
    fn clone(&self) -> Self {
        Image {
            width: self.width,
            height: self.height,
            stride: self.stride,
            color_format: self.color_format,
            data: self.data.clone(),
        }
    }
}

fn align_to_multiple_of(x: u32, alignment: u32) -> u32 {
    if x % alignment == 0 {
        x
    } else {
        x + alignment - (x % alignment)
    }
}

impl ImageBuf {
    pub fn alloc(width: u32, height: u32, stride: Option<u32>, color_format: ColorFormat) -> Self {
        let stride = stride.unwrap_or_else(|| {
            let bytes_per_pixel = match color_format {
                ColorFormat::Rgba8888 => 4,
                ColorFormat::Bgra8888 => 4,
                ColorFormat::Rgb24 => 3,
                ColorFormat::Nv12 => 1,
            };

            align_to_multiple_of(bytes_per_pixel * width, 4)
        });

        let bytes = match color_format {
            ColorFormat::Rgba8888 => height * stride,
            ColorFormat::Bgra8888 => height * stride,
            ColorFormat::Rgb24 => height * stride,
            ColorFormat::Nv12 => {
                assert_eq!(height % 2, 0);
                (height + height / 2) * stride
            }
        }
        .try_into()
        .expect("image bytes count does not fit into usize");

        ImageBuf {
            width,
            height,
            stride,
            color_format,
            data: vec![0; bytes],
        }
    }
}

impl<D: Deref<Target = [u8]>> Image<D> {
    pub fn new(width: u32, height: u32, stride: u32, color_format: ColorFormat, data: D) -> Self {
        Self {
            width,
            height,
            stride,
            color_format,
            data,
        }
    }

    pub fn copy_data(&self) -> ImageBuf {
        ImageBuf {
            width: self.width,
            height: self.height,
            stride: self.stride,
            color_format: self.color_format,
            data: Vec::from(self.data.deref()),
        }
    }

    pub fn copy_into(&self, target: &mut ImageBuf) {
        if self.data.len() <= target.data.len() {
            // fast path
            target.width = self.width;
            target.height = self.height;
            target.stride = self.stride;
            target.color_format = self.color_format;

            target.data.truncate(self.data.len());
            target.data.copy_from_slice(&self.data);
        } else {
            // slow path
            *target = self.copy_data();
        }
    }

    pub fn validate(&self) {
        let height = self
            .height
            .try_into()
            .expect("height does not fit into usize");
        let _width: usize = self
            .width
            .try_into()
            .expect("width does not fit into usize");
        let stride = self
            .stride
            .try_into()
            .expect("stride does not fit into usize");

        let total_size =
            usize::checked_mul(height, stride).expect("total size does not fit into usize");
        assert!(
            total_size <= self.data.len(),
            "buffer is smaller than expected size"
        );
    }
}
