//! モックの実装が自分の契約テストを通すことを確かめる。

use std::sync::Arc;

use mm_doc::Fade;
use mm_render_api::{AudioRequest, Renderer};
use mm_testkit::audio::SampleMixerFactory;
use mm_testkit::cache::MemoryCache;
use mm_testkit::color::SampleColor;
use mm_testkit::decode::{DecodeFixture, SampleDecoderFactory, SampleEncoderFactory, SampleMedia};
use mm_testkit::fixtures::{sample_services, vertical_project};
use mm_testkit::host::{HostFixture, SampleLauncher};
use mm_testkit::node::{FootageKind, MapRegistry, NodeFixture, SolidKind, TextKind};
use mm_testkit::render::{RenderFixture, SampleRenderer};
use mm_testkit::text::{SampleRasterizer, SampleShaper};
use mm_types::{RationalTime, Size, TimeRange};

#[test]
fn モックのデコーダーは契約を通す() {
    let media = SampleMedia::default();
    mm_testkit::decode::contract(
        || {
            Arc::new(SampleDecoderFactory {
                media: media.clone(),
            })
        },
        &DecodeFixture::sample(&media),
    );
}

#[test]
fn モックのエンコーダーは契約を通す() {
    mm_testkit::decode::encoder_contract(
        || Arc::new(SampleEncoderFactory),
        mm_testkit::SAMPLE_ID,
        &std::env::temp_dir(),
    );
}

#[test]
fn モックのミキサーは契約を通す() {
    mm_testkit::audio::contract(|| Arc::new(SampleMixerFactory));
}

#[test]
fn モックの文字は契約を通す() {
    mm_testkit::text::contract(|| Arc::new(SampleShaper), || Arc::new(SampleRasterizer));
}

#[test]
fn モックの色は契約を通す() {
    mm_testkit::color::contract(|| Arc::new(SampleColor));
}

#[test]
fn モックのホストは契約を通す() {
    mm_testkit::host::contract(|| Arc::new(SampleLauncher), &HostFixture::sample());
}

#[test]
fn モックのキャッシュは契約を通す() {
    mm_testkit::cache::contract(|limit| Arc::new(MemoryCache::new(Some(limit))), 1024);
}

#[test]
fn 付属ファイルの契約を通す() {
    mm_testkit::ingest::contract(&[]);
}

#[test]
fn モックの種類は契約を通す() {
    mm_testkit::node::contract(|| Arc::new(SolidKind), &NodeFixture::default());
    mm_testkit::node::contract(|| Arc::new(TextKind), &NodeFixture::default());
    mm_testkit::node::contract(
        || Arc::new(FootageKind),
        &NodeFixture {
            footage: Some(SampleMedia::default()),
            ..NodeFixture::default()
        },
    );
    mm_testkit::node::contract(|| Arc::new(FootageKind), &NodeFixture::default());
}

#[test]
fn モックのレンダラーは音のフェードを掛ける() {
    let media = SampleMedia::default();
    let out = std::env::temp_dir().join("mm_testkit_render_fade.json");
    let (mut project, comp, _) = vertical_project(&media, &out);
    let layer = &mut project.compositions[0].layers[0];
    let range = layer.range().unwrap();
    let rate = mm_audio_api::DEFAULT_SAMPLE_RATE;
    layer.audio.as_mut().unwrap().fade_out = Some(Fade::linear(RationalTime::seconds(1)));
    let nodes = MapRegistry::with_samples();
    let mut r = SampleRenderer::new(
        Arc::new(SampleDecoderFactory {
            media: media.clone(),
        }),
        Arc::new(SampleMixerFactory),
        sample_services(),
    );
    let tail = TimeRange::new(
        range.end.checked_sub(RationalTime::seconds(1)).unwrap(),
        range.end,
    )
    .unwrap();
    let block = r
        .render_audio(&AudioRequest {
            project: &project,
            composition: &comp,
            range: tail,
            sample_rate: rate,
            channels: mm_audio_api::DEFAULT_CHANNELS,
            nodes: &nodes,
        })
        .unwrap();
    let ch = usize::from(block.channels);
    let peak = |frames: std::ops::Range<usize>| {
        block.samples[frames.start * ch..frames.end * ch]
            .iter()
            .fold(0f32, |m, s| m.max(s.abs()))
    };
    let n = block.frames();
    assert!(peak(0..n / 8) > 0.0, "フェードの始まりは鳴る");
    assert!(peak(n - 1..n) < 1e-3, "フェードの末尾は無音");
    assert!(
        peak(n / 8..n / 4) > peak(n - n / 8..n),
        "末尾へ向かって小さくなる"
    );
}

#[test]
fn モックのレンダラーは契約を通す() {
    let media = SampleMedia::default();
    let out = std::env::temp_dir().join("mm_testkit_render_contract.json");
    let (project, comp, _) = vertical_project(&media, &out);
    let nodes = MapRegistry::with_samples();
    mm_testkit::render::contract(
        || {
            Box::new(SampleRenderer::new(
                Arc::new(SampleDecoderFactory {
                    media: media.clone(),
                }),
                Arc::new(SampleMixerFactory),
                sample_services(),
            ))
        },
        &RenderFixture {
            project: &project,
            composition: &comp,
            nodes: &nodes,
            size: Size::new(media.size.width, media.size.height),
        },
    );
}
