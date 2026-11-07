//! 入れ子のコンポジション（geneva の `compositions` と、それを指す `composition` の source）。
//!
//! geneva の入れ子は自分の大きさとレイヤーを持ち、1 枚の絵として画像と同じように置かれる。中の時刻はそれを見せる
//! クリップの始まりから数え、割合は入れ子の大きさに対する。movie-maker の入れ子のコンポジションも同じ扱い
//! （レイヤーの `in` からの時刻で描き、`(0, 0, 幅, 高さ)` に置く）なので、中のレイヤーは本編と同じ変換で作る。

use std::collections::HashMap;

use mm_doc::{Composition, Layer};
use mm_types::{Id, MmError, RationalTime, Result};
use serde_json::{Map, Value};

use super::look;
use super::visual::{self, Ctx};

/// 入れ子の名前ごとの大きさ。幅か高さが無い入れ子は誤り。
pub fn sizes(defs: &Map<String, Value>) -> Result<HashMap<String, [u32; 2]>> {
    defs.iter()
        .map(|(name, def)| {
            let side = |k: &str| {
                def.get(k)
                    .and_then(Value::as_u64)
                    .and_then(|v| u32::try_from(v).ok())
                    .filter(|v| *v > 0)
                    .ok_or_else(|| MmError::InvalidArgument(format!("入れ子 {name} の {k}")))
            };
            Ok((name.clone(), [side("width")?, side("height")?]))
        })
        .collect()
}

/// 全部の入れ子をコンポジションにする。`ctx.comps` に大きさを入れてから呼ぶ（入れ子の中の入れ子も引ける）。
pub fn compositions(ctx: &mut Ctx<'_>, defs: &Map<String, Value>) -> Result<Vec<Composition>> {
    let mut out = Vec::with_capacity(defs.len());
    for (name, def) in defs {
        let size = ctx.comps[name];
        let outer = std::mem::replace(&mut ctx.frame, size.map(f64::from));
        let layers = inner_layers(ctx, name, def);
        ctx.frame = outer;
        let layers = layers?;
        let duration = layers
            .iter()
            .map(|l| l.range().map(|r| r.end))
            .try_fold(RationalTime::from_frames(1, ctx.fps)?, |m, e| {
                e.map(|e| m.max(e))
            })?;
        out.push(
            Composition::new(Id::new(name), size[0], size[1], ctx.fps, duration)
                .with_layers(layers),
        );
    }
    Ok(out)
}

/// 入れ子の中のレイヤー。地の色（`background`）があれば、入れ子の長さいっぱいの単色を一番下に敷く。
fn inner_layers(ctx: &mut Ctx<'_>, name: &str, def: &Value) -> Result<Vec<Layer>> {
    let list = def
        .get("layers")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut layers = visual::visual_layers(ctx, &list)?;
    let background = def.get("background").and_then(Value::as_str);
    look::lay_background(&mut layers, background.unwrap_or(look::TRANSPARENT))?;
    for l in &mut layers {
        l.id = Id::new(format!("{name}/{}", l.id));
        for m in &mut l.masks {
            m.id = Id::new(format!("{name}/{}", m.id));
        }
    }
    Ok(layers)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    use crate::geneva_cmd::look::SOLID_KIND;
    use crate::geneva_cmd::media::Media;
    use mm_types::Rational;

    fn defs() -> Map<String, Value> {
        json!({
            "pip": {"width": 566, "height": 318, "background": "#000", "layers": [{"id": "v", "clips": [
                {"source": {"kind": "image", "asset": "png"}, "start": "0f", "duration": "54f",
                 "fit": "none", "transform": {"anchor": "0 0", "position": [-321.5, -276.5], "scale": 0.5}}
            ]}]}
        })
        .as_object()
        .cloned()
        .unwrap()
    }

    fn media() -> HashMap<String, Media> {
        HashMap::from([(
            "png".to_owned(),
            Media {
                size: Some([1920, 1080]),
                duration: RationalTime::seconds(1),
                fps: None,
                still: true,
            },
        )])
    }

    #[test]
    fn 入れ子は自分の大きさで中のレイヤーを変換する() {
        let media = media();
        let mut ctx = Ctx::new(Rational::integer(30), [1920.0, 1080.0], &media);
        ctx.comps = sizes(&defs()).unwrap();
        let comps = compositions(&mut ctx, &defs()).unwrap();
        assert_eq!(ctx.frame, [1920.0, 1080.0], "外の大きさに戻す");
        let pip = &comps[0];
        assert_eq!((pip.width, pip.height), (566, 318));
        assert_eq!(
            pip.duration,
            RationalTime::from_frames(54, ctx.fps).unwrap()
        );
        assert_eq!(pip.layers.len(), 2, "地の単色と中の画像");
        assert_eq!(pip.layers[0].source.kind, SOLID_KIND);
        let ids: Vec<&str> = pip.layers.iter().map(|l| l.id.as_str()).collect();
        assert_eq!(
            ids,
            ["pip/background", "pip/v-0"],
            "ID はプロジェクト全体で重ならない"
        );
        let pos = pip.layers[1].transform.position.value.as_vec2().unwrap();
        assert_eq!(pos, [-321.5, -276.5]);
    }

    #[test]
    fn 入れ子を指すクリップは入れ子の大きさの絵として置く() {
        let media = media();
        let mut ctx = Ctx::new(Rational::integer(30), [1920.0, 1080.0], &media);
        ctx.comps = sizes(&defs()).unwrap();
        let clip = json!({"source": {"kind": "composition", "composition": "pip"}, "start": "30f",
            "duration": "54f", "fit": "none", "transform": {"anchor": "0 0", "position": {"x": 1290, "y": 120}},
            "mask": {"radius": 26, "feather": 1}});
        let layers = visual::visual_layers(&mut ctx, &[json!({"clips": [clip]})]).unwrap();
        let l = &layers[0];
        assert_eq!(l.source.kind, visual::COMPOSITION_KIND);
        assert_eq!(l.in_point, RationalTime::ZERO);
        assert_eq!(
            l.transform.position.value.as_vec2().unwrap(),
            [1290.0, 120.0]
        );
        let rect = l.masks[0].rect.as_ref().unwrap();
        assert_eq!(rect.size.value.as_vec2().unwrap(), [566.0, 318.0]);
        assert_eq!(ctx.report.to_json(), json!([]));
    }

    #[test]
    fn 大きさの無い入れ子は誤り() {
        let bad = json!({"x": {"width": 10, "layers": []}});
        assert!(sizes(bad.as_object().unwrap()).is_err());
    }
}
