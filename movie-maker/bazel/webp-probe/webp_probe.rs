//! libwebp を Bazel でビルドしてリンクできるかの試し。mm-stream と同じ RGB の符号化を通す。

const SIDE: u32 = 4;
const CHANNELS: usize = 3;
const QUALITY: f32 = 80.0;

#[test]
fn rgb_を符号化して復号すると同じ大きさに戻る() {
    let rgb = vec![128u8; (SIDE * SIDE) as usize * CHANNELS];
    let encoded = webp::Encoder::new(&rgb, webp::PixelLayout::Rgb, SIDE, SIDE)
        .encode(QUALITY)
        .to_vec();
    let decoded = webp::Decoder::new(&encoded).decode().expect("復号できる");
    assert_eq!((decoded.width(), decoded.height()), (SIDE, SIDE));
}
