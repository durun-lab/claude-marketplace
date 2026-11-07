//! レイヤーの音の効果。状態を持つ効果（1 サンプルの遅延と、キーフレームの gain）で、書き出し（1 回で全体）と
//! 再生（短い区間の連続）が同じサンプルになること、失敗した効果はかけずに続けることを確かめる。

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use mm_audio_api::AudioBlock;
use mm_doc::{Asset, Interpolation, Keyframe, Node, Param, Project};
use mm_node_api::{AudioEffectCx, FrameCx, FrameRef, NodeKind, Traits};
use mm_render::WgpuRenderer;
use mm_render_api::{AudioRequest, Renderer};
use mm_testkit::decode::SampleMedia;
use mm_testkit::fixtures::SAMPLE_ASSET_PATH;
use mm_testkit::node::MapRegistry;
use mm_types::{
    ColorSpace, Id, MmError, ParamSchema, ParamSpec, ParamType, ParamValue, Params, RationalTime,
    Result, Size, TimeRange,
};

use crate::common::{comp, effect, gpu, layer, project, registry, renderer};

const KIND: &str = "test.delay_gain";
const RATE: u32 = 48_000;

/// y[n] = x[n-1] * gain。続きでない区間では遅延を空にする（`audio.clap` と同じ扱い）。
#[derive(Default)]
struct DelayGain {
    fail: bool,
    state: Mutex<HashMap<String, (RationalTime, [f32; 2])>>,
}

impl NodeKind for DelayGain {
    fn id(&self) -> &'static str {
        KIND
    }

    fn version(&self) -> u32 {
        1
    }

    fn params(&self) -> ParamSchema {
        let number = ParamType::Number {
            min: None,
            max: None,
        };
        ParamSchema::new(vec![ParamSpec::new(
            "gain",
            number,
            ParamValue::Number(1.0),
        )])
    }

    fn traits(&self) -> Traits {
        Traits {
            audio: true,
            pixel: false,
            ..Traits::PIXEL
        }
    }

    fn migrate(&self, old: serde_json::Value, _from: u32) -> Result<Params> {
        Ok(serde_json::from_value(old)?)
    }

    fn is_identity(&self, _cx: &FrameCx<'_>) -> bool {
        true
    }

    fn render(&self, cx: &mut FrameCx<'_>) -> Result<FrameRef> {
        Ok(FrameRef::Transparent { size: cx.size })
    }

    fn process_audio(&self, cx: &AudioEffectCx<'_>, block: &mut AudioBlock) -> Result<()> {
        if self.fail {
            return Err(MmError::Internal("試験の失敗".into()));
        }
        let mut states = self.state.lock().unwrap();
        let (next, last) = states
            .entry(cx.instance.to_owned())
            .or_insert((RationalTime::ZERO, [0.0; 2]));
        if *next != cx.range.start {
            *last = [0.0; 2];
        }
        let mut point = 0;
        for (i, frame) in block.samples.chunks_exact_mut(2).enumerate() {
            while point + 1 < cx.params.len() && cx.params[point + 1].0 <= i {
                point += 1;
            }
            let gain = cx.params[point].1.number_or("gain", 1.0) as f32;
            for (c, s) in frame.iter_mut().enumerate() {
                let x = *s;
                *s = last[c] * gain;
                last[c] = x;
            }
        }
        *next = cx.range.end;
        Ok(())
    }
}

fn ms(v: i128) -> RationalTime {
    RationalTime::new(v, 1000).expect("表せる")
}

/// 素材の音のレイヤーに、gain を 1 秒で 0 から 2 へ上げる効果を挿したプロジェクト。
fn scene(with_effect: bool) -> Project {
    let footage =
        Node::new("footage", 1).with_param("asset", ParamValue::AssetRef(Id::new("clip")));
    let mut l = layer("f", footage, 2);
    if with_effect {
        let mut node = Node::new(KIND, 1);
        let key = |t, v| Keyframe::new(t, ParamValue::Number(v), Interpolation::Linear);
        node.params.insert(
            "gain".into(),
            Param {
                value: ParamValue::Number(1.0),
                keyframes: vec![key(ms(0), 0.0), key(ms(1000), 2.0)],
            },
        );
        l.effects.push(effect("e", node));
    }
    let mut p = project(vec![comp("main", Size::new(16, 16), 2, vec![l])], vec![]);
    let media = SampleMedia::default();
    p.assets.push(Asset {
        id: Id::new("clip"),
        path: SAMPLE_ASSET_PATH.into(),
        content_hash: "sample0000000000".into(),
        color_space: ColorSpace::bt709(),
        duration: media.duration,
        fps: Some(media.fps),
        timebase_offset: RationalTime::ZERO,
    });
    p
}

fn nodes(kind: DelayGain) -> MapRegistry {
    let mut r = registry();
    r.insert(Arc::new(kind));
    r
}

fn mix(r: &mut WgpuRenderer, p: &Project, nodes: &MapRegistry, cuts: &[i128]) -> Vec<f32> {
    let mut out = Vec::new();
    for w in cuts.windows(2) {
        let block = r
            .render_audio(&AudioRequest {
                project: p,
                composition: &Id::new("main"),
                range: TimeRange::new(ms(w[0]), ms(w[1])).unwrap(),
                sample_rate: RATE,
                channels: 2,
                nodes,
            })
            .expect("音を混ぜられる");
        out.extend(block.samples);
    }
    out
}

#[test]
fn 書き出しと再生は同じサンプルになる() {
    let Some(ctx) = gpu() else { return };
    let p = scene(true);
    let export = mix(
        &mut renderer(&ctx),
        &p,
        &nodes(DelayGain::default()),
        &[0, 1500],
    );
    let plain = mix(&mut renderer(&ctx), &scene(false), &registry(), &[0, 1500]);
    assert_ne!(export, plain, "効果がかかる");
    for cuts in [vec![0, 100, 200, 700, 1500], vec![0, 13, 371, 1001, 1500]] {
        let preview = mix(&mut renderer(&ctx), &p, &nodes(DelayGain::default()), &cuts);
        assert!(export == preview, "{cuts:?} に分けて");
    }
}

#[test]
fn 失敗した効果はかけずに続けてエラーを残す() {
    let Some(ctx) = gpu() else { return };
    let mut r = renderer(&ctx);
    let failing = DelayGain {
        fail: true,
        ..DelayGain::default()
    };
    let got = mix(&mut r, &scene(true), &nodes(failing), &[0, 500]);
    let plain = mix(&mut renderer(&ctx), &scene(false), &registry(), &[0, 500]);
    assert_eq!(got, plain);
    let errors = r.take_errors();
    assert!(errors.iter().any(|e| e.contains(KIND)), "{errors:?}");
}
