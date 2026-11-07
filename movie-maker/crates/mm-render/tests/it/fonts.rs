//! プロジェクトの書体の素材を、ノードの種類に組が変わったときだけ渡し、フレームのキャッシュのキーに入れる。

use std::sync::{Arc, Mutex};

use mm_doc::{Asset, Project};
use mm_node_api::{FrameCx, FrameRef, NodeKind, Quality, Traits};
use mm_render_api::{RenderRequest, Renderer};
use mm_testkit::fixtures::invert_lut_asset;
use mm_text_api::FontFile;
use mm_types::{Id, ParamSchema, Params, Result, Rgba, Size};

use crate::common::{comp, gpu, layer, project, registry, renderer, solid};

const SIZE: Size = Size::new(32, 18);

/// 渡された書体の組を覚える種類。描くことはない。
#[derive(Default)]
struct FontSpy {
    calls: Mutex<Vec<Vec<FontFile>>>,
}

impl NodeKind for FontSpy {
    fn id(&self) -> &'static str {
        "font-spy"
    }
    fn version(&self) -> u32 {
        1
    }
    fn params(&self) -> ParamSchema {
        ParamSchema::new(Vec::new())
    }
    fn traits(&self) -> Traits {
        Traits::PIXEL
    }
    fn migrate(&self, old: serde_json::Value, _from: u32) -> Result<Params> {
        Ok(serde_json::from_value(old)?)
    }
    fn render(&self, cx: &mut FrameCx<'_>) -> Result<FrameRef> {
        Ok(FrameRef::Transparent { size: cx.size })
    }
    fn use_fonts(&self, fonts: &[FontFile]) -> Result<()> {
        self.calls.lock().unwrap().push(fonts.to_vec());
        Ok(())
    }
}

fn font_asset(hash: &str) -> Asset {
    let mut asset = invert_lut_asset();
    asset.id = Id::new("font-a");
    asset.path = "D:/proj/fonts/A B/A-Regular.ttf".into();
    asset.content_hash = hash.into();
    asset
}

fn with_font(hash: &str) -> Project {
    let mut p = project(
        vec![comp(
            "main",
            SIZE,
            1,
            vec![layer("bg", solid(Rgba::WHITE), 1)],
        )],
        Vec::new(),
    );
    p.assets.push(font_asset(hash));
    p
}

#[test]
fn 書体の組が変わったときだけ種類に渡し_キャッシュのキーを変える() {
    let Some(ctx) = gpu() else { return };
    let mut r = renderer(&ctx);
    let spy = Arc::new(FontSpy::default());
    let mut nodes = registry();
    nodes.insert(spy.clone());
    let mut draw = |p: &Project| {
        r.render_frame(&RenderRequest {
            project: p,
            composition: &Id::new("main"),
            time: mm_types::RationalTime::ZERO,
            size: SIZE,
            quality: Quality::Full,
            nodes: &nodes,
        })
        .expect("描ける");
        r.frame_cache_stats().entries
    };
    let first = draw(&with_font("aa"));
    assert_eq!(
        draw(&with_font("aa")),
        first,
        "同じ組なら覚えた画像に当たる"
    );
    let changed = draw(&with_font("bb"));
    assert!(
        changed > first,
        "書体の組が変わると別のキー（{first} → {changed}）"
    );
    let calls = spy.calls.lock().unwrap().clone();
    assert_eq!(calls.len(), 2, "組が変わったときだけ渡す");
    assert_eq!(calls[0][0].family.as_deref(), Some("A B"));
    assert_eq!(calls[1][0].content_hash, "bb");
}
