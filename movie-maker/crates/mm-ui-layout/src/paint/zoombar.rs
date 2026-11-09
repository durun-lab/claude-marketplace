//! ズームバー。溝は目盛りの帯の色で、上端に行の領域との境の線を引く。スクロールボックスは縦のスクロールバーと同じ
//! 目盛りの時刻の色（溝とのコントラストの比が WCAG 2 の部品の下限の 3 以上）で、両端の内側に溝の色の縦の線を 2 本ずつ
//! 引き（つまみの模様）、端を掴めることを示す。ポインターが乗っているか掴んでいる間は、スクロールボックスを選んだ外枠の色に
//! する。再生位置は、溝とスクロールボックスの上に再生位置の色の細い線で出す（見えている範囲の外にあっても位置が分かる）。

use super::{Canvas, Depth, PaintInput};
use crate::geom::Rect;
use crate::timeline::Hit;
use crate::zoombar;

/// スクロールボックスと溝の縁の間隔。
const THUMB_INSET: f32 = 2.0;
/// 端を示す線の幅、2 本の線の間隔、スクロールボックスの縁からの距離。
const GRIP_W: f32 = 1.0;
const GRIP_GAP: f32 = 2.0;
const GRIP_INSET: f32 = 2.0;
/// 再生位置の線の幅。
const PLAYHEAD_W: f32 = 2.0;

pub(super) fn paint(s: &PaintInput, c: &mut Canvas) {
    let l = s.vp.layout;
    let track = l.zoombar;
    if track.h <= 0.0 {
        return;
    }
    let p = &s.state.palette;
    // 縦のスクロールバーの下の角も溝の色で埋め、ズームバーが右端まで続いて見えるようにする。
    let corner = Rect::new(l.scrollbar.x, track.y, l.scrollbar.w, track.h);
    c.push(Depth::Surround, corner, p.ruler);
    c.push(Depth::Surround, track, p.ruler);
    // テーマでは溝と奇数の行の縞が同じ色になるので、上端に目盛りの線の色の境を引く。
    let edge = Rect::new(track.x, track.y, track.w + corner.w, 1.0);
    c.push(Depth::Surround, edge, p.tick);
    if track.w <= 0.0 {
        return;
    }
    let thumb = zoombar::thumb_of(s.vp, s.model.duration).inset(THUMB_INSET);
    let lit = matches!(s.state.hover, Hit::ZoomBar { part } if part.on_thumb());
    c.push(
        Depth::Surround,
        thumb,
        if lit { p.selection } else { p.label },
    );
    let grip_h = thumb.h - 2.0 * GRIP_INSET;
    for from_edge in [GRIP_INSET, GRIP_INSET + GRIP_GAP] {
        for x in [thumb.x + from_edge, thumb.right() - from_edge - GRIP_W] {
            let grip = Rect::new(x, thumb.y + GRIP_INSET, GRIP_W, grip_h);
            c.push_in(Depth::Surround, grip, p.ruler, thumb);
        }
    }
    let range = zoombar::extent(s.model.duration, &s.vp.time, l.row_area.w);
    let x = zoombar::x_of(track, range, s.state.playhead);
    let line = Rect::new(x - PLAYHEAD_W / 2.0, track.y, PLAYHEAD_W, track.h);
    c.push_in(Depth::Surround, line, p.playhead, track);
}
