//! サーフェスの UI の字をシェーピングする（cosmic-text）。フォントはテーマの family で、そのフォントに無い日本語の字は
//! フォールバックのフォント（Yu Gothic UI、Meiryo）でシェーピングする。私用領域の字（ツールバーの字のアイコン）は
//! Windows の記号のフォントを指してシェーピングする。シェーピングの結果は文字列と字の大きさごとに
//! キャッシュし、描くたびにはシェーピングし直さない。

use std::collections::HashMap;
use std::sync::Arc;

use cosmic_text::{
    fontdb, Attrs, Buffer, Fallback, Family, FontSystem, Metrics, PlatformFallback, Shaping, Wrap,
};
use mm_ui_layout::{line_height, TextMeasure};
use unicode_script::Script;

/// 日本語の字（漢字、ひらがな、カタカナ）のフォールバックのフォント。前から順に、入っているものを使う。
const JAPANESE: &[&str] = &[
    "Yu Gothic UI",
    "Meiryo",
    "Meiryo UI",
    "MS UI Gothic",
    "Hiragino Sans",
    "Noto Sans JP",
];
/// Unicode の Script が Common の字（全角の句読点や記号、絵文字）だけの語のフォールバックのフォント。
/// cosmic-text の Windows の既定の後ろに日本語のフォントを足し、ほかのフォントを探しに行く前に日本語のフォントで
/// シェーピングする。
const COMMON: &[&str] = &[
    "Segoe UI",
    "Segoe UI Emoji",
    "Segoe UI Symbol",
    "Segoe UI Historic",
    "Yu Gothic UI",
    "Meiryo",
];
/// Windows の記号のフォント。前から順に、入っているものを使う（Windows 11 は Segoe Fluent Icons、Windows 10 は
/// Segoe MDL2 Assets。WinUI の `SymbolIcon` と同じ並び）。2 つは同じ符号位置に同じ意味の字を割り当てている。
const SYMBOL_FONTS: &[&str] = &["Segoe Fluent Icons", "Segoe MDL2 Assets"];
/// 記号のフォントの字の範囲（Unicode の私用領域）。符号位置の意味はフォントごとに違うので、フォールバックに任せない。
const SYMBOL_RANGE: std::ops::RangeInclusive<char> = '\u{E000}'..='\u{F8FF}';
/// キャッシュする文字列の数の目安（字の大きさごと）。超えたら、しばらく使っていないものを捨てる。
const KEEP: usize = 2048;
/// 数が目安を超えても残す、最後に使ってからのフレームの数（60 fps で 10 秒）。
const KEEP_FRAMES: u64 = 600;
/// フォールバックのフォントを選ぶときのロケール。
const LOCALE: &str = "ja-JP";

/// フォールバックのフォントの選び方。日本語の字は日本語のフォントにし、ほかの Script は cosmic-text の既定に任せる。
/// 既定は漢字のフォントをロケールで選び、`ja-JP` では中国語のフォントを選ぶので、漢字も日本語のフォントに固定する。
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

/// シェーピングした文字列 1 つ。
pub(crate) struct Shaped {
    pub(crate) buffer: Buffer,
    /// 切り詰めの位置ごとの左端からの距離（[`TextMeasure::stops`]）。
    stops: Vec<f32>,
    /// 最後に使ったフレーム。
    used: u64,
}

/// シェーピングの結果のキャッシュ。字の大きさ（f32 のビット）ごとに、文字列から引く。
#[derive(Default)]
pub(crate) struct ShapeCache {
    shaped: HashMap<u32, HashMap<Arc<str>, Shaped>>,
    frame: u64,
    /// シェーピングした回数（シェーピングし直していないことを確かめる）。
    count: u64,
}

impl ShapeCache {
    /// キャッシュした結果。描く所は、置き方を決めるときに測ったものだけを引く。
    pub(crate) fn get(&self, text: &str, size: f32) -> Option<&Shaped> {
        self.shaped.get(&size.to_bits())?.get(text)
    }

    /// これまでにシェーピングした回数。
    pub(crate) fn count(&self) -> u64 {
        self.count
    }
}

/// 字をシェーピングしてキャッシュするもの。`mm-ui-layout` の文字の置き方に幅を測らせ（[`TextMeasure`]）、
/// 描く所にシェーピングの結果を渡す。
pub(crate) struct Shaper {
    fonts: FontSystem,
    family: String,
    /// 記号のフォント（[`SYMBOL_FONTS`] のうち入っている最初のもの）。
    symbols: Option<&'static str>,
    cache: ShapeCache,
}

impl Shaper {
    /// OS のフォントを読み込んで作る。フォントは `family`、そのフォントに無い字はフォールバックのフォントで
    /// シェーピングする。
    pub(crate) fn new(family: &str) -> Self {
        let mut db = fontdb::Database::new();
        db.load_system_fonts();
        Self::with_db(family, db)
    }

    /// フォントの一式 `db` から作る。
    pub(crate) fn with_db(family: &str, db: fontdb::Database) -> Self {
        let symbols = SYMBOL_FONTS.iter().copied().find(|name| {
            db.faces()
                .any(|f| f.families.iter().any(|(n, _)| n == name))
        });
        let fallback = UiFallback(PlatformFallback);
        Self {
            fonts: FontSystem::new_with_locale_and_db_and_fallback(LOCALE.into(), db, fallback),
            family: family.to_owned(),
            symbols,
            cache: ShapeCache::default(),
        }
    }

    /// 記号のフォントが入っているか。無ければツールバーは字のアイコンを標準のフォントの記号の字で描く。
    pub(crate) fn has_symbol_font(&self) -> bool {
        self.symbols.is_some()
    }

    /// フォントを替える。替わったらキャッシュを捨てる。
    pub(crate) fn set_family(&mut self, family: &str) {
        if family != self.family {
            self.family = family.to_owned();
            self.cache.shaped.clear();
        }
    }

    /// シェーピングの結果のキャッシュ。
    pub(crate) fn cache(&self) -> &ShapeCache {
        &self.cache
    }

    /// 描くときに一緒に要る、フォントの一式とキャッシュ。
    pub(crate) fn split(&mut self) -> (&mut FontSystem, &ShapeCache) {
        (&mut self.fonts, &self.cache)
    }

    /// 1 フレームの終わり。キャッシュした数が目安を超えたら、しばらく使っていないものを捨てる。
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
            let family = match self.symbols {
                Some(symbols) if is_symbol(text) => symbols,
                _ => self.family.as_str(),
            };
            map.insert(text.into(), shape(&mut self.fonts, family, text, size));
            cache.count += 1;
        }
        let shaped = map.get_mut(text).expect("キャッシュしたばかり");
        shaped.used = cache.frame;
        &shaped.stops
    }
}

/// 字が全部、記号のフォントの字の範囲か。
fn is_symbol(text: &str) -> bool {
    !text.is_empty() && text.chars().all(|c| SYMBOL_RANGE.contains(&c))
}

/// `text` を大きさ `size` で 1 行にシェーピングする。
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

/// シェーピングのクラスタ（`start` が同じグリフの並び。書記素クラスタにあたる）の終わりごとの右端。
/// 空白で終わるクラスタの後ろは除く。
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

    /// `text` の最初の字をシェーピングしたフォントの名前。
    fn face_of(s: &mut Shaper, text: &str) -> String {
        s.stops(text, 12.0);
        let (fonts, cache) = s.split();
        let shaped = cache.get(text, 12.0).expect("シェーピングした");
        let run = shaped.buffer.layout_runs().next().expect("1 行");
        let id = run.glyphs[0].font_id;
        fonts.db().face(id).expect("フォント").families[0].0.clone()
    }

    /// OS に `family` のフォントがあるか。無ければ試験をスキップする。
    fn installed(s: &Shaper, family: &str) -> bool {
        let found = s.fonts.db().faces().any(|f| f.families[0].0 == family);
        if !found {
            eprintln!("{family} が無いのでスキップする");
        }
        found
    }

    #[test]
    fn 幅は字の数に比例し_切り詰めの位置は昇順で最後が全体の幅() {
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
        let small = s.width("take 3.mp4", 12.0);
        let large = s.width("take 3.mp4", 24.0);
        assert!((large / small - 2.0).abs() < 0.02, "{small} と {large}");
    }

    #[test]
    fn 日本語はフォールバックのフォントでシェーピングし_ひらがなと漢字は同じ日本語のフォント() {
        let mut s = shaper();
        if !installed(&s, "Yu Gothic UI") {
            return;
        }
        assert_eq!(face_of(&mut s, "Footage"), "Tahoma");
        assert_eq!(face_of(&mut s, "こんにちは"), "Yu Gothic UI");
        assert_eq!(
            face_of(&mut s, "世界"),
            "Yu Gothic UI",
            "漢字も中国語のフォントにしない"
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
    fn 絵文字と_tahoma_に無い記号もフォールバックのフォントでシェーピングし_豆腐にしない() {
        let mut s = shaper();
        if !installed(&s, "Segoe UI Emoji") {
            return;
        }
        assert_eq!(face_of(&mut s, "🎬"), "Segoe UI Emoji");
        for text in ["→", "★", "✓", "①", "♪", "🎬", "é", "Ж"] {
            s.stops(text, 12.0);
            let shaped = s.cache().get(text, 12.0).expect("シェーピングした");
            let run = shaped.buffer.layout_runs().next().expect("1 行");
            assert!(
                run.glyphs.iter().all(|g| g.glyph_id != 0),
                "{text} がどのフォントにも無い（.notdef）"
            );
        }
    }

    /// Windows のフォントのフォルダーの `files` だけを読み込んだ一式。どれかが無ければ `None`（試験をスキップする）。
    fn db_of(files: &[&str]) -> Option<fontdb::Database> {
        let dir = std::path::PathBuf::from(std::env::var_os("WINDIR")?).join("Fonts");
        let mut db = fontdb::Database::new();
        for file in files {
            db.load_font_file(dir.join(file)).ok()?;
        }
        Some(db)
    }

    #[test]
    fn 私用領域の字は記号のフォントを指してシェーピングし_segoe_fluent_icons_が無ければ_segoe_mdl2_assets(
    ) {
        let mut s = shaper();
        if !installed(&s, "Segoe Fluent Icons") {
            return;
        }
        assert!(s.has_symbol_font());
        assert_eq!(face_of(&mut s, "\u{E7A7}"), "Segoe Fluent Icons");
        assert_eq!(
            face_of(&mut s, "Undo"),
            "Tahoma",
            "私用領域の外の字は今までどおり"
        );
        let Some(win10) = db_of(&["tahoma.ttf", "segmdl2.ttf"]) else {
            return;
        };
        let mut s = Shaper::with_db("Tahoma", win10);
        assert_eq!(face_of(&mut s, "\u{E7A7}"), "Segoe MDL2 Assets");
        let plain = db_of(&["tahoma.ttf"]).expect("Tahoma");
        assert!(!Shaper::with_db("Tahoma", plain).has_symbol_font());
    }

    #[test]
    fn 切り詰めはシェーピングした幅で省略記号を足して入る所まで() {
        let mut s = shaper();
        let text = "a-very-long-file-name-of-the-footage.mp4";
        let ellipsis = s.width(ELLIPSIS, 12.0);
        let stops = s.stops(text, 12.0).to_vec();
        let max = 80.0;
        let Fit::Cut(w) = fit(&stops, ellipsis, max) else {
            panic!("80 px には入らない")
        };
        assert!(w + ellipsis <= max && w > max - ellipsis - 12.0, "{w}");
    }

    #[test]
    fn 同じ文字列はシェーピングし直さず_フォントを替えたらシェーピングし直す() {
        let mut s = shaper();
        s.stops("Footage", 12.0);
        s.stops("Footage", 12.0);
        s.end_frame();
        s.stops("Footage", 12.0);
        assert_eq!(s.cache().count(), 1);
        s.stops("Footage", 11.0);
        assert_eq!(
            s.cache().count(),
            2,
            "字の大きさが違えば別にシェーピングする"
        );
        s.set_family("Segoe UI");
        s.stops("Footage", 12.0);
        assert_eq!(s.cache().count(), 3);
    }

    #[test]
    fn キャッシュした数が目安を超えたら_しばらく使っていないものを捨てる() {
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
