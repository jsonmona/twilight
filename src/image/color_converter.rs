use crate::image::{ColorFormat, Image};
use crate::util::AsUsize;
use std::ops::{Deref, DerefMut};

pub fn convert_color<A, B>(src: &Image<A>, dst: &mut Image<B>)
where
    A: Deref<Target = [u8]>,
    B: DerefMut<Target = [u8]>,
{
    assert_eq!(src.height, dst.height);
    assert_eq!(src.width, dst.width);

    src.validate();
    dst.validate();

    use ColorFormat::*;

    if src.color_format == dst.color_format {
        dst.data.copy_from_slice(&src.data);
    } else if src.color_format == Bgra8888 && dst.color_format == Rgb24 {
        convert_bgra8888_rgb24(src, dst);
    } else if src.color_format == Rgb24 && dst.color_format == Bgra8888 {
        convert_rgb24_bgra8888(src, dst);
    } else {
        unimplemented!(
            "cannot convert {:?} to {:?}",
            src.color_format,
            dst.color_format
        );
    }
}

fn convert_bgra8888_rgb24<A, B>(src: &Image<A>, dst: &mut Image<B>)
where
    A: Deref<Target = [u8]>,
    B: DerefMut<Target = [u8]>,
{
    assert_eq!(src.height, dst.height);
    assert_eq!(src.width, dst.width);
    assert_eq!(src.color_format, ColorFormat::Bgra8888);
    assert_eq!(dst.color_format, ColorFormat::Rgb24);

    let data_src = src.data.deref();
    let data_dst = dst.data.deref_mut();

    let height = src.height.as_usize();
    let width = src.width.as_usize();

    let src_stride = usize::checked_mul(width, 4).expect("source stride too large to fit in usize");
    let dst_stride =
        usize::checked_mul(width, 3).expect("destination stride too large to fit in usize");

    let src_expected_size = usize::checked_mul(height, src_stride)
        .expect("source bytes count too large to fit in usize");
    let dst_expected_size = usize::checked_mul(height, dst_stride)
        .expect("destination bytes count too large to fit in usize");

    assert!(src_expected_size <= data_src.len());
    assert!(dst_expected_size <= data_dst.len());

    for i in 0..height {
        for j in 0..width {
            unsafe {
                let b = *data_src.get_unchecked(i * src_stride + j * 4);
                let g = *data_src.get_unchecked(i * src_stride + j * 4 + 1);
                let r = *data_src.get_unchecked(i * src_stride + j * 4 + 2);

                *data_dst.get_unchecked_mut(i * dst_stride + j * 3) = r;
                *data_dst.get_unchecked_mut(i * dst_stride + j * 3 + 1) = g;
                *data_dst.get_unchecked_mut(i * dst_stride + j * 3 + 2) = b;
            }
        }
    }
}

fn convert_rgb24_bgra8888<A, B>(src: &Image<A>, dst: &mut Image<B>)
where
    A: Deref<Target = [u8]>,
    B: DerefMut<Target = [u8]>,
{
    assert_eq!(src.height, dst.height);
    assert_eq!(src.width, dst.width);
    assert_eq!(src.color_format, ColorFormat::Rgb24);
    assert_eq!(dst.color_format, ColorFormat::Bgra8888);

    let data_src = src.data.deref();
    let data_dst = dst.data.deref_mut();

    let height = src.height.as_usize();
    let width = src.width.as_usize();
    let src_stride = usize::checked_mul(width, 3).expect("source stride too large to fit in usize");
    let dst_stride =
        usize::checked_mul(width, 4).expect("destination stride too large to fit in usize");

    assert!(height * src_stride <= data_src.len());
    assert!(height * dst_stride <= data_dst.len());

    for i in 0..height {
        for j in 0..width {
            unsafe {
                let r = *data_src.get_unchecked(i * src_stride + j * 3);
                let g = *data_src.get_unchecked(i * src_stride + j * 3 + 1);
                let b = *data_src.get_unchecked(i * src_stride + j * 3 + 2);

                *data_dst.get_unchecked_mut(i * dst_stride + j * 4) = b;
                *data_dst.get_unchecked_mut(i * dst_stride + j * 4 + 1) = g;
                *data_dst.get_unchecked_mut(i * dst_stride + j * 4 + 2) = r;
                *data_dst.get_unchecked_mut(i * dst_stride + j * 4 + 3) = 255;
            }
        }
    }
}
