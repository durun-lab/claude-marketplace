//! Timeline への入力（[`PointerEvent`]）と、操作の結果（[`Action`]）。

use super::TimelineCommand;
use crate::gesture::EditKind;
use crate::keyframe_rows::KeyframeAction;
use crate::timeline::{HeaderButton, Hit};

/// 押したときの修飾キー。ドラッグの種類を選ぶ（カットの端は `alt` で roll、中は `shift` で slide）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Mods {
    pub shift: bool,
    pub alt: bool,
}

/// Timeline への入力。ポインターと、ドラッグに効くキー（取り消しとスナップの入/切）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PointerEvent {
    Down {
        x: f32,
        y: f32,
        mods: Mods,
    },
    Move {
        x: f32,
        y: f32,
    },
    Up,
    /// `dy` が正で奥へ（拡大、右への移動、上へのスクロール）。`shift` で縦にスクロールする。
    Wheel {
        x: f32,
        y: f32,
        dy: f32,
        ctrl: bool,
        shift: bool,
    },
    /// ポインターが Timeline から出た。
    Leave,
    /// 離す前にドラッグが終わった（Esc、ほかのウィンドウがキャプチャーを取った）。編集を確定せずに捨てる。
    Cancel,
    /// キーとメニューの Timeline のコマンド（拡大縮小、全体表示、前後の帯の端、スナップの入/切）。ドラッグとの前後を
    /// 保つため、ポインターの入力のキューで届く。
    Command(TimelineCommand),
    /// ドラッグ中に押したスナップの入/切のキーを離した。
    SnapKeyUp,
    /// 点 `(x, y)` の右クリック。ポインターの下のカット、レイヤー、キーフレームの行を選び、[`Action::ContextMenu`] を返す。
    ContextMenu {
        x: f32,
        y: f32,
    },
    /// キーボードのメニューキー（Shift+F10、アプリケーションキー）。選んでいるものの位置で [`Action::ContextMenu`] を返す。
    ContextMenuKey,
}

/// 操作の結果。
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Action {
    /// 再生位置を動かす（秒。フレームの境目）。
    Seek(f64),
    /// レイヤーの選択が変わった。
    Select(Option<usize>),
    /// シーケンスのカットの選択（レイヤーの添字、カットの添字）。外れたら `None`。
    SelectCut(Option<(usize, usize)>),
    /// 端のつまみや中のドラッグが表す編集。`by` はコンポジションの時間軸の秒（スナップ後。フレームへの丸めは受け取る側）。
    /// 確定するのは `done` のときだけで、それまではドラッグ中の表示（スナップのガイド線）に使う。
    Edit { kind: EditKind, by: f64, done: bool },
    /// 見出しのボタンを押した。行の添字とボタン。受け取る側がレイヤーの今の状態から操作にする。
    HeaderButton { index: usize, button: HeaderButton },
    /// 見出しのドラッグを離した。行 `from` を行の境界 `boundary`（0 が先頭の行の上、行の数が最後の行の下）へ動かす。
    MoveLayer { from: usize, boundary: usize },
    /// キーフレームの行の操作（`controller/keyframe_rows.rs`）。
    Keyframe(KeyframeAction),
    /// Blade でカットを押した。レイヤー `item` の、時刻 `time`（秒。再生位置とフレームに合わせたもの）で割る。
    Split { item: usize, time: f64 },
    /// 右クリックのメニューを出す。`hit` は当たった所（添字を含む。`HitKind::of` で右クリックのメニューの種類にする）。
    /// `time` は Split Clip Here の割る時刻（秒。Blade と同じく Snapping が入なら再生位置に吸い付け、フレームにそろえる）で、
    /// 割れるカットの中を右クリックしたときだけ持つ。`at` はメニューを出す点（Timeline の座標）。ポインターの下のものは、
    /// この前の `Select`、`SelectCut`、`Keyframe(Select)` で選んである。
    ContextMenu {
        hit: Hit,
        time: Option<f64>,
        at: (f32, f32),
    },
}
