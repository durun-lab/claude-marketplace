//! 色と重ね方: `solid` の source を movie-maker の単色のノードに、`blend` をレイヤーの合成モードにする。

use mm_doc::{BlendMode, Effect, Layer, Node};
use mm_types::{Id, ParamValue, RationalTime, Result, Rgba};
use serde_json::Value;

use super::report::{Gap, Report};
use super::visual::KIND_VERSION;

/// geneva の単色の source の種類。movie-maker の単色のノードの種類（`mm-nodes-basic` の solid）と同じ名前。
pub const SOLID_KIND: &str = "solid";
const COLOR_PARAM: &str = "color";
/// ガウスぼかしの効果の種類と、標準偏差（入力の画素）のパラメータ（`mm-nodes-basic` の blur.gaussian）。
const BLUR_KIND: &str = "blur.gaussian";
const RADIUS_PARAM: &str = "radius";
/// 地の色の名前（geneva の出力の既定と、入れ子の既定）。
pub const BLACK: &str = "black";
pub const TRANSPARENT: &str = "transparent";
/// 地の色のレイヤーの ID。
const BACKGROUND_ID: &str = "background";
/// `#RGB`、`#RRGGBB`、`#RRGGBBAA` の 16 進の桁の数。
const SHORT_HEX: usize = 3;
const OPAQUE_HEX: usize = 6;
const ALPHA_HEX: usize = 8;

/// `solid` の source を単色のノードにする。色が読めない（動く色、知らない名前）なら `None`。
pub fn solid_node(source: &Value) -> Option<Node> {
    let color = source
        .get(COLOR_PARAM)
        .and_then(Value::as_str)
        .and_then(color)?;
    Some(Node::new(SOLID_KIND, KIND_VERSION).with_param(COLOR_PARAM, ParamValue::Color(color)))
}

/// geneva の色（`#RGB`、`#RRGGBB`、`#RRGGBBAA`、`black`、`white`、`transparent`）を線形の premultiplied にする。
/// 符号化値は `Rgba::from_code8` が書き出しで同じ値に戻る式で線形にする。
pub fn color(text: &str) -> Option<Rgba> {
    let text = text.trim();
    match text {
        "black" => return Some(Rgba::BLACK),
        "white" => return Some(Rgba::WHITE),
        "transparent" => return Some(Rgba::TRANSPARENT),
        _ => {}
    }
    let hex = text.strip_prefix('#')?;
    let digit = |i: usize, n: usize| u8::from_str_radix(hex.get(i..i + n)?, 16).ok();
    let code = match hex.len() {
        SHORT_HEX => {
            let one = |i| digit(i, 1).map(|d| d * 17);
            [one(0)?, one(1)?, one(2)?, u8::MAX]
        }
        OPAQUE_HEX => [digit(0, 2)?, digit(2, 2)?, digit(4, 2)?, u8::MAX],
        ALPHA_HEX => [digit(0, 2)?, digit(2, 2)?, digit(4, 2)?, digit(6, 2)?],
        _ => return None,
    };
    Some(Rgba::from_code8(code))
}

/// geneva の地の色（`background`）を、全部のレイヤーの長さいっぱいの単色として一番下に敷く。
/// geneva は地の色を塗ってから重ねるので、下の色を読む合成モード（`multiply` など）は地の上で計算される。透明なら敷かない。
pub fn lay_background(layers: &mut Vec<Layer>, color_text: &str) -> Result<()> {
    let Some(fill) = color(color_text).filter(|c| c.a > 0.0) else {
        return Ok(());
    };
    let end = layers
        .iter()
        .map(|l| l.range().map(|r| r.end))
        .try_fold(RationalTime::ZERO, |m, e| e.map(|e| m.max(e)))?;
    if end > RationalTime::ZERO {
        let node =
            Node::new(SOLID_KIND, KIND_VERSION).with_param(COLOR_PARAM, ParamValue::Color(fill));
        let bg = Layer::new(
            BACKGROUND_ID,
            node,
            RationalTime::ZERO,
            RationalTime::ZERO,
            end,
        );
        layers.insert(0, bg);
    }
    Ok(())
}

/// クリップの `blend` をレイヤーの合成モードにする。名前は geneva と同じ式（W3C）を指す。知らない名前は数えて `normal`。
pub fn blend(report: &mut Report, clip: &Value) -> BlendMode {
    let Some(name) = clip.get("blend").and_then(Value::as_str) else {
        return BlendMode::Normal;
    };
    serde_json::from_value(Value::String(name.replace('-', "_"))).unwrap_or_else(|_| {
        report.add(Gap::Blend);
        BlendMode::Normal
    })
}

/// クリップの `effects` をレイヤーの効果にする。`out_per_src` はレイヤーの画像の 1 画素が出力で何画素か。
/// 描けるのは固定の半径のぼかし（`blur`）だけで、それ以外と動く半径は数えて除く。
pub fn effects(report: &mut Report, clip: &Value, id: &str, out_per_src: f64) -> Vec<Effect> {
    let list = clip
        .get("effects")
        .and_then(Value::as_array)
        .map_or(&[][..], Vec::as_slice);
    let mut out = Vec::new();
    for (i, e) in list.iter().enumerate() {
        let radius = e.get("radius").and_then(Value::as_f64);
        match (e.get("kind").and_then(Value::as_str), radius) {
            (Some("blur"), Some(r)) if out_per_src > 0.0 => out.push(Effect {
                id: Id::new(format!("{id}-fx{i}")),
                node: Node::new(BLUR_KIND, KIND_VERSION)
                    .with_param(RADIUS_PARAM, ParamValue::Number(r / out_per_src)),
                enabled: true,
            }),
            _ => report.add(Gap::Effects),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn 色の書き方をどれも読む() {
        assert_eq!(color("#000"), Some(Rgba::BLACK));
        assert_eq!(color("#FFFFFF"), Some(Rgba::WHITE));
        assert_eq!(color("#ffffff00"), Some(Rgba::TRANSPARENT));
        assert_eq!(
            color("#0B1230"),
            Some(Rgba::from_code8([0x0B, 0x12, 0x30, 255]))
        );
        assert_eq!(color("black"), Some(Rgba::BLACK));
        assert_eq!(color("#12"), None);
        assert_eq!(color("rgb(1, 2, 3)"), None);
    }

    #[test]
    fn 重ね方は_geneva_の名前をそのまま合成モードにする() {
        let mut report = Report::default();
        let mode = |b: &str, r: &mut Report| blend(r, &json!({"blend": b}));
        assert_eq!(mode("add", &mut report), BlendMode::Add);
        assert_eq!(mode("multiply", &mut report), BlendMode::Multiply);
        assert_eq!(mode("soft-light", &mut report), BlendMode::SoftLight);
        assert_eq!(blend(&mut report, &json!({})), BlendMode::Normal);
        assert_eq!(report.to_json(), json!([]));
        assert_eq!(mode("plus-darker", &mut report), BlendMode::Normal);
        assert_eq!(report.to_json()[0]["what"], "blend");
    }

    #[test]
    fn ぼかしは置いた後の半径をレイヤーの画像の画素に戻す() {
        let mut report = Report::default();
        let clip = json!({"effects": [{"kind": "blur", "radius": 36}, {"kind": "glow"}]});
        let fx = effects(&mut report, &clip, "bg-0", 0.5);
        assert_eq!(fx.len(), 1);
        assert_eq!(fx[0].node.kind, BLUR_KIND);
        let radius = fx[0].node.params[RADIUS_PARAM].value.as_number();
        assert_eq!(radius, Some(72.0));
        assert_eq!(report.to_json()[0]["what"], "effects");
    }

    #[test]
    fn 地の色は全部の長さの単色を一番下に敷き_透明なら敷かない() {
        let clip = |id: &str, end: i64| {
            let node = solid_node(&json!({"color": "#fff"})).unwrap();
            Layer::new(
                id,
                node,
                RationalTime::seconds(1),
                RationalTime::ZERO,
                RationalTime::seconds(end),
            )
        };
        let mut layers = vec![clip("a", 2), clip("b", 5)];
        lay_background(&mut layers, BLACK).unwrap();
        assert_eq!(layers[0].id.as_str(), BACKGROUND_ID);
        assert_eq!(layers[0].range().unwrap().end, RationalTime::seconds(6));
        let mut layers = vec![clip("a", 2)];
        lay_background(&mut layers, TRANSPARENT).unwrap();
        assert_eq!(layers.len(), 1);
    }

    #[test]
    fn 単色は色のパラメータを持つノードにする() {
        let node = solid_node(&json!({"kind": "solid", "color": "#EBCF9C"})).unwrap();
        assert_eq!(node.kind, SOLID_KIND);
        assert!(solid_node(&json!({"kind": "solid", "color": [{"t": 0}]})).is_none());
    }
}
