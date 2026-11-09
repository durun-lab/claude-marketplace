//! サーフェスの配色と字。WebView と同じ `ui/theme` の theme.json の色と字を、Timeline に使う。
//! サーフェスは映像の周りなので、ダークのモードを使う。

use mm_ui_layout::palette::{parse_hex, Palette};
use mm_ui_layout::UiFont;
use serde_json::Value;

/// WebView と共有するテーマ。ビルドの時点で埋め込み、起動の場所に依らない。
const THEME_JSON: &str = include_str!("../../../../ui/theme/theme.json");
/// サーフェスに使うモード。
const MODE: &str = "dark";

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
}
