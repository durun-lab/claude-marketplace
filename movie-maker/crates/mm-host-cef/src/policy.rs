//! ページが読み込んでよい URL の決まり。ホストの `RequestHandler` がこれで遷移と要求を拒否する。
//!
//! ページはパックやプロジェクトの HTML で、他人が書いたものでもある。ネットワークとファイルを読めると、
//! 画素が決定的でなくなり、ローカルのファイルの中身を画面に出せてしまう。そこで、ホストが書いたページそのものと、
//! ページの中に埋め込んだ資源（`data:` と `blob:`）だけを許す。

use std::path::Path;

/// ページの中に埋め込める資源の scheme。
const EMBEDDED_SCHEMES: [&str; 2] = ["data:", "blob:"];
/// ローカルのファイルの URL の、パスの前まで。
const FILE_URL_HEAD: &str = "file:///";
/// パーセントエンコードの 1 バイト（`%XX`）の 16 進の桁の数。
const PERCENT_HEX_DIGITS: usize = 2;
/// 16 進の基数。
const HEX_RADIX: u32 = 16;

/// ファイル `path` を開く `file:` の URL。
pub fn file_url(path: &Path) -> String {
    format!("{FILE_URL_HEAD}{}", slashed(path))
}

fn slashed(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

/// `url` がファイル `path` を指す `file:` の URL か。Chromium がパーセントエンコードした形をデコードして比べ、
/// Windows のパスに合わせて大文字と小文字を区別しない。
pub fn is_file_url_of(url: &str, path: &Path) -> bool {
    has_scheme(url, FILE_URL_HEAD)
        && percent_decode(&url[FILE_URL_HEAD.len()..])
            .is_some_and(|p| p.eq_ignore_ascii_case(&slashed(path)))
}

/// `%XX` を 1 バイトに戻す。形の崩れた `%` と、UTF-8 にならない並びは `None`。
fn percent_decode(text: &str) -> Option<String> {
    let mut out = Vec::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find('%') {
        out.extend_from_slice(&rest.as_bytes()[..at]);
        let hex = rest
            .get(at + 1..at + 1 + PERCENT_HEX_DIGITS)
            .filter(|h| h.bytes().all(|b| b.is_ascii_hexdigit()))?;
        out.push(u8::from_str_radix(hex, HEX_RADIX).ok()?);
        rest = &rest[at + 1 + PERCENT_HEX_DIGITS..];
    }
    out.extend_from_slice(rest.as_bytes());
    String::from_utf8(out).ok()
}

/// `url` の scheme が `scheme`（`data:` の形）か。大文字と小文字を区別しない。
fn has_scheme(url: &str, scheme: &str) -> bool {
    url.get(..scheme.len())
        .is_some_and(|head| head.eq_ignore_ascii_case(scheme))
}

/// ページ `page` の中の資源の要求 `url` を通すか。ページそのものと、`data:` と `blob:` だけを通す。
pub fn allows_resource(url: &str, page: &str) -> bool {
    url == page || EMBEDDED_SCHEMES.iter().any(|s| has_scheme(url, s))
}

/// ページ `page` を開いたブラウザーの遷移 `url` を通すか。ホストが開いたページそのものだけを通す
/// （`location` の書き換え、リンク、フォームの送信、`data:` への遷移を止める）。
pub fn allows_navigation(url: &str, page: &str) -> bool {
    !page.is_empty() && url == page
}
