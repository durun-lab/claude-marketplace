//! 偽の提供元が書くファイルの形。PNG（画像）、WAV（音）、Y4M（動画）。どれも FFmpeg が読める。

use std::f32::consts::TAU;

use mm_types::{MmError, Result};

/// 音のサンプルレート。
pub const SAMPLE_RATE: u32 = 48_000;
/// 音のチャンネル数。
pub const CHANNELS: u16 = 2;
const BITS_PER_SAMPLE: u16 = 16;
const BYTES_PER_SAMPLE: u16 = BITS_PER_SAMPLE / 8;
const WAV_HEADER_BYTES: u32 = 36;
const FMT_CHUNK_BYTES: u32 = 16;
const PCM_FORMAT: u16 = 1;
const MS_PER_SEC: u64 = 1000;

/// 単色の RGBA8 の PNG。
pub fn png_solid(width: u32, height: u32, rgba: [u8; 4]) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut out, width, height);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        enc.set_source_srgb(png::SrgbRenderingIntent::Perceptual);
        let mut writer = enc.write_header().map_err(png_error)?;
        let data = rgba.repeat(width as usize * height as usize);
        writer.write_image_data(&data).map_err(png_error)?;
    }
    Ok(out)
}

/// 正弦波の 16 bit の PCM の WAV。全部のチャンネルに同じ波を書く。
pub fn wav_sine(duration_ms: u64, hz: f32, amplitude: f32) -> Vec<u8> {
    let frames = (duration_ms * u64::from(SAMPLE_RATE) / MS_PER_SEC) as u32;
    let block = CHANNELS * BYTES_PER_SAMPLE;
    let data_len = frames * u32::from(block);
    let mut out = Vec::with_capacity((WAV_HEADER_BYTES + 8 + data_len) as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(WAV_HEADER_BYTES + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&FMT_CHUNK_BYTES.to_le_bytes());
    out.extend_from_slice(&PCM_FORMAT.to_le_bytes());
    out.extend_from_slice(&CHANNELS.to_le_bytes());
    out.extend_from_slice(&SAMPLE_RATE.to_le_bytes());
    out.extend_from_slice(&(SAMPLE_RATE * u32::from(block)).to_le_bytes());
    out.extend_from_slice(&block.to_le_bytes());
    out.extend_from_slice(&BITS_PER_SAMPLE.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    for i in 0..frames {
        let v = (TAU * hz * i as f32 / SAMPLE_RATE as f32).sin() * amplitude;
        let s = (v * f32::from(i16::MAX)) as i16;
        for _ in 0..CHANNELS {
            out.extend_from_slice(&s.to_le_bytes());
        }
    }
    out
}

/// 単色の YUV 4:2:0 の Y4M。`yuv` は 8 bit の Y、U、V。
pub fn y4m_solid(width: u32, height: u32, fps: u32, frames: u32, yuv: [u8; 3]) -> Vec<u8> {
    let luma = width as usize * height as usize;
    let chroma = width.div_ceil(2) as usize * height.div_ceil(2) as usize;
    let mut frame = Vec::with_capacity(luma + 2 * chroma);
    frame.resize(luma, yuv[0]);
    frame.resize(luma + chroma, yuv[1]);
    frame.resize(luma + 2 * chroma, yuv[2]);
    let mut out = format!("YUV4MPEG2 W{width} H{height} F{fps}:1 Ip A1:1 C420jpeg\n").into_bytes();
    for _ in 0..frames {
        out.extend_from_slice(b"FRAME\n");
        out.extend_from_slice(&frame);
    }
    out
}

fn png_error(e: png::EncodingError) -> MmError {
    MmError::Internal(format!("PNG を書けない: {e}"))
}
