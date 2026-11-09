//! サーフェス全体の割り付け。上に Program（素材を開いていれば左に Source）と、それぞれの下の再生の操作のバー、下に Timeline。

use crate::geom::Rect;
use crate::timeline_layout::MIN_TIMELINE_H;
use crate::transport::TRANSPORT_H;

/// Program と Timeline の間隔。
const GAP: f32 = 4.0;
/// Timeline を全体表示にしたときの左右の余白（px）。
pub const VIEW_MARGIN: f32 = 16.0;
/// Timeline の高さ（サーフェスに対する比）。中身の高さには合わせず、入らない行は縦にスクロールする。
pub const TIMELINE_RATIO: f32 = 0.45;
/// 再生の操作のバーを出すのに要る映像の高さ。上の段がこれとバーの和より低ければ、バーを出さずに映像に回す
/// （映像の見えないビューアーは使えず、再生の操作はキーとメニューにもあるため）。
const MIN_VIDEO_H: f32 = TRANSPORT_H;

/// サーフェスの割り付け。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SurfaceLayout {
    /// Program の映像を出す矩形（再生の操作のバーを除く）。
    pub program: Rect,
    /// Program の下の再生の操作のバー。
    pub program_bar: Rect,
    pub timeline: Rect,
    /// Source のビューアー。素材を開いているときだけ、Program の左に並ぶ。
    pub source: Option<Rect>,
    /// Source の下の再生の操作のバー。`source` と一緒に現れる。
    pub source_bar: Option<Rect>,
}

/// 幅 `w`、高さ `h` のサーフェスを割る。Timeline の高さはサーフェスの [`TIMELINE_RATIO`]（下限は目盛りとレイヤー 2 本）。
/// 上の段は映像の下に再生の操作のバー（[`TRANSPORT_H`]。上の段が低ければ高さ 0）を取り、`with_source` なら
/// 左の Source と右の Program に半分ずつ割る。
pub fn split_viewers(w: f32, h: f32, with_source: bool) -> SurfaceLayout {
    let h = h.max(0.0);
    let th = (h * TIMELINE_RATIO).max(MIN_TIMELINE_H).min(h);
    let ph = (h - th - GAP).max(0.0);
    let bar_h = if ph >= MIN_VIDEO_H + TRANSPORT_H {
        TRANSPORT_H
    } else {
        0.0
    };
    let vh = ph - bar_h;
    let timeline = Rect::new(0.0, h - th, w, th);
    // 映像の矩形と、その真下のバー。
    let column = |x: f32, cw: f32| (Rect::new(x, 0.0, cw, vh), Rect::new(x, vh, cw, bar_h));
    if !with_source {
        let (program, program_bar) = column(0.0, w);
        return SurfaceLayout {
            program,
            program_bar,
            timeline,
            source: None,
            source_bar: None,
        };
    }
    let half = ((w - GAP) / 2.0).max(0.0);
    let (source, source_bar) = column(0.0, half);
    let (program, program_bar) = column(half + GAP, half);
    SurfaceLayout {
        program,
        program_bar,
        timeline,
        source: Some(source),
        source_bar: Some(source_bar),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 二つの領域はサーフェスを隙間つきで埋める() {
        let l = split_viewers(1000.0, 800.0, false);
        assert_eq!(l.program_bar.bottom() + GAP, l.timeline.y);
        assert_eq!(l.timeline.bottom(), 800.0);
        assert_eq!((l.program.w, l.timeline.w), (1000.0, 1000.0));
    }

    #[test]
    fn source_を開くと上の段を左右に半分ずつ割り_timeline_は変わらない() {
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
    fn 小さなサーフェスでも矩形が負にならない() {
        let l = split_viewers(10.0, 20.0, false);
        assert!(l.program.h >= 0.0 && l.timeline.h >= 0.0);
        assert_eq!(l.timeline.bottom(), 20.0);
    }
}
