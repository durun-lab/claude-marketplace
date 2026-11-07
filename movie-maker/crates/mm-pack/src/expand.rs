//! マクロを 1 つのトランザクションに展開する。

use std::collections::BTreeMap;

use mm_doc::{Interpolation, Keyframe, Layer, Node, Param, Project};
use mm_ops::{Op, Transaction};
use mm_text_api::{TextAlign, TextShaper, TextStyle};
use mm_types::{MmError, ParamValue, RationalTime, Result};
use serde_json::{json, Value};

use crate::args::{millis, parse_time, Env};
use crate::layout::{place, Placement};
use crate::macros::{Step, TextStep};
use crate::pack::Pack;
use crate::screen::{fit_width, pin, Screens};
use crate::sticker;
use crate::style::{PlateSpec, TextStyleSpec};

/// 文字の種類のバージョン（`mm-nodes-basic` の種類はどれも 1）。
const KIND_VERSION: u32 = 1;
/// プレートのレイヤーの ID に足す後ろの部分。
const PLATE_SUFFIX: &str = ".plate";

/// マクロ `name` を引数 `args`（名前 → 文字の値）で展開する。記録の名前は「<マクロの表示名>（<パック>）」。
/// プレートの大きさは `shaper` で文字を組んで決める（描くときと同じシェーパーを渡すと、プレートが文字にぴったり合う）。
/// 知らないマクロは `NotFound`、引数や手順の誤りは `InvalidArgument`。
/// 部品はどのコンポジションにもテーマの本編の画面（`canvas`）の大きさで置く。適用する先のプロジェクトがあるときは
/// [`expand_for`] を使う。
pub fn expand(
    pack: &Pack,
    name: &str,
    args: &BTreeMap<String, String>,
    shaper: &dyn TextShaper,
) -> Result<Transaction> {
    expand_on(pack, name, args, shaper, Screens::new(&pack.theme, None))
}

/// [`expand`] と同じく展開し、部品を置くコンポジションの画面の大きさを `project` から引く。本編と大きさの違う
/// コンポジション（ショート）では、文字のスタイルの位置を基準点に近い画面の縁からの距離を保って写し、web の部品に
/// その画面の大きさを渡す。
pub fn expand_for(
    pack: &Pack,
    name: &str,
    args: &BTreeMap<String, String>,
    shaper: &dyn TextShaper,
    project: &Project,
) -> Result<Transaction> {
    expand_on(
        pack,
        name,
        args,
        shaper,
        Screens::new(&pack.theme, Some(project)),
    )
}

fn expand_on(
    pack: &Pack,
    name: &str,
    args: &BTreeMap<String, String>,
    shaper: &dyn TextShaper,
    screens: Screens<'_>,
) -> Result<Transaction> {
    let def = pack.find(name)?;
    let env = Env::new(def, &pack.theme, args)?;
    let mut ops = Vec::new();
    for (i, step) in def.steps.iter().enumerate() {
        let values = match step {
            Step::Op(raw) => vec![env.fill_json(raw)?],
            Step::Text(t) => {
                let input = TextInput::read(pack, &env, t)?;
                text_ops(pack, &input, shaper, screens)?
            }
            Step::Mixer(m) => vec![mixer_op(pack, &env.fill_text(&m.comp)?)?],
            Step::Transition(t) => vec![crate::transition::transition_op(pack, &env, t)?],
            Step::Web(w) => vec![crate::web::web_op(pack, &env, w, screens)?],
        };
        for value in values {
            let op: Op = serde_json::from_value(value).map_err(|e| {
                MmError::InvalidArgument(format!("{name} の手順 {}: 操作の形が違う: {e}", i + 1))
            })?;
            ops.push(op);
        }
    }
    let title = format!("{}（{}）", def.title, pack.manifest.name);
    Ok(Transaction::new(title, ops))
}

/// `mixer.json` を `comp` に当てる `setMixer`。パックに `mixer.json` が無ければ `InvalidArgument`。
fn mixer_op(pack: &Pack, comp: &str) -> Result<Value> {
    let mixer = pack.mixer.as_ref().ok_or_else(|| {
        MmError::InvalidArgument(format!(
            "パック {} に mixer.json が無い（mixer 手順が要る）",
            pack.manifest.name
        ))
    })?;
    Ok(json!({"op": "setMixer", "composition": comp, "mixer": serde_json::to_value(mixer)?}))
}

/// 引数を埋めて読んだ文字の手順。
struct TextInput<'a> {
    style: &'a TextStyleSpec,
    comp: String,
    id: String,
    text: String,
    at: RationalTime,
    dur: RationalTime,
    attach: Option<String>,
}

impl<'a> TextInput<'a> {
    fn read(pack: &'a Pack, env: &Env, step: &TextStep) -> Result<Self> {
        let style = pack.styles.get(&step.style).ok_or_else(|| {
            MmError::InvalidArgument(format!(
                "styles.json に文字のスタイル {} が無い",
                step.style
            ))
        })?;
        let id = env.fill_text(&step.layer)?;
        let mut text = env.fill_text(&step.text)?;
        if style.corrections {
            text = correct(&text, &pack.corrections);
        }
        let dur = parse_time(&env.fill_text(&step.dur)?)?;
        if dur <= RationalTime::ZERO {
            return Err(MmError::InvalidArgument(format!("{id}: 長さは正にする")));
        }
        let attach = match &step.attach {
            Some(raw) => Some(env.fill_text(raw)?).filter(|s| !s.is_empty()),
            None => None,
        };
        Ok(Self {
            style,
            comp: env.fill_text(&step.comp)?,
            id,
            text,
            at: parse_time(&env.fill_text(&step.at)?)?,
            dur,
            attach,
        })
    }
}

/// 文字のスタイルで文字を置く操作。プレートがあればプレートのレイヤーを下に、文字のレイヤーを上に足す。
fn text_ops(
    pack: &Pack,
    input: &TextInput<'_>,
    shaper: &dyn TextShaper,
    screens: Screens<'_>,
) -> Result<Vec<Value>> {
    let style = input.style;
    let color = pack.theme.color(&style.color)?;
    let font = style
        .font
        .as_deref()
        .map(|r| pack.theme.font(r))
        .transpose()?;
    let (canvas, screen) = (screens.canvas(), screens.of(&input.comp));
    let max_width = style.max_width.map(|w| fit_width(w, canvas[0], screen[0]));
    let bounds = shaper
        .shape(&input.text, &text_style(style, color, font, max_width))?
        .bounds;
    let padding = style.plate.as_ref().map_or([0.0, 0.0], |p| p.padding);
    let block = [f64::from(bounds.width), f64::from(bounds.height)];
    let position = pin(style.position, style.anchor, canvas, screen);
    let spot = place(block, padding, position, style.anchor);
    let frame = Frame {
        at: input.at,
        dur: input.dur,
        opacity: fade(style.fade_in, style.fade_out, input.at, input.dur)?,
        attach: input.attach.clone(),
        spot,
        tilt: style.tilt,
    };
    let mut layers = Vec::new();
    if let Some(plate) = &style.plate {
        let node = plate_node(pack, plate, spot.plate, screen)?;
        layers.push(frame.layer(format!("{}{PLATE_SUFFIX}", input.id), node));
    }
    // `text` は画像の左上を `position` に `bounds` の位置を足して決めるので、字の塊の左上が `text_origin` に来るよう引いておく
    let position = [
        spot.text_origin[0] - f64::from(bounds.x),
        spot.text_origin[1] - f64::from(bounds.y),
    ];
    let mut node = text_node(&input.text, style.size, color, position, max_width);
    if let Some(family) = font {
        node = node.with_param("font", ParamValue::Text(family.to_owned()));
    }
    layers.push(frame.layer(input.id.clone(), node));
    layers
        .into_iter()
        .map(|layer| {
            let layer = serde_json::to_value(layer)?;
            Ok(json!({"op": "addLayer", "composition": input.comp, "layer": layer}))
        })
        .collect()
}

/// `text` の種類が描くときと同じスタイル（字の大きさ、色、折り返しの幅、中央揃え、行の高さの既定）。
/// 測るときのスタイル。書体は描くとき（`text` の `font`）と同じ family にする。`max_width` は画面に合わせた折り返しの幅。
fn text_style(
    style: &TextStyleSpec,
    color: mm_types::Rgba,
    font: Option<&str>,
    max_width: Option<f64>,
) -> TextStyle {
    let defaults = TextStyle::default();
    TextStyle {
        font_family: font.map_or(defaults.font_family.clone(), str::to_owned),
        size_px: style.size as f32,
        color,
        max_width: max_width.map(|w| w as f32),
        align: TextAlign::Center,
        ..defaults
    }
}

/// プレートを描く `shader.wgsl` のノード。`rect` は左上と大きさ、`screen` は置くコンポジションの画面（画素）。
fn plate_node(pack: &Pack, plate: &PlateSpec, rect: [f64; 4], screen: [f64; 2]) -> Result<Node> {
    let mut node = Node::new("shader.wgsl", KIND_VERSION)
        .with_param("code", ParamValue::Text(sticker::code(&pack.theme, plate)?));
    for (name, value) in sticker::shape_params(plate, rect, screen) {
        node = node.with_param(name, value);
    }
    Ok(node)
}

/// プレートと文字のレイヤーが共有する時間と置き方。
struct Frame {
    at: RationalTime,
    dur: RationalTime,
    opacity: Param,
    attach: Option<String>,
    spot: Placement,
    tilt: f64,
}

impl Frame {
    fn layer(&self, id: String, node: Node) -> Layer {
        let mut layer = Layer::new(id, node, self.at, RationalTime::ZERO, self.dur);
        layer.opacity = self.opacity.clone();
        layer.attach_to = self.attach.as_deref().map(mm_types::Id::new);
        if self.tilt != 0.0 {
            // 回転の中心はコンポジションの画素で、プレートの中心。プレートと文字が同じ中心で回るので重なりがずれない。
            let [x, y, w, h] = self.spot.plate;
            layer.transform.rotation = Param::constant(ParamValue::Number(self.tilt));
            layer.transform.anchor = Some(Param::constant(ParamValue::Vec2([
                x + w / 2.0,
                y + h / 2.0,
            ])));
        }
        layer
    }
}

/// 中央揃えの `text` のノード。`max_width` が無ければ折り返さない（0）。
fn text_node(
    text: &str,
    size: f64,
    color: mm_types::Rgba,
    position: [f64; 2],
    max_width: Option<f64>,
) -> Node {
    Node::new("text", KIND_VERSION)
        .with_param("text", ParamValue::Text(text.to_string()))
        .with_param("size", ParamValue::Number(size))
        .with_param("color", ParamValue::Color(color))
        .with_param("position", ParamValue::Vec2(position))
        .with_param("max_width", ParamValue::Number(max_width.unwrap_or(0.0)))
        .with_param("align", ParamValue::Text("center".into()))
}

/// 出る・消える動き（秒）の不透明度。動きの長さの和が出ている長さを超えるときは、超えた秒数を添えて `InvalidArgument`
/// にする（動きが黙って消えない）。
pub(crate) fn fade(
    fade_in: f64,
    fade_out: f64,
    at: RationalTime,
    dur: RationalTime,
) -> Result<Param> {
    let constant = Param::constant(ParamValue::Number(1.0));
    if fade_in <= 0.0 && fade_out <= 0.0 {
        return Ok(constant);
    }
    let fade_in = millis(fade_in)?;
    let fade_out = millis(fade_out)?;
    let total = fade_in.checked_add(fade_out)?;
    if total > dur {
        return Err(MmError::InvalidArgument(format!(
            "出る動き {}s と消える動き {}s の和が長さ {}s を {}s 超える",
            fade_in.to_f64(),
            fade_out.to_f64(),
            dur.to_f64(),
            total.checked_sub(dur)?.to_f64()
        )));
    }
    let end = at.checked_add(dur)?;
    let key =
        |t: RationalTime, v: f64| Keyframe::new(t, ParamValue::Number(v), Interpolation::Linear);
    let mut keys = vec![
        key(
            at,
            if fade_in > RationalTime::ZERO {
                0.0
            } else {
                1.0
            },
        ),
        key(at.checked_add(fade_in)?, 1.0),
        key(end.checked_sub(fade_out)?, 1.0),
        key(
            end,
            if fade_out > RationalTime::ZERO {
                0.0
            } else {
                1.0
            },
        ),
    ];
    keys.dedup_by(|a, b| a.time == b.time);
    Ok(Param {
        value: ParamValue::Number(1.0),
        keyframes: keys,
    })
}

/// 表記の置き換え。先頭から 1 回だけ走査し、各位置でいちばん長い語を置き換える。
/// 置き換えた後の文字は走査し直さないので、ある語の置き換え先が別の語にまた置き換わることはない。
fn correct(text: &str, corrections: &BTreeMap<String, String>) -> String {
    let mut pairs: Vec<(&str, &str)> = corrections
        .iter()
        .filter(|(from, _)| !from.is_empty())
        .map(|(from, to)| (from.as_str(), to.as_str()))
        .collect();
    pairs.sort_by(|a, b| b.0.len().cmp(&a.0.len()).then(a.0.cmp(b.0)));
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(c) = rest.chars().next() {
        match pairs.iter().find(|(from, _)| rest.starts_with(from)) {
            Some((from, to)) => {
                out.push_str(to);
                rest = &rest[from.len()..];
            }
            None => {
                out.push(c);
                rest = &rest[c.len_utf8()..];
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(a, b)| (a.to_string(), b.to_string()))
            .collect()
    }

    #[test]
    fn 長い語から置き換える() {
        let t = table(&[("ボーカロイド6", "VOCALOID6"), ("ボーカロイド", "VOCALOID")]);
        assert_eq!(
            correct("ボーカロイド6とボーカロイド", &t),
            "VOCALOID6とVOCALOID"
        );
    }

    #[test]
    fn 置き換えた後の文字は置き換え直さない() {
        let t = table(&[("クロード", "Claude"), ("Cl", "CL")]);
        assert_eq!(correct("クロードとCl", &t), "ClaudeとCL");
    }
}
