//! プロジェクトの書体をページに渡す材料。`Prepare` の `fonts` から `@font-face` を作ってページの HTML に差し込み、
//! ファイルは [`FONT_ORIGIN`] の下の URL で配る（配るのは `cef` feature のスキームハンドラー）。
//!
//! `data:` で埋めず URL で配るのは、vlog の 3 書体で 25 MB あり、埋めるとページを読むたびに数十 MB の CSS を
//! 解くことになるため。URL なら Chromium は使う family と太さのファイルだけを取りに来る。

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use mm_host_api::FONT_ORIGIN;
use serde::Deserialize;

/// `Prepare` の `fonts` の 1 件（`mm_text_api::FontFile` の JSON）。
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct PageFont {
    pub path: PathBuf,
    #[serde(default)]
    pub content_hash: String,
    /// ファイルの中の名前に足して宣言する family の名前。
    #[serde(default)]
    pub family: Option<String>,
}

/// 1 つのファイルの `@font-face` の材料。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FaceMeta {
    /// 宣言する family の名前（足す名前、ファイルの中の名前の順。重複なし）。
    pub families: Vec<String>,
    /// CSS の `font-weight`（OS/2 の usWeightClass）。
    pub weight: u16,
    pub italic: bool,
}

/// 書体のファイルを読んで `@font-face` の材料にする。書体として読めなければ `None`。
pub fn face_meta(data: &[u8], declared: Option<&str>) -> Option<FaceMeta> {
    let face = ttf_parser::Face::parse(data, 0).ok()?;
    let mut families: Vec<String> = declared.into_iter().map(str::to_owned).collect();
    for id in [
        ttf_parser::name_id::TYPOGRAPHIC_FAMILY,
        ttf_parser::name_id::FAMILY,
    ] {
        let found = face
            .names()
            .into_iter()
            .filter(|n| n.name_id == id && n.is_unicode())
            .find_map(|n| n.to_string());
        if let Some(name) = found {
            if !families.contains(&name) {
                families.push(name);
            }
            break;
        }
    }
    Some(FaceMeta {
        families,
        weight: face.weight().to_number(),
        italic: face.is_italic(),
    })
}

/// URL の名前。中身のハッシュ（空なら場所の FNV-1a）で、URL に使えない文字を入れない。
pub fn font_key(font: &PageFont) -> String {
    if !font.content_hash.is_empty() && font.content_hash.chars().all(|c| c.is_ascii_alphanumeric())
    {
        return font.content_hash.clone();
    }
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in font.path.to_string_lossy().bytes() {
        h = (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3);
    }
    format!("p{h:016x}")
}

/// CSS の文字列の中に置ける形にする（引用符と逆斜線と改行を外す）。
fn css_string(s: &str) -> String {
    s.chars()
        .filter(|c| !matches!(c, '"' | '\\' | '\n' | '\r'))
        .collect()
}

/// 1 つのファイルの `@font-face`（宣言する family の名前ごとに 1 つ）。
pub fn face_css(key: &str, meta: &FaceMeta) -> String {
    let style = if meta.italic { "italic" } else { "normal" };
    meta.families
        .iter()
        .map(|family| {
            format!(
                "@font-face{{font-family:\"{}\";src:url(\"{FONT_ORIGIN}/{key}\");\
                 font-weight:{};font-style:{style};font-display:block;}}",
                css_string(family),
                meta.weight
            )
        })
        .collect()
}

/// `style` を `</head>` の前（無ければ末尾）に差し込む。
pub fn inject_style(html: &str, css: &str) -> String {
    if css.is_empty() {
        return html.to_owned();
    }
    let tag = format!("<style>{css}</style>");
    let at = html
        .to_ascii_lowercase()
        .find("</head>")
        .unwrap_or(html.len());
    let mut out = String::with_capacity(html.len() + tag.len());
    out.push_str(&html[..at]);
    out.push_str(&tag);
    out.push_str(&html[at..]);
    out
}

/// URL の名前 → ファイルの場所と、読んだ材料。CEF の IO スレッドのハンドラーと本体のスレッドが共有する。
#[derive(Debug, Default, Clone)]
pub struct FontTable {
    inner: Arc<Mutex<Table>>,
}

#[derive(Debug, Default)]
struct Table {
    paths: HashMap<String, PathBuf>,
    /// 読んだ材料。ページを読み直すたびにファイルを解かないよう、ホストが生きている間は覚えておく。
    metas: HashMap<String, Option<FaceMeta>>,
}

impl FontTable {
    /// `fonts` に入れ替え、ページに差し込む CSS を返す。読めないファイルは飛ばし、そのページは OS の書体で描く。
    pub fn replace(&self, fonts: &[PageFont]) -> String {
        let mut table = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        table.paths.clear();
        let mut css = String::new();
        for font in fonts {
            let key = font_key(font);
            let meta = table
                .metas
                .entry(key.clone())
                .or_insert_with(|| read_meta(font))
                .clone();
            if let Some(meta) = meta {
                css.push_str(&face_css(&key, &meta));
                table.paths.insert(key, font.path.clone());
            }
        }
        css
    }

    /// URL の名前のファイル。
    pub fn path(&self, key: &str) -> Option<PathBuf> {
        let table = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        table.paths.get(key).cloned()
    }

    /// URL からファイルの中身。起点の外か、知らない名前か、読めなければ `None`。
    pub fn read_url(&self, url: &str) -> Option<Vec<u8>> {
        let key = url.strip_prefix(FONT_ORIGIN)?.trim_start_matches('/');
        std::fs::read(self.path(key)?).ok()
    }
}

fn read_meta(font: &PageFont) -> Option<FaceMeta> {
    match std::fs::read(&font.path) {
        Ok(data) => face_meta(&data, font.family.as_deref()),
        Err(e) => {
            eprintln!("mm-host-cef: 書体 {} を読めない: {e}", font.path.display());
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn style_は_head_の終わりの前に入る() {
        let out = inject_style(
            "<html><head><title>x</title></HEAD><body></body></html>",
            "a{}",
        );
        assert_eq!(
            out,
            "<html><head><title>x</title><style>a{}</style></HEAD><body></body></html>"
        );
        assert_eq!(
            inject_style("<p>x</p>", "a{}"),
            "<p>x</p><style>a{}</style>"
        );
        assert_eq!(inject_style("<p>x</p>", ""), "<p>x</p>");
    }

    #[test]
    fn family_の名前ごとに宣言し_引用符を外す() {
        let meta = FaceMeta {
            families: vec!["A \"B\"".into(), "C".into()],
            weight: 900,
            italic: false,
        };
        let css = face_css("k1", &meta);
        assert_eq!(css.matches("@font-face").count(), 2);
        assert!(css.contains("font-family:\"A B\""), "{css}");
        assert!(css.contains(&format!("url(\"{FONT_ORIGIN}/k1\")")), "{css}");
        assert!(css.contains("font-weight:900"), "{css}");
    }

    #[test]
    fn url_の名前はハッシュか場所から作り_記号を入れない() {
        let mut font = PageFont {
            path: "a/b.ttf".into(),
            content_hash: "ab12".into(),
            family: None,
        };
        assert_eq!(font_key(&font), "ab12");
        font.content_hash = "../x".into();
        assert!(font_key(&font).chars().all(|c| c.is_ascii_alphanumeric()));
    }

    #[test]
    fn パックの書体からフォルダーの名前と中の名前と太さを読む() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../packs/vlog/fonts/M PLUS Rounded 1c/MPLUSRounded1c-Black.ttf");
        let data = std::fs::read(path).unwrap();
        let meta = face_meta(&data, Some("M PLUS Rounded 1c")).unwrap();
        assert_eq!(meta.families, ["M PLUS Rounded 1c", "Rounded Mplus 1c"]);
        assert_eq!(meta.weight, 900);
        assert!(!meta.italic);
    }

    #[test]
    fn 読めないファイルは飛ばして空の_css_にする() {
        let table = FontTable::default();
        let css = table.replace(&[PageFont {
            path: "無い/None.ttf".into(),
            content_hash: "h".into(),
            family: Some("X".into()),
        }]);
        assert_eq!(css, "");
        assert_eq!(table.read_url(&format!("{FONT_ORIGIN}/h")), None);
    }
}
