use criterion::{black_box, criterion_group, criterion_main, BatchSize, Criterion};
use rand::prelude::*;
use rand_xoshiro::Xoshiro256PlusPlus;
use twilight::image::{ColorFormat, ImageBuf};
use twilight::video::decoder::jpeg::JpegDecoder;
use twilight::video::decoder::DecoderStage;
use twilight::video::encoder::jpeg::JpegEncoder;
use twilight::video::encoder::EncoderStage;

fn make_image(w: u32, h: u32) -> ImageBuf {
    let mut random = Xoshiro256PlusPlus::seed_from_u64(0xdeadbeef5a5a5a5a);
    let mut img = ImageBuf::alloc(w, h, None, ColorFormat::Bgra8888);
    random.fill_bytes(&mut img.data);
    img.data[0..(w as usize) * 16].fill(random.next_u32() as u8);
    img
}

fn criterion_benchmark(c: &mut Criterion) {
    let width = black_box(1920);
    let height = black_box(1080);
    let mut encoder = JpegEncoder::new(width, height, true).unwrap();
    let mut decoder = JpegDecoder::new(width, height).unwrap();
    let img = make_image(width, height);
    let encoded = encoder.encode(img.copied()).unwrap();

    c.bench_function("jpeg_encode", |b| {
        b.iter_batched(
            || img.copied(),
            |x| encoder.encode(x).unwrap(),
            BatchSize::LargeInput,
        )
    });

    c.bench_function("jpeg_decode", |b| {
        b.iter_with_large_drop(|| decoder.decode(&encoded).unwrap())
    });
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
