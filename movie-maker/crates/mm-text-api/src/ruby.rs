//! ルビの注記（青空文庫の `｜親文字《ふりがな》`、漢字の並びの直後の `《ふりがな》`）の読み方。
//! シェーパーとレンダラーが同じ区切りで読むよう、契約の一部としてここに 1 つだけ置く。
//! 親文字もふりがなも無い注記の記号（親文字の無い `《…》`、`《` の続かない `｜`、閉じていない `《`）は地の文字として残す。

use std::ops::Range;

/// 親文字の始まりを示す縦線。
const BASE_MARK: char = '｜';
/// ふりがなの始まりと終わり。
const RUBY_OPEN: char = '《';
const RUBY_CLOSE: char = '》';

/// ふりがなの大きさの、親文字に対する割合（CSS の `ruby` の既定の 50%）。シェーパーがルビを含む行の上に空ける高さと、
/// 描く実装のふりがなの字の大きさは、どちらも字の大きさにこれを掛けた値にする。
pub const RUBY_SCALE: f32 = 0.5;

/// 注記を読んだ文字列の 1 区切り。範囲は元の文字列のバイト範囲。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RubySpan {
    /// 地の文字 1 つか、親文字の並び。
    pub base: Range<usize>,
    /// ふりがな。地の文字なら `None`。
    pub ruby: Option<Range<usize>>,
}

impl RubySpan {
    fn plain(at: usize, c: char) -> Self {
        Self {
            base: at..at + c.len_utf8(),
            ruby: None,
        }
    }
}

/// `｜` が無いときに親文字にしてよい文字。CJK 統合漢字（拡張 A から H まで、拡張 B 以降は 4 バイト）、
/// 互換漢字（補助を含む）と、青空文庫が漢字の並びに数える 々、〆、〇、ヶ。
fn is_kanji(c: char) -> bool {
    matches!(
        c,
        '\u{4E00}'..='\u{9FFF}'
            | '\u{3400}'..='\u{4DBF}'
            | '\u{F900}'..='\u{FAFF}'
            | '\u{20000}'..='\u{323AF}'
            | '々'
            | '〆'
            | '〇'
            | 'ヶ'
    )
}

/// `《` の直後 `open_end` から、同じ行の中の `》` までのふりがなの範囲。空か閉じていなければ `None`。
fn reading(text: &str, open_end: usize) -> Option<Range<usize>> {
    let rest = &text[open_end..];
    let close = rest.find([RUBY_CLOSE, '\n']).filter(|&i| i > 0)?;
    rest[close..]
        .starts_with(RUBY_CLOSE)
        .then(|| open_end..open_end + close)
}

/// 親文字の区切りの始まり。`｜` があればその次、無ければ直前の地の漢字の並びの始まり。
fn base_start(text: &str, out: &[RubySpan], mark: Option<usize>) -> usize {
    match mark {
        Some(m) => m + 1,
        None => {
            let run = out
                .iter()
                .rev()
                .take_while(|s| s.ruby.is_none() && text[s.base.clone()].chars().all(is_kanji))
                .count();
            out.len() - run
        }
    }
}

/// 文字列を、地の文字 1 つずつと、ふりがなの付いた親文字の区切りに分ける。注記の記号はどの区切りにも入れない。
pub fn parse_ruby(text: &str) -> Vec<RubySpan> {
    let mut out: Vec<RubySpan> = Vec::new();
    let mut mark: Option<usize> = None;
    let mut pos = 0;
    while let Some(c) = text[pos..].chars().next() {
        let next = pos + c.len_utf8();
        if c == RUBY_OPEN {
            let start = base_start(text, &out, mark);
            if let Some(ruby) = reading(text, next).filter(|_| start < out.len()) {
                let base = out[start].base.start..out[out.len() - 1].base.end;
                out.truncate(mark.unwrap_or(start));
                pos = ruby.end + RUBY_CLOSE.len_utf8();
                out.push(RubySpan {
                    base,
                    ruby: Some(ruby),
                });
                mark = None;
                continue;
            }
            // ルビにならなかった `《` で縦線の親文字は終わる。残すと後の `《…》` が縦線から親文字を取る。
            mark = None;
        }
        if c == BASE_MARK {
            mark = Some(out.len());
        } else if c == '\n' {
            mark = None;
        }
        out.push(RubySpan::plain(pos, c));
        pos = next;
    }
    out
}

/// ふりがなの付いた区切りがあるか。
pub fn has_ruby(spans: &[RubySpan]) -> bool {
    spans.iter().any(|s| s.ruby.is_some())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pieces(text: &str) -> Vec<(&str, Option<&str>)> {
        parse_ruby(text)
            .into_iter()
            .map(|s| (&text[s.base], s.ruby.map(|r| &text[r])))
            .collect()
    }

    fn plain(text: &str) -> Vec<(&str, Option<&str>)> {
        text.char_indices()
            .map(|(i, c)| (&text[i..i + c.len_utf8()], None))
            .collect()
    }

    #[test]
    fn 縦線のある親文字と漢字の並びを読む() {
        let p = pieces("｜東京《とうきょう》と漢字《かんじ》");
        assert_eq!(
            p,
            vec![
                ("東京", Some("とうきょう")),
                ("と", None),
                ("漢字", Some("かんじ")),
            ]
        );
    }

    #[test]
    fn 直前のルビの親文字は次の親文字に含めない() {
        let p = pieces("｜東京《とうきょう》都《と》");
        assert_eq!(p, vec![("東京", Some("とうきょう")), ("都", Some("と"))]);
    }

    #[test]
    fn 親文字の無い括弧と続かない縦線は文字のまま() {
        assert_eq!(pieces("見て《ほしい》"), plain("見て《ほしい》"));
        assert_eq!(pieces("A｜B"), plain("A｜B"));
        assert_eq!(pieces("《まだ"), plain("《まだ"));
        assert_eq!(pieces("漢字《》"), plain("漢字《》"));
    }

    #[test]
    fn ルビにならなかった括弧の後は縦線の位置を使わない() {
        let text = "｜東京《》は《x》";
        assert_eq!(pieces(text), plain(text));
        assert_eq!(
            pieces("｜東京《》都《と》"),
            [plain("｜東京《》"), vec![("都", Some("と"))]].concat(),
            "後の注記は直前の漢字の並びだけを親文字にする"
        );
    }

    #[test]
    fn 入れ子の括弧は最初の閉じ括弧までをふりがなにする() {
        let p = pieces("漢《字《じ》》");
        assert_eq!(p, vec![("漢", Some("字《じ")), ("》", None)]);
    }

    #[test]
    fn 拡張漢字と互換漢字と記号も漢字の並びに数える() {
        assert_eq!(
            pieces("𠮷野家《よしのや》"),
            vec![("𠮷野家", Some("よしのや"))]
        );
        assert_eq!(pieces("〇〇《まるまる》"), vec![("〇〇", Some("まるまる"))]);
        assert_eq!(
            pieces("\u{FA19}《かみ》"),
            vec![("\u{FA19}", Some("かみ"))],
            "互換漢字"
        );
        let text = "𠀋《じょう》";
        let spans = parse_ruby(text);
        assert_eq!(spans[0].base, 0..4, "4 バイトの文字の範囲");
        assert_eq!(&text[spans[0].ruby.clone().unwrap()], "じょう");
    }

    #[test]
    fn ふりがなと親文字は行をまたがない() {
        assert_eq!(pieces("漢字《かん\nじ》"), plain("漢字《かん\nじ》"));
        let p = pieces("｜あ\nい《x》");
        assert_eq!(p[0], ("｜", None), "改行で縦線は親文字の始まりでなくなる");
    }

    #[test]
    fn 範囲は元の文字列を指す() {
        let text = "今日は｜東京《とうきょう》へ";
        let spans = parse_ruby(text);
        let tokyo = spans.iter().find(|s| s.ruby.is_some()).unwrap();
        assert_eq!(
            tokyo.base,
            text.find('東').unwrap()..text.find('《').unwrap()
        );
        assert!(has_ruby(&spans));
        assert!(!has_ruby(&parse_ruby("ただの文")));
    }
}
