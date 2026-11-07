//! `web` 手順の展開。パックの `web/<page>.html` を `web.cef` のノードのレイヤーにする。
//! ページの HTML はノードのパラメータにコピーするので、プロジェクトはパックのフォルダーが無くても同じに描ける。

use mm_doc::{Layer, Node};
use mm_types::{MmError, ParamValue, RationalTime, Result};
use serde_json::{json, Map, Value};

use crate::args::{parse_time, Env};
use crate::expand::fade;
use crate::macros::WebStep;
use crate::pack::Pack;

/// `web.cef` の種類の名前とバージョン（`mm-nodes-cef` の `KIND_ID` と `KIND_VERSION`）。
const WEB_KIND: &str = "web.cef";
const WEB_KIND_VERSION: u32 = 1;

/// web の部品が使う、テーマの色を CSS 変数（`--名前`）にして `var(--名前)` の参照の表を返す関数。
/// 色の当て方を 1 か所に置き、部品ごとの写しを持たない。
const THEME_SCRIPT: &str = "<script>const mmThemeColors = (theme) => {
  const out = {};
  for (const [k, v] of Object.entries(theme.colors || {})) {
    document.documentElement.style.setProperty(`--${k}`, v);
    out[k] = `var(--${k})`;
  }
  return out;
};</script>
";

/// 最初の `<script`（大文字小文字を問わない）の前に [`THEME_SCRIPT`] を差し込む。ページの script より前に定義が要る。
/// script の無いページは関数を使わないので変えない（doctype より前に入れると Chromium が互換モードで組む）。
/// 差し込み済みの HTML には足さない（`const` を 2 回宣言するとページの script が動かない）。
fn with_theme_script(html: &str) -> String {
    if html.contains(THEME_SCRIPT) {
        return html.to_string();
    }
    // ASCII の小文字化はバイトの位置を変えないので、見つけた位置をそのまま元の HTML に使える。
    match html.to_ascii_lowercase().find("<script") {
        Some(at) => format!("{}{THEME_SCRIPT}{}", &html[..at], &html[at..]),
        None => html.to_string(),
    }
}

/// `web` 手順を `addLayer` 1 つにする。
pub(crate) fn web_op(pack: &Pack, env: &Env, step: &WebStep) -> Result<Value> {
    let page = env.fill_text(&step.page)?;
    let html = pack.pages.get(&page).ok_or_else(|| {
        MmError::InvalidArgument(format!(
            "web/{page}.html がパック {} に無い",
            pack.manifest.name
        ))
    })?;
    let id = env.fill_text(&step.layer)?;
    let at = parse_time(&env.fill_text(&step.at)?)?;
    let dur = parse_time(&env.fill_text(&step.dur)?)?;
    if dur <= RationalTime::ZERO {
        return Err(MmError::InvalidArgument(format!("{id}: 長さは正にする")));
    }
    let node = Node::new(WEB_KIND, WEB_KIND_VERSION)
        .with_param("html", ParamValue::Text(with_theme_script(html)))
        .with_param("data", ParamValue::Json(page_data(pack, env, step)?))
        .with_param("static", ParamValue::Bool(!step.animated));
    let mut layer = Layer::new(id, node, at, RationalTime::ZERO, dur);
    layer.opacity = fade(step.fade_in, step.fade_out, at, dur)?;
    if let Some(raw) = &step.attach {
        let cut = env.fill_text(raw)?;
        layer.attach_to = (!cut.is_empty()).then(|| mm_types::Id::new(cut));
    }
    let comp = env.fill_text(&step.comp)?;
    Ok(json!({"op": "addLayer", "composition": comp, "layer": serde_json::to_value(layer)?}))
}

/// 引数を埋めた `data` に、テーマの色（`#RRGGBB`）、書体、画面の大きさを `theme` として足す。
/// ページは色と書体をテーマから引くので、パックの決まりが 1 か所に留まる。
fn page_data(pack: &Pack, env: &Env, step: &WebStep) -> Result<Value> {
    let mut data = match env.fill_json(&step.data)? {
        Value::Object(map) => map,
        Value::Null => Map::new(),
        other => {
            return Err(MmError::InvalidArgument(format!(
                "web 手順の data はオブジェクトにする: {other}"
            )))
        }
    };
    let theme = &pack.theme;
    data.insert(
        "theme".into(),
        json!({
            "colors": theme.colors,
            "fonts": theme.fonts,
            "canvas": {"width": theme.canvas.width, "height": theme.canvas.height},
        }),
    );
    Ok(Value::Object(data))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn テーマの関数はページの最初の_script_の前に1回だけ入る() {
        let page = "<!doctype html><html><head><style>p{}</style></head><body>\
                    <SCRIPT>mmThemeColors({})</SCRIPT><script>x()</script></body></html>";
        let out = with_theme_script(page);
        let theme = out.find(THEME_SCRIPT).expect("差し込む");
        assert_eq!(
            theme,
            page.find("<SCRIPT>").unwrap(),
            "大文字の script も探す"
        );
        assert!(out.starts_with("<!doctype html>"), "doctype は先頭のまま");
        assert_eq!(with_theme_script(&out), out, "2 回目は足さない");
    }

    #[test]
    fn script_の無いページは変えない() {
        let page = "<!doctype html><html><body><p>静止</p></body></html>";
        assert_eq!(with_theme_script(page), page);
    }
}
