//! `web/*.html` のページの約束（`crates/mm-nodes-cef/README.md`）の検査。
//!
//! ページは `window.mmSetTime(t, data)` で、時刻 `t` と `data` だけから画面を作る。実時間で進むもの（時計、乱数、
//! タイマー、CSS と SVG と Web Animations の動き）を使うと同じ `t` でも画素が変わり、外の資源は CSP が読ませない。
//! 書き方の文字で見つけるので、どのパックのページも `mm pack check` で同じに確かめられる。

/// 実時間で進むものと乱数の書き方。
const FORBIDDEN: [&str; 19] = [
    "Date.now",
    "new Date",
    "performance.now",
    "Math.random",
    "crypto.getRandomValues",
    "setTimeout",
    "setInterval",
    "requestAnimationFrame",
    "requestIdleCallback",
    "document.timeline",
    ".animate(",
    "@keyframes",
    "animation:",
    "animation-",
    "transition:",
    "transition-",
    "<animate",
    "<set ",
    "style.animation",
];
/// 画面を作る入口。
const ENTRY: &str = "window.mmSetTime";
/// 埋め込んだ資源の頭。
const DATA_URL: &str = "data:";
/// 外の URL に見えるが読み込まない、SVG の名前空間。
const SVG_NS: &str = "http://www.w3.org/2000/svg";

/// ページ `name` の約束の違反の一覧。
pub(crate) fn check_page(name: &str, html: &str) -> Vec<String> {
    let mut errors = Vec::new();
    if !html.contains(ENTRY) {
        errors.push(format!("web/{name}.html: {ENTRY} で画面を作る"));
    }
    for word in FORBIDDEN.iter().filter(|w| html.contains(**w)) {
        errors.push(format!(
            "web/{name}.html: {word} を使わない（同じ t で画素が変わる）"
        ));
    }
    let embedded = html
        .match_indices("src=\"")
        .all(|(i, m)| html[i + m.len()..].starts_with(DATA_URL));
    if !embedded {
        errors.push(format!(
            "web/{name}.html: 資源は {DATA_URL} の URL で埋め込む"
        ));
    }
    let external = html
        .match_indices("http")
        .any(|(i, _)| !html[i..].starts_with(SVG_NS));
    if external {
        errors.push(format!("web/{name}.html: 外の URL を読まない"));
    }
    errors
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 約束を守るページは通る() {
        let html = format!(
            "<svg xmlns=\"{SVG_NS}\"></svg><img src=\"data:image/png;base64,AA\">\
             <script>{ENTRY} = (t, d) => {{}};</script>"
        );
        assert!(check_page("ok", &html).is_empty());
    }

    #[test]
    fn 時計と動きと外の資源と入口の無いページを捕まえる() {
        let html = "<style>p { transition-duration: 1s }</style><img src=\"https://x/a.png\">\
                    <script>setInterval(() => {}, 9); el.animate([], 1)</script>";
        let errors = check_page("bad", html).join("\n");
        for needle in [
            "mmSetTime で画面を作る",
            "transition- を使わない",
            "setInterval を使わない",
            ".animate( を使わない",
            "data: の URL で埋め込む",
            "外の URL を読まない",
        ] {
            assert!(errors.contains(needle), "{needle}: {errors}");
        }
    }
}
