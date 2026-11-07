//! プロジェクトが持つ書体のファイル。描く実装はこれを読み込み、同じ名前の OS の書体より優先する。
//! 中身の BLAKE3 で見分けるので、同じ組なら同じ画素になり、プロジェクトをほかの PC に移しても同じ書体で描ける。
//!
//! 書体は `fonts/<family>/<ファイル>` に置き、フォルダーの名前を family の名前としても登録する。
//! ファイルの中の名前が配布の名前と違う書体（Google Fonts の M PLUS Rounded 1c は中が `Rounded Mplus 1c`）を、
//! テーマや CSS が書く名前で引けるようにするため（Google Fonts の CSS API が `METADATA.pb` の名前で宣言するのと同じ役）。

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// 書体のファイルとして扱う拡張子（小文字）。
const FONT_EXTENSIONS: [&str; 3] = ["ttf", "otf", "ttc"];
/// 書体を置くフォルダーの名前。この直下のフォルダーの名前が family の名前になる。
pub const FONTS_DIR: &str = "fonts";

/// 書体のファイル 1 つ。
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct FontFile {
    /// ファイルの場所。
    pub path: PathBuf,
    /// 中身の BLAKE3（小文字の 16 進）。空なら場所で見分ける。
    pub content_hash: String,
    /// ファイルの中の名前に足して登録する family の名前（`fonts/<family>/` のフォルダーの名前）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub family: Option<String>,
}

impl FontFile {
    /// 素材の場所とハッシュから作る。family の名前は場所の `fonts/<family>/` から取る。
    pub fn from_asset(path: impl Into<PathBuf>, content_hash: impl Into<String>) -> Self {
        let path = path.into();
        Self {
            family: declared_family(&path),
            path,
            content_hash: content_hash.into(),
        }
    }

    /// 見分ける名前。中身のハッシュ（空なら場所）と、足す family の名前。
    pub fn identity(&self) -> String {
        let id = if self.content_hash.is_empty() {
            self.path.to_string_lossy().into_owned()
        } else {
            self.content_hash.clone()
        };
        match &self.family {
            Some(family) => format!("{id}@{family}"),
            None => id,
        }
    }
}

/// 場所が `…/fonts/<family>/<ファイル>` なら `<family>`。
pub fn declared_family(path: &Path) -> Option<String> {
    let dir = path.parent()?;
    let grand = dir.parent()?.file_name()?;
    if !grand.eq_ignore_ascii_case(FONTS_DIR) {
        return None;
    }
    dir.file_name()?.to_str().map(str::to_owned)
}

/// 素材のパスが書体のファイルか（拡張子 `ttf`、`otf`、`ttc`）。
pub fn is_font_path(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| FONT_EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
}

/// 書体の組の指紋。順と重複に依らず、空の組は空の文字列。フレームのキャッシュのキーに入れる。
pub fn fonts_fingerprint(fonts: &[FontFile]) -> String {
    let mut ids: Vec<String> = fonts.iter().map(FontFile::identity).collect();
    ids.sort();
    ids.dedup();
    ids.join(",")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn font(path: &str, hash: &str) -> FontFile {
        FontFile::from_asset(path, hash)
    }

    #[test]
    fn 指紋は順と重複と場所に依らない() {
        let a = [font("x/a.ttf", "aa"), font("x/b.ttf", "bb")];
        let b = [
            font("y/b.ttf", "bb"),
            font("y/a.ttf", "aa"),
            font("z/a.ttf", "aa"),
        ];
        assert_eq!(fonts_fingerprint(&a), fonts_fingerprint(&b));
        assert_eq!(fonts_fingerprint(&[]), "");
    }

    #[test]
    fn ハッシュが空なら場所で見分け_family_の名前も入れる() {
        assert_eq!(font("x/a.ttf", "").identity(), "x/a.ttf");
        assert_eq!(font("p/fonts/A B/a.ttf", "h").identity(), "h@A B");
    }

    #[test]
    fn family_の名前は_fonts_の直下のフォルダーから取る() {
        let f = font(
            "D:/proj/fonts/M PLUS Rounded 1c/MPLUSRounded1c-Regular.ttf",
            "h",
        );
        assert_eq!(f.family.as_deref(), Some("M PLUS Rounded 1c"));
        assert_eq!(font("D:/proj/other/A/a.ttf", "h").family, None);
        assert_eq!(font("fonts/a.ttf", "h").family, None);
    }

    #[test]
    fn 拡張子で書体のファイルを見分ける() {
        assert!(is_font_path(Path::new("fonts/A-Regular.TTF")));
        assert!(is_font_path(Path::new("a.otf")));
        assert!(!is_font_path(Path::new("a.cube")));
        assert!(!is_font_path(Path::new("ttf")));
    }
}
