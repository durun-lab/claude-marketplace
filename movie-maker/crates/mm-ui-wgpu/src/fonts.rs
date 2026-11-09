//! サーフェスの UI の字を組む（cosmic-text）。書体はテーマの family で、その書体に無い日本語の字は
//! 代わりの書体（Yu Gothic UI、Meiryo）で組む。組んだ結果は文字列と字の大きさごとに覚え、描くたびには組み直さない。

use std::collections::HashMap;
use std::sync::Arc;

use cosmic_text::{
    fontdb, Attrs, Buffer, Fallback, Family, FontSystem, Metrics, PlatformFallback, Shaping, Wrap,
};
use mm_ui_layout::{line_height, TextMeasure};
use unicode_script::Script;

/// 日本語の字（漢字、ひらがな、カタカナ）の代わりの書体。前から順に、入っているものを使う。
const JAPANESE: &[&str] = &[
    "Yu Gothic UI",
    "Meiryo",
    "Meiryo UI",
    "MS UI Gothic",
    "Hiragino Sans",
    "Noto Sans JP",
];
/// 字の種類を持たない字（全角の句読点や記号）だけの語の代わりの書体。cosmic-text の Windows の既定の後ろに
/// 日本語の書体を足し、ほかの書体を探しに行く前に日本語の書体で組む。
const COMMON: &[&str] = &[
    "Segoe UI",
    "Segoe UI Emoji",
    "Segoe UI Symbol",
    "Segoe UI Historic",
    "Yu Gothic UI",
    "Meiryo",
];
/// 覚えておく文字列の数の目安（字の大きさごと）。超えたら、しばらく使っていないものを捨てる。
const KEEP: usize = 2048;
/// 数が目安を超えても残す、最後に使ってからのフレームの数（60 fps で 10 秒）。
const KEEP_FRAMES: u64 = 600;
/// 代わりの書体を選ぶときのロケール。
const LOCALE: &str = "ja-JP";

/// 代わりの書体の選び方。日本語の字は日本語の書体にし、ほかの字の種類は cosmic-text の既定に任せる。
/// 既定は漢字の書体をロケールで選び、`ja-JP` では中国語の書体に落ちるので、漢字も日本語の書体に固定する。
struct UiFallback(PlatformFallback);

impl Fallback for UiFallback {
    fn common_fallback(&self) -> &[&'static str] {
        COMMON
    }

    fn forbidden_fallback(&self) -> &[&'static str] {
        self.0.forbidden_fallback()
    }

    fn script_fallback(&self, script: Script, locale: &str) -> &[&'static str] {
        match script {
            Script::Han | Script::Hiragana | Script::Katakana => JAPANESE,
            _ => self.0.script_fallback(script, locale),
        }
    }
}

/// 組んだ文字列 1 つ。
pub(crate) struct Shaped {
    pub(crate) buffer: Buffer,
    /// 切ってよい所ごとの左端からの距離（[`TextMeasure::stops`]）。
    stops: Vec<f32>,
    /// 最後に使ったフレーム。
    used: u64,
}

/// 組んだ結果の覚え。字の大きさ（f32 のビット）ごとに、文字列から引く。
#[derive(Default)]
pub(crate) struct ShapeCache {
    shaped: HashMap<u32, HashMap<Arc<str>, Shaped>>,
    frame: u64,
    /// 組んだ回数（組み直していないことを確かめる）。
    count: u64,
}

impl ShapeCache {
    /// 覚えた結果。描く所は、置き方を決めるときに測ったものだけを引く。
    pub(crate) fn get(&self, text: &str, size: f32) -> Option<&Shaped> {
        self.shaped.get(&size.to_bits())?.get(text)
    }

    /// これまでに組んだ回数。
    pub(crate) fn count(&self) -> u64 {
        self.count
    }
}

/// 字を組んで覚えるもの。`mm-ui-layout` の文字の置き方に幅を測らせ（[`TextMeasure`]）、描く所に組んだ結果を渡す。
pub(crate) struct Shaper {
    fonts: FontSystem,
    family: String,
    cache: ShapeCache,
}

impl Shaper {
    /// OS の書体を読み込んで作る。書体は `family`、その書体に無い字は代わりの書体で組む。
    pub(crate) fn new(family: &str) -> Self {
        let mut db = fontdb::Database::new();
        db.load_system_fonts();
        let fallback = UiFallback(PlatformFallback);
        Self {
            fonts: FontSystem::new_with_locale_and_db_and_fallback(LOCALE.into(), db, fallback),
            family: family.to_owned(),
            cache: ShapeCache::default(),
        }
    }

    /// 書体を替える。替わったら覚えた結果を捨てる。
    pub(crate) fn set_family(&mut self, family: &str) {
        if family != self.family {
            self.family = family.to_owned();
            self.cache.shaped.clear();
        }
    }

    /// 覚えた結果。
    pub(crate) fn cache(&self) -> &ShapeCache {
        &self.cache
    }

    /// 描くときに一緒に要る、書体の一式と覚えた結果。
    pub(crate) fn split(&mut self) -> (&mut FontSystem, &ShapeCache) {
        (&mut self.fonts, &self.cache)
    }

    /// 1 フレームの終わり。覚えている数が目安を超えたら、しばらく使っていないものを捨てる。
    pub(crate) fn end_frame(&mut self) {
        let frame = self.cache.frame;
        for map in self.cache.shaped.values_mut() {
            if map.len() > KEEP {
                map.retain(|_, s| s.used + KEEP_FRAMES >= frame);
            }
        }
        self.cache.frame += 1;
    }
}

impl TextMeasure for Shaper {
    fn stops(&mut self, text: &str, size: f32) -> &[f32] {
        let cache = &mut self.cache;
        let map = cache.shaped.entry(size.to_bits()).or_default();
        if !map.contains_key(text) {
            map.insert(
                text.into(),
                shape(&mut self.fonts, &self.family, text, size),
            );
            cache.count += 1;
        }
        let shaped = map.get_mut(text).expect("覚えたばかり");
        shaped.used = cache.frame;
        &shaped.stops
    }
}

/// `text` を大きさ `size` で 1 行に組む。
fn shape(fonts: &mut FontSystem, family: &str, text: &str, size: f32) -> Shaped {
    let mut buffer = Buffer::new(fonts, Metrics::new(size, line_height(size)));
    buffer.set_wrap(Wrap::None);
    buffer.set_size(None, None);
    let attrs = Attrs::new().family(Family::Name(family));
    buffer.set_text(text, &attrs, Shaping::Advanced, None);
    buffer.shape_until_scroll(fonts, false);
    let stops = stops_of(&buffer);
    Shaped {
        buffer,
        stops,
        used: 0,
    }
}

/// 字のまとまり（`start` が同じグリフの並び）の終わりごとの右端。空白で終わるまとまりの後ろは除く。
fn stops_of(buffer: &Buffer) -> Vec<f32> {
    let Some(run) = buffer.layout_runs().next() else {
        return Vec::new();
    };
    let glyphs = run.glyphs;
    glyphs
        .iter()
        .enumerate()
        .filter(|(i, g)| glyphs.get(i + 1).is_none_or(|n| n.start != g.start))
        .filter(|(_, g)| !run.text[g.start..g.end].chars().all(char::is_whitespace))
        .map(|(_, g)| g.x + g.w)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use mm_ui_layout::{fit, Fit, UiFont, ELLIPSIS};

    fn shaper() -> Shaper {
        Shaper::new(&UiFont::default().family)
    }

    /// `text` の最初の字を組んだ書体の名前。
    fn face_of(s: &mut Shaper, text: &str) -> String {
        s.stops(text, 12.0);
        let (fonts, cache) = s.split();
        let shaped = cache.get(text, 12.0).expect("組んだ");
        let run = shaped.buffer.layout_runs().next().expect("1 行");
        let id = run.glyphs[0].font_id;
        fonts.db().face(id).expect("書体").families[0].0.clone()
    }

    #[test]
    fn 幅は字の数に比例し_切ってよい所は昇順で最後が全体の幅() {
        let mut s = shaper();
        let one = s.width("A", 12.0);
        let four = s.width("AAAA", 12.0);
        assert!(one > 4.0 && one < 12.0, "12 px の A の幅 {one}");
        assert!((four - 4.0 * one).abs() < 0.01, "{four} と {one}");
        let stops = s.stops("ab cd", 12.0).to_vec();
        assert_eq!(stops.len(), 4, "空白の後ろでは切らない: {stops:?}");
        assert!(stops.windows(2).all(|w| w[0] < w[1]), "{stops:?}");
        assert_eq!(s.width("", 12.0), 0.0);
    }

    #[test]
    fn 字の大きさに比例する() {
        let mut s = shaper();
        let small = s.width("Footage: a.mp4", 12.0);
        let large = s.width("Footage: a.mp4", 24.0);
        assert!((large / small - 2.0).abs() < 0.02, "{small} と {large}");
    }

    #[test]
    fn 日本語は代わりの書体で組み_ひらがなと漢字は同じ日本語の書体() {
        let mut s = shaper();
        if s.fonts
            .db()
            .faces()
            .all(|f| f.families[0].0 != "Yu Gothic UI")
        {
            eprintln!("Yu Gothic UI が無いのでスキップする");
            return;
        }
        assert_eq!(face_of(&mut s, "Footage"), "Tahoma");
        assert_eq!(face_of(&mut s, "こんにちは"), "Yu Gothic UI");
        assert_eq!(
            face_of(&mut s, "世界"),
            "Yu Gothic UI",
            "漢字も中国語の書体にしない"
        );
        assert_eq!(
            face_of(&mut s, "「」"),
            "Yu Gothic UI",
            "全角の記号だけの語"
        );
        let w = s.width("日本語", 12.0);
        assert!((w - 36.0).abs() < 1.0, "全角 3 字は字の大きさの 3 倍: {w}");
    }

    #[test]
    fn 切り詰めは組んだ幅で三点を足して入る所まで() {
        let mut s = shaper();
        let text = "Footage: a-very-long-file-name.mp4";
        let ellipsis = s.width(ELLIPSIS, 12.0);
        let stops = s.stops(text, 12.0).to_vec();
        let max = 80.0;
        let Fit::Cut(w) = fit(&stops, ellipsis, max) else {
            panic!("80 px には入らない")
        };
        assert!(w + ellipsis <= max && w > max - ellipsis - 12.0, "{w}");
    }

    #[test]
    fn 同じ文字列は組み直さず_書体を替えたら組み直す() {
        let mut s = shaper();
        s.stops("Footage", 12.0);
        s.stops("Footage", 12.0);
        s.end_frame();
        s.stops("Footage", 12.0);
        assert_eq!(s.cache().count(), 1);
        s.stops("Footage", 11.0);
        assert_eq!(s.cache().count(), 2, "字の大きさが違えば別に組む");
        s.set_family("Segoe UI");
        s.stops("Footage", 12.0);
        assert_eq!(s.cache().count(), 3);
    }

    #[test]
    fn 覚えた数が目安を超えたら_しばらく使っていないものを捨てる() {
        let mut s = shaper();
        for i in 0..=KEEP {
            s.stops(&format!("old {i}"), 12.0);
        }
        for _ in 0..=KEEP_FRAMES {
            s.end_frame();
        }
        s.stops("new", 12.0);
        s.end_frame();
        assert!(s.cache().get("old 0", 12.0).is_none());
        assert!(s.cache().get("new", 12.0).is_some());
    }
}
