//! サーフェスの配色と字と、WebView の列の幅。WebView と同じ `ui/theme` の theme.json の色と字を Timeline に使い、
//! 列の幅で割り付けを決める。サーフェスは映像の周りなので、ダークのモードを使う。

use std::sync::LazyLock;

use mm_ui_layout::palette::{parse_hex, Palette};
use mm_ui_layout::UiFont;
use serde_json::Value;

/// WebView と共有するテーマ。ビルドの時点で埋め込み、起動の場所に依らない。
const THEME_JSON: &str = include_str!("../../../../ui/theme/theme.json");
/// サーフェスに使うモード。
const MODE: &str = "dark";
/// theme.json を読めないときの WebView の左の列と右の列の既定の幅（論理ピクセル）。
const DEFAULT_COLUMNS: (f64, f64) = (260.0, 400.0);

/// WebView の左の列と右の列の既定の幅（論理ピクセル）。theme.json の `left-column` と `right-column`。大きさが変わる
/// たびに引くので、最初の 1 回だけ読む。
static COLUMNS: LazyLock<(f64, f64)> = LazyLock::new(|| {
    columns_from(THEME_JSON).unwrap_or_else(|e| {
        tracing::warn!(error = %e, "theme.json の列の幅を読めないので既定の幅にする");
        DEFAULT_COLUMNS
    })
});

/// WebView の 2 列（Tasks などの左の列と、Inspector などの右の列）の既定の幅の和（論理ピクセル）。
pub fn webview_width() -> f64 {
    COLUMNS.0 + COLUMNS.1
}

/// WebView の左の列と右の列の既定の幅（論理ピクセル）。右の列は広げられる上限でもある（キーフレームの欄が要る幅）。
pub fn columns() -> (f64, f64) {
    *COLUMNS
}

fn columns_from(json: &str) -> Result<(f64, f64), String> {
    let theme: Value = serde_json::from_str(json).map_err(|e| e.to_string())?;
    let size = |name: &str| {
        theme
            .pointer(&format!("/shape/size/{name}"))
            .and_then(Value::as_f64)
            .ok_or_else(|| format!("shape.size.{name} が無い"))
    };
    Ok((size("left-column")?, size("right-column")?))
}

/// theme.json のダークのモードから作った Timeline の色。読めなければ既定の色。
pub fn surface_palette() -> Palette {
    palette_from(THEME_JSON, MODE).unwrap_or_else(|e| {
        tracing::warn!(error = %e, "theme.json を読めないので既定の色にする");
        Palette::DEFAULT
    })
}

/// theme.json の `font` から作った Timeline の字（`family` の先頭、`size` の `sm` と `base`）。読めなければ既定の字。
pub fn surface_font() -> UiFont {
    font_from(THEME_JSON).unwrap_or_else(|e| {
        tracing::warn!(error = %e, "theme.json の字を読めないので既定の字にする");
        UiFont::default()
    })
}

fn font_from(json: &str) -> Result<UiFont, String> {
    let theme: Value = serde_json::from_str(json).map_err(|e| e.to_string())?;
    let font = theme.get("font").ok_or("font が無い")?;
    let family = font
        .pointer("/family/0")
        .and_then(Value::as_str)
        .ok_or("font.family が無い")?;
    let size = |name: &str| {
        font.pointer(&format!("/size/{name}"))
            .and_then(Value::as_f64)
            .map(|v| v as f32)
    };
    let d = UiFont::default();
    Ok(UiFont {
        family: family.to_owned(),
        small: size("sm").unwrap_or(d.small),
        base: size("base").unwrap_or(d.base),
    })
}

fn palette_from(json: &str, mode: &str) -> Result<Palette, String> {
    let theme: Value = serde_json::from_str(json).map_err(|e| e.to_string())?;
    let colors = theme
        .pointer(&format!("/modes/{mode}/color"))
        .ok_or_else(|| format!("モード {mode} の色が無い"))?;
    Ok(Palette::themed(|name| {
        colors.get(name).and_then(Value::as_str).and_then(parse_hex)
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ダークのモードの_surface_の色を使う() {
        let p = palette_from(THEME_JSON, MODE).expect("読める");
        let theme: Value = serde_json::from_str(THEME_JSON).unwrap();
        let surface = theme["modes"]["dark"]["color"]["surface"].as_str().unwrap();
        assert_eq!(Some(p.panel), parse_hex(surface));
        assert_ne!(p, Palette::DEFAULT);
    }

    #[test]
    fn 無いモードはエラー() {
        assert!(palette_from(THEME_JSON, "sepia").is_err());
    }

    #[test]
    fn 字は_family_の先頭と_sm_と_base_の大きさ() {
        let font = font_from(THEME_JSON).expect("読める");
        let theme: Value = serde_json::from_str(THEME_JSON).unwrap();
        assert_eq!(font.family, theme["font"]["family"][0].as_str().unwrap());
        assert_eq!(
            f64::from(font.small),
            theme["font"]["size"]["sm"].as_f64().unwrap()
        );
        assert_eq!(
            f64::from(font.base),
            theme["font"]["size"]["base"].as_f64().unwrap()
        );
        assert!(font_from("{}").is_err());
    }

    #[test]
    fn webview_の幅は左の列と右の列の幅の和() {
        let theme: Value = serde_json::from_str(THEME_JSON).unwrap();
        let size = |name: &str| theme["shape"]["size"][name].as_f64().unwrap();
        assert_eq!(
            size("right-column"),
            400.0,
            "右の列はキーフレームの欄が行の右に出る幅"
        );
        assert_eq!(webview_width(), size("left-column") + size("right-column"));
        assert_eq!(webview_width(), DEFAULT_COLUMNS.0 + DEFAULT_COLUMNS.1);
        assert_eq!(columns(), (size("left-column"), size("right-column")));
        assert!(columns_from("{}").is_err());
    }
}
