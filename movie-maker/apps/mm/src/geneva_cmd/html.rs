//! `html` の source（geneva のマークアップ。タイトルの札やトランジションの絵）を `web.cef` のレイヤーにする。
//!
//! geneva はマークアップの CSS アニメーションをクリップの時刻で止めて描く。`web.cef` はページの
//! `window.mmSetTime(t)` に時刻を渡すので、ページの全部のアニメーション（Web Animations）をその時刻に止めるスクリプトを足す。
//! 箱は画面の大きさのページとして描き、箱のアンカーを位置へ動かす平行移動だけをレイヤーの変形にする。

use mm_doc::{Node, Param, Transform};
use mm_types::{ParamValue, Result};
use serde_json::Value;

use super::syntax;

/// `web.cef` の種類の名前とバージョン、パラメータ（`mm-nodes-cef` の `KIND_ID`、`KIND_VERSION`、`HTML_PARAM`、`STATIC_PARAM`）。
const WEB_KIND: &str = "web.cef";
const WEB_KIND_VERSION: u32 = 1;
const HTML_PARAM: &str = "html";
const STATIC_PARAM: &str = "static";
/// 箱の大きさを中身に合わせる指定。中身の大きさは組んでみないと分からないので、画面の大きさとして扱う。
const AUTO: &str = "auto";

/// マークアップの前に置く文書の頭。geneva の箱は余白なしの透明なページ。
/// geneva は和文の字の間で折り返さない（幅 560 の欄に 150 px の 4 字を 1 行で描く）ので、`keep-all` で合わせる。
const HEAD: &str = "<!doctype html><html><head><meta charset=\"utf-8\">\
<style>html, body { margin: 0; background: transparent; overflow: hidden; word-break: keep-all; }</style></head><body>";
/// マークアップの後ろに置く、全部のアニメーションを時刻 `t`（秒）に止めるスクリプト。
const CLOCK: &str = "<script>window.mmSetTime = (t) => {\
for (const a of document.getAnimations()) { a.pause(); a.currentTime = t * 1000; } };</script></body></html>";

/// `html` の source を `web.cef` のノードと変形にする。描けない形（素材のファイルを指す、画面と違う大きさの箱、
/// 動く変形）なら `None`。
pub fn convert(clip: &Value, frame: [f64; 2]) -> Result<Option<(Node, Transform)>> {
    let source = clip.get("source").unwrap_or(&Value::Null);
    let Some(markup) = source.get("html").and_then(Value::as_str) else {
        return Ok(None);
    };
    if !box_is_frame(source, frame)? {
        return Ok(None);
    }
    let Some(offset) = offset(clip, frame)? else {
        return Ok(None);
    };
    let css = source.get("css").and_then(Value::as_str);
    let node = Node::new(WEB_KIND, WEB_KIND_VERSION)
        .with_param(HTML_PARAM, ParamValue::Text(page(markup, css)))
        .with_param(STATIC_PARAM, ParamValue::Bool(false));
    let transform = Transform {
        anchor: Some(Param::constant(ParamValue::Vec2([0.0, 0.0]))),
        position: Param::constant(ParamValue::Vec2(offset)),
        ..Transform::default()
    };
    Ok(Some((node, transform)))
}

/// 箱の幅と高さが画面と同じか（省略、`auto`、画面と同じ長さ）。
fn box_is_frame(source: &Value, frame: [f64; 2]) -> Result<bool> {
    for (key, full) in [("width", frame[0]), ("height", frame[1])] {
        let side = match source.get(key) {
            None => continue,
            Some(Value::String(s)) if s.trim() == AUTO => continue,
            Some(v) => syntax::length(v, full)?,
        };
        if (side - full).abs() > f64::EPSILON {
            return Ok(false);
        }
    }
    Ok(true)
}

/// 箱の左上の位置（位置 − アンカー）。拡大、回転、キーフレームがあれば `None`。
fn offset(clip: &Value, frame: [f64; 2]) -> Result<Option<[f64; 2]>> {
    let t = clip.get("transform").unwrap_or(&Value::Null);
    let animated = |k: &str| t.get(k).is_some_and(|v| v.get("keyframes").is_some());
    let unit_scale = t.get("scale").is_none_or(|s| s.as_f64() == Some(1.0));
    let no_rotation = t.get("rotation").is_none_or(|r| r.as_f64() == Some(0.0));
    if animated("position") || animated("anchor") || !unit_scale || !no_rotation {
        return Ok(None);
    }
    let center = [frame[0] / 2.0, frame[1] / 2.0];
    let position = t
        .get("position")
        .map(|p| syntax::point(p, frame))
        .transpose()?
        .unwrap_or(center);
    let anchor = t
        .get("anchor")
        .map(|a| syntax::point(a, frame))
        .transpose()?
        .unwrap_or(center);
    Ok(Some([position[0] - anchor[0], position[1] - anchor[1]]))
}

/// マークアップを 1 枚のページにする。`css` はマークアップの `<style>` の後ろに足す（geneva と同じく後勝ち）。
fn page(markup: &str, css: Option<&str>) -> String {
    let style = css
        .map(|c| format!("<style>{c}</style>"))
        .unwrap_or_default();
    format!("{HEAD}{markup}{style}{CLOCK}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const FRAME: [f64; 2] = [1920.0, 1080.0];

    fn html(node: &Node) -> String {
        node.params[HTML_PARAM].value.as_text().unwrap().to_owned()
    }

    #[test]
    fn 画面の大きさの箱は時刻を止めるスクリプトつきのページになる() {
        let clip = json!({
            "source": {"kind": "html", "html": "<div class=\"stage\"></div>", "width": "auto", "height": "auto"},
            "transform": {"position": "50% 50%"}
        });
        let (node, transform) = convert(&clip, FRAME).unwrap().unwrap();
        assert_eq!(node.kind, WEB_KIND);
        let page = html(&node);
        assert!(page.starts_with("<!doctype html>"));
        assert!(page.contains("<div class=\"stage\"></div>"));
        assert!(page.contains("a.currentTime = t * 1000"));
        assert_eq!(node.params[STATIC_PARAM].value.as_bool(), Some(false));
        assert_eq!(transform.position.value.as_vec2(), Some([0.0, 0.0]));
    }

    #[test]
    fn css_はマークアップの後ろに足す() {
        let clip =
            json!({"source": {"kind": "html", "html": "<p>a</p>", "css": "p { color: red }"}});
        let (node, _) = convert(&clip, FRAME).unwrap().unwrap();
        let page = html(&node);
        assert!(page.find("<p>a</p>").unwrap() < page.find("p { color: red }").unwrap());
    }

    #[test]
    fn 左上のアンカーを動かした箱は平行移動にする() {
        let clip = json!({
            "source": {"kind": "html", "html": "<p>a</p>"},
            "transform": {"position": {"x": 100, "y": 50}, "anchor": "0 0"}
        });
        let (_, transform) = convert(&clip, FRAME).unwrap().unwrap();
        assert_eq!(transform.position.value.as_vec2(), Some([100.0, 50.0]));
    }

    #[test]
    fn 画面と違う大きさの箱と素材のファイルは変換しない() {
        let small = json!({"source": {"kind": "html", "html": "<p>a</p>", "width": 560}});
        assert!(convert(&small, FRAME).unwrap().is_none());
        let file = json!({"source": {"kind": "html", "asset": "card"}});
        assert!(convert(&file, FRAME).unwrap().is_none());
    }
}
