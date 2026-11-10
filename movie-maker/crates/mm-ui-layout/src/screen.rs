//! サーフェス全体の割り付け。上のビューアーの領域に Program（素材を開いていれば左に Source）の映像の領域と、
//! それぞれの真下の再生の操作のバー、下に Timeline。

use crate::geom::Rect;
use crate::timeline_layout::MIN_TIMELINE_H;
use crate::transport::TRANSPORT_H;

/// Program と Timeline の間隔。
const GAP: f32 = 4.0;
/// Timeline を全体表示にしたときの左右の余白（px）。
pub const VIEW_MARGIN: f32 = 16.0;
/// Timeline の高さ（サーフェスに対する比）。中身の高さには合わせず、入らない行は縦にスクロールする。
pub const TIMELINE_RATIO: f32 = 0.45;
/// 再生の操作のバーを出すのに要る映像の高さ。ビューアーの領域がこれとバーの和より低ければ、バーを出さずに映像に回す
/// （映像の見えないビューアーは使えず、再生の操作はキーとメニューにもあるため）。
const MIN_VIDEO_H: f32 = TRANSPORT_H;
/// Program（映像の領域と再生の操作のバー）の高さの下限。仕切りを下げても、映像とバーが 1 つずつ残る。
pub const MIN_PROGRAM_H: f32 = MIN_VIDEO_H + TRANSPORT_H;
/// 仕切りをつかめる範囲が、見た目の線（[`GAP`]、[`SPLITTER_W`]）から Program と Timeline の側へはみ出す幅。
const GRAB_PAD: f32 = 2.0;
/// サーフェスと WebView の仕切りの幅。サーフェスの右端に含め、WebView を開けているときだけ出す。
pub const SPLITTER_W: f32 = 4.0;

/// 2 つの仕切り。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Splitter {
    /// サーフェスと WebView の境界。左右に動かす。
    Vertical,
    /// Program と Timeline の境界。上下に動かす。
    Horizontal,
}

/// 幅 `w` のサーフェスのうち、Program と Timeline が使う幅。縦の仕切りを出すときは右端の [`SPLITTER_W`] を除く。
pub fn content_width(w: f32, vertical: bool) -> f32 {
    if vertical {
        (w - SPLITTER_W).max(0.0)
    } else {
        w.max(0.0)
    }
}

/// サーフェスと WebView の仕切りの見た目の矩形。サーフェスの右端の全高。
pub fn vertical_splitter_rect(w: f32, h: f32) -> Rect {
    Rect::new((w - SPLITTER_W).max(0.0), 0.0, SPLITTER_W.min(w), h)
}

/// サーフェスの点 `(x, y)`（論理ピクセル）にある仕切り。つかめる範囲は見た目の線より Program と Timeline の側へ
/// [`GRAB_PAD`] 広い。角は縦の仕切りが勝つ。`vertical` は縦の仕切りを出しているか。
pub fn splitter_at(w: f32, h: f32, ratio: f32, vertical: bool, x: f32, y: f32) -> Option<Splitter> {
    if vertical && x >= w - SPLITTER_W - GRAB_PAD && x < w {
        return Some(Splitter::Vertical);
    }
    let gap = splitter_rect(content_width(w, vertical), h, ratio);
    let in_x = x >= 0.0 && x < gap.right();
    (in_x && y >= gap.y - GRAB_PAD && y < gap.bottom() + GRAB_PAD).then_some(Splitter::Horizontal)
}

/// サーフェスの割り付け。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SurfaceLayout {
    /// Program の映像の領域（映像を縦横比を保って置く所。再生の操作のバーを除く）。
    pub program: Rect,
    /// Program の下の再生の操作のバー。
    pub program_transport: Rect,
    pub timeline: Rect,
    /// Source の映像の領域（再生の操作のバーを除く）。素材を開いているときだけ、Program の左に並ぶ。
    pub source: Option<Rect>,
    /// Source の下の再生の操作のバー。`source` と一緒に現れる。
    pub source_transport: Option<Rect>,
}

/// 高さ `h` のサーフェスで、Timeline が高さの比 `ratio` から取る高さ。下限は目盛りとレイヤー 2 本、上限は Program の下限
/// （[`MIN_PROGRAM_H`]）を残す高さ。サーフェスが低くて両立しないときは Timeline の下限を取る。
pub fn timeline_height(h: f32, ratio: f32) -> f32 {
    let h = h.max(0.0);
    let room = h - GAP - MIN_PROGRAM_H;
    (h * ratio).min(room).max(MIN_TIMELINE_H).min(h)
}

/// Timeline の高さ `th` を、サーフェスの高さ `h` に対する比にする。仕切りのドラッグが保存する値。下限と上限は
/// [`timeline_height`] と同じで、窓の高さが変わっても Timeline の割合を保つ。
pub fn timeline_ratio(h: f32, th: f32) -> f32 {
    if h <= 0.0 {
        return TIMELINE_RATIO;
    }
    let room = (h - GAP - MIN_PROGRAM_H).max(MIN_TIMELINE_H);
    th.clamp(MIN_TIMELINE_H, room) / h
}

/// Program と Timeline の仕切りの見た目の矩形（高さ [`GAP`]）。つかむ範囲は [`SPLITTER_GRAB_H`]。
pub fn splitter_rect(w: f32, h: f32, ratio: f32) -> Rect {
    Rect::new(0.0, h - timeline_height(h, ratio) - GAP, w, GAP)
}

/// 既定の高さの比（[`TIMELINE_RATIO`]）で割る。
pub fn split_viewers(w: f32, h: f32, with_source: bool) -> SurfaceLayout {
    split_viewers_with(w, h, with_source, TIMELINE_RATIO)
}

/// 幅 `w`、高さ `h` のサーフェスを割る。Timeline の高さはサーフェスの `timeline_ratio` 倍（[`timeline_height`]）。
/// ビューアーの領域は映像の領域の下に再生の操作のバー（[`TRANSPORT_H`]。領域が低ければ高さ 0）を取り、`with_source` なら
/// 左の Source と右の Program に半分ずつ割る。
pub fn split_viewers_with(w: f32, h: f32, with_source: bool, timeline_ratio: f32) -> SurfaceLayout {
    let h = h.max(0.0);
    let th = timeline_height(h, timeline_ratio);
    let ph = (h - th - GAP).max(0.0);
    let transport_h = if ph >= MIN_VIDEO_H + TRANSPORT_H {
        TRANSPORT_H
    } else {
        0.0
    };
    let vh = ph - transport_h;
    let timeline = Rect::new(0.0, h - th, w, th);
    // 映像の領域と、その真下の再生の操作のバー。
    let column = |x: f32, cw: f32| (Rect::new(x, 0.0, cw, vh), Rect::new(x, vh, cw, transport_h));
    if !with_source {
        let (program, program_transport) = column(0.0, w);
        return SurfaceLayout {
            program,
            program_transport,
            timeline,
            source: None,
            source_transport: None,
        };
    }
    let half = ((w - GAP) / 2.0).max(0.0);
    let (source, source_transport) = column(0.0, half);
    let (program, program_transport) = column(half + GAP, half);
    SurfaceLayout {
        program,
        program_transport,
        timeline,
        source: Some(source),
        source_transport: Some(source_transport),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 二つの領域はサーフェスを隙間つきで埋める() {
        let l = split_viewers(1000.0, 800.0, false);
        assert_eq!(l.program_transport.bottom() + GAP, l.timeline.y);
        assert_eq!(l.timeline.bottom(), 800.0);
        assert_eq!((l.program.w, l.timeline.w), (1000.0, 1000.0));
    }

    #[test]
    fn source_を開くとビューアーの領域を左右に半分ずつ割り_timeline_は変わらない() {
        let plain = split_viewers(1000.0, 800.0, false);
        let l = split_viewers(1000.0, 800.0, true);
        let source = l.source.expect("Source");
        assert_eq!(source.right() + GAP, l.program.x);
        assert_eq!(l.program.right(), 1000.0);
        assert_eq!(l.timeline, plain.timeline);
        assert_eq!((source.h, l.program.h), (plain.program.h, plain.program.h));
        assert_eq!(plain.source, None);
    }

    #[test]
    fn timeline_はサーフェスの_45_パーセントの高さ() {
        assert_eq!(split_viewers(1000.0, 800.0, false).timeline.h, 360.0);
        assert_eq!(split_viewers(1000.0, 1200.0, false).timeline.h, 540.0);
    }

    #[test]
    fn 低いサーフェスでも_timeline_は目盛りと行_2_本の高さを保つ() {
        assert_eq!(
            split_viewers(1000.0, 150.0, false).timeline.h,
            MIN_TIMELINE_H
        );
    }

    #[test]
    fn 高さの比を渡すと_timeline_の高さがその比になる() {
        assert_eq!(
            split_viewers_with(1000.0, 800.0, false, 0.5).timeline.h,
            400.0
        );
        assert_eq!(
            split_viewers_with(1000.0, 800.0, false, TIMELINE_RATIO),
            split_viewers(1000.0, 800.0, false)
        );
    }

    #[test]
    fn timeline_の高さは_program_の下限を残す所まで() {
        let l = split_viewers_with(1000.0, 800.0, false, 0.99);
        let program = l.program.h + l.program_transport.h;
        assert_eq!(program, MIN_PROGRAM_H);
        assert!(l.program_transport.h > 0.0, "バーは残る");
        assert_eq!(l.timeline.bottom(), 800.0);
    }

    #[test]
    fn 比は下限と上限に丸めて保存し_窓の高さが変わっても割合を保つ() {
        assert_eq!(timeline_ratio(800.0, 0.0), MIN_TIMELINE_H / 800.0);
        assert_eq!(timeline_ratio(800.0, 5000.0), (800.0 - GAP - MIN_PROGRAM_H) / 800.0);
        let r = timeline_ratio(800.0, 400.0);
        assert_eq!(r, 0.5);
        assert_eq!(timeline_height(1200.0, r), 600.0);
        assert_eq!(timeline_ratio(0.0, 100.0), TIMELINE_RATIO);
    }

    #[test]
    fn 仕切りは_program_と_timeline_の隙間() {
        let (w, h, r) = (1000.0, 800.0, 0.5);
        let gap = splitter_rect(w, h, r);
        let l = split_viewers_with(w, h, false, r);
        assert_eq!(gap.bottom(), l.timeline.y);
        assert_eq!(gap.y, l.program_transport.bottom());
        assert_eq!((gap.x, gap.w, gap.h), (0.0, w, GAP));
    }

    #[test]
    fn 縦の仕切りはサーフェスの右端で_program_と_timeline_の幅から除く() {
        assert_eq!(content_width(1000.0, true), 1000.0 - SPLITTER_W);
        assert_eq!(content_width(1000.0, false), 1000.0);
        let v = vertical_splitter_rect(1000.0, 800.0);
        assert_eq!((v.right(), v.w, v.h), (1000.0, SPLITTER_W, 800.0));
    }

    #[test]
    fn 点にある仕切りを引く() {
        let (w, h, r) = (1000.0, 800.0, 0.5);
        let gap = splitter_rect(content_width(w, true), h, r);
        let at = |x: f32, y: f32| splitter_at(w, h, r, true, x, y);
        assert_eq!(at(500.0, gap.y + 1.0), Some(Splitter::Horizontal));
        assert_eq!(at(500.0, gap.y - 1.9), Some(Splitter::Horizontal), "上へはみ出す");
        assert_eq!(at(500.0, gap.y - 2.1), None);
        assert_eq!(at(500.0, gap.bottom() + 1.9), Some(Splitter::Horizontal));
        assert_eq!(at(w - 1.0, 100.0), Some(Splitter::Vertical));
        assert_eq!(at(w - SPLITTER_W - 1.9, 100.0), Some(Splitter::Vertical));
        assert_eq!(at(w - SPLITTER_W - 2.1, 100.0), None);
        assert_eq!(at(w - 1.0, gap.y), Some(Splitter::Vertical), "角は縦");
        assert_eq!(at(w, 100.0), None, "サーフェスの外");
    }

    #[test]
    fn 縦の仕切りを出さなければ右端は_timeline_のもの() {
        let (w, h, r) = (1000.0, 800.0, 0.5);
        let gap = splitter_rect(w, h, r);
        assert_eq!(splitter_at(w, h, r, false, w - 1.0, gap.y), Some(Splitter::Horizontal));
        assert_eq!(splitter_at(w, h, r, false, w - 1.0, 100.0), None);
    }

    #[test]
    fn 小さなサーフェスでも矩形が負にならない() {
        let l = split_viewers(10.0, 20.0, false);
        assert!(l.program.h >= 0.0 && l.timeline.h >= 0.0);
        assert_eq!(l.timeline.bottom(), 20.0);
    }
}
