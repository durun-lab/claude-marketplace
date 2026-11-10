//! Timeline のポインター操作。目盛りのスクラブ、レイヤーとカットの選択、端のつまみと中のドラッグ（スナップ）、
//! 拡大縮小と横の移動（ホイール、ズームバー）、縦のスクロール（Shift+ホイール、スクロールバー）。
//! キーとメニューのコマンド（[`TimelineCommand`]）と再生中のページ送りも受ける。
//!
//! 入力は Timeline の矩形の左上を (0, 0) とする座標で受け、操作の結果は [`Action`] で返す。
//! ドラッグから編集の操作と量を決める所は [`crate::gesture`]。操作の適用（`mm-ops`）はここの外で行う。
//! 当たり判定と描画が同じ値を見るよう、割り付けと行の並びもここが持つ（[`TimelineController::viewport`]）。
//! スナップの入/切とドラッグ中の反転、ドラッグ中のツールチップは `controller/snapping.rs`。

mod command;
mod tools;
mod zoom;

pub use command::{edit_target, TimelineCommand};

use crate::gesture::{body_edit, frame_secs, EditKind};
use crate::paint::PaintState;
use crate::palette::Palette;
use crate::rows::Rows;
use crate::scroll::{clamp_scroll, reveal, scroll_for_thumb};
use crate::snap::{quantize, SnapTarget};
use crate::time_view::TimeView;
use crate::timeline::{hit_test, HeaderButton, Hit, Part, TimelineModel};
use crate::timeline_layout::{TimelineLayout, Viewport};
use crate::tool::{BladeLine, Tool};
use crate::zoombar::ZoomBarDrag;
use zoom::FullExtent;

/// Shift+ホイールで縦にスクロールする量（px）。横と同じ量にする。
const SCROLL_STEP_PX: f32 = 60.0;

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
    /// Blade でカットを押した。レイヤー `item` の、時刻 `time`（秒。再生位置とフレームに合わせたもの）で割る。
    Split { item: usize, time: f64 },
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Drag {
    None,
    Scrub,
    /// 端のつまみ。`cut` が `None` ならレイヤーの端。
    Edge {
        item: usize,
        cut: Option<usize>,
        part: Part,
        mods: Mods,
    },
    /// カットの中。`from` はつかんだ時刻。
    Body {
        item: usize,
        cut: usize,
        mods: Mods,
        from: f64,
    },
    /// レイヤーの帯の中（Selection）。離したときに `shiftLayer` にする。`from` はつかんだ時刻。
    ShiftLayer {
        item: usize,
        from: f64,
    },
    /// 見出しの縦のドラッグ。
    Header(header_drag::HeaderDrag),
    /// スクロールボックスのドラッグ。`grab_offset` は押した点の、スクロールボックスの上端からの距離。
    Thumb {
        grab_offset: f32,
    },
    /// ズームバーのスクロールボックスか、その端のドラッグ。
    ZoomBar(ZoomBarDrag),
}

/// Timeline の表示と操作の状態。
#[derive(Debug, Clone)]
pub struct TimelineController {
    /// ズームと横の位置。
    pub view: TimeView,
    /// 縦のスクロール（論理ピクセル）。
    pub scroll_y: f32,
    pub hover: Hit,
    pub selected: Option<usize>,
    /// 選んだカット（レイヤーの添字、カットの添字）。
    pub selected_cut: Option<(usize, usize)>,
    /// スナップの入/切（表示の状態）。
    pub snapping: bool,
    /// 今のツール。取り消しの履歴にも `.mm/view.json` にも入れず、開くたびに Selection から始める。
    pub tool: Tool,
    /// Blade のときの、ポインターの下のカットに描く割る時刻の線。
    blade_line: Option<BladeLine>,
    rows: Rows,
    layout: TimelineLayout,
    /// Timeline の大きさ（論理ピクセル）。
    size: (f32, f32),
    /// 端のドラッグで吸い付いた目標。
    snap: Option<SnapTarget>,
    /// ドラッグ中にスナップの入/切のキーを押している。押している間だけ入/切を反転する。
    snap_held: bool,
    /// ドラッグ中のポインターの位置。入/切を反転したら、同じ位置で吸い付き直す。
    pointer: (f32, f32),
    /// ドラッグで最後に返した編集。離したときの確定にそのまま使う。
    last_edit: Option<(EditKind, f64)>,
    drag: Drag,
    /// 全体表示のコマンドで全体表示にしたときの、全体表示とその前の表示。
    full_extent: Option<FullExtent>,
}

impl TimelineController {
    pub fn new(view: TimeView) -> Self {
        Self {
            view,
            scroll_y: 0.0,
            hover: Hit::Empty,
            selected: None,
            selected_cut: None,
            snapping: true,
            tool: Tool::default(),
            blade_line: None,
            rows: Rows::default(),
            layout: TimelineLayout::default(),
            size: (0.0, 0.0),
            snap: None,
            snap_held: false,
            pointer: (0.0, 0.0),
            last_edit: None,
            drag: Drag::None,
            full_extent: None,
        }
    }

    /// 行の並びを置き直す（モデルか行の高さが変わったとき）。スクロールは範囲に収める。
    pub fn set_rows(&mut self, rows: Rows) {
        self.rows = rows;
        self.relayout();
    }

    /// Timeline の大きさ（論理ピクセル）を置き直す。
    pub fn resize(&mut self, size: (f32, f32)) {
        self.size = size;
        self.relayout();
    }

    fn relayout(&mut self) {
        self.layout = TimelineLayout::new(self.size, self.rows.content_height());
        self.scroll_to(self.scroll_y);
    }

    /// 縦のスクロールを `y` にする（範囲に収める）。
    pub fn scroll_to(&mut self, y: f32) {
        self.scroll_y = clamp_scroll(y, self.rows.content_height(), self.layout.row_area.h);
    }

    /// 行 `i` が全部見えるまでスクロールする。
    pub fn reveal(&mut self, i: usize) {
        if i < self.rows.len() {
            self.scroll_y = reveal(&self.rows, i, self.scroll_y, self.layout.row_area.h);
        }
    }

    /// 割り付け。
    pub fn layout(&self) -> &TimelineLayout {
        &self.layout
    }

    /// 見えている所。描画と当たり判定が使う。
    pub fn viewport(&self) -> Viewport<'_> {
        Viewport {
            layout: &self.layout,
            rows: &self.rows,
            time: self.view,
            scroll_y: self.scroll_y,
        }
    }

    /// 並びが変わったモデルに合わせる。範囲の外の選択を外し、古い添字を指すホバーとドラッグを捨てる。
    pub fn model_changed(&mut self, model: &TimelineModel) {
        self.selected = self.selected.filter(|i| *i < model.items.len());
        self.selected_cut = self
            .selected_cut
            .filter(|(i, c)| model.items.get(*i).is_some_and(|it| *c < it.cuts.len()));
        self.hover = Hit::Empty;
        self.blade_line = None;
        self.end_drag();
    }

    /// 描くときの状態。
    pub fn paint_state(&self, playhead: f64) -> PaintState {
        PaintState {
            playhead,
            hover: self.hover,
            selected: self.selected,
            selected_cut: self.selected_cut,
            snap: self.snap,
            preview: self.last_edit,
            drag_tooltip: self.drag_tooltip(),
            header_drop: self.header_drop(),
            palette: Palette::DEFAULT,
        }
    }

    /// 入力を 1 つ受けて、起きた操作を返す。
    pub fn handle(
        &mut self,
        ev: PointerEvent,
        model: &TimelineModel,
        playhead: f64,
    ) -> Vec<Action> {
        match ev {
            PointerEvent::Down { x, y, mods } => {
                self.press_with_tool((x, y), mods, (model, playhead))
            }
            PointerEvent::Move { x, y } => self.moved(x, y, model, playhead),
            PointerEvent::Up => self.up(model),
            PointerEvent::Wheel {
                x,
                y,
                dy,
                ctrl,
                shift,
            } => {
                self.wheel((x, y), dy, (ctrl, shift), model.duration);
                self.follow_scroll(model.items.len());
                if self.drag == Drag::None {
                    self.hover = hit_test(model, &self.viewport(), x, y);
                    self.update_blade_line(model, playhead);
                }
                Vec::new()
            }
            PointerEvent::Leave => {
                self.hover = Hit::Empty;
                self.blade_line = None;
                Vec::new()
            }
            PointerEvent::Cancel => {
                self.end_drag();
                self.update_blade_line(model, playhead);
                Vec::new()
            }
            PointerEvent::Command(command) => self.command(command, model, playhead),
            PointerEvent::SnapKeyUp => self.release_snap_key(model, playhead),
        }
    }

    fn seek_at(&self, x: f32, model: &TimelineModel) -> f64 {
        Self::seek_time(self.viewport().t_of(x), model)
    }

    /// 時刻 `t` に再生位置を動かすときの時刻。いちばん近いフレームの先頭にそろえ、コンポジションの中に収める。
    fn seek_time(t: f64, model: &TimelineModel) -> f64 {
        let frame = frame_secs(model);
        let t = quantize(t, model.fps);
        t.clamp(0.0, (model.duration - frame).max(0.0))
    }

    fn down(&mut self, x: f32, y: f32, mods: Mods, model: &TimelineModel) -> Vec<Action> {
        let hit = hit_test(model, &self.viewport(), x, y);
        match hit {
            Hit::Ruler => {
                self.drag = Drag::Scrub;
                return vec![Action::Seek(self.seek_at(x, model))];
            }
            Hit::Scrollbar { thumb } => {
                self.press_scrollbar(thumb, y);
                return Vec::new();
            }
            Hit::ZoomBar { part } => {
                self.press_zoombar(part, x, model.duration);
                return Vec::new();
            }
            // 見出しの列の上の角はタイムコードを出すだけで、選択もカットの選択も変えない。
            Hit::Corner => return Vec::new(),
            // ボタンは操作にするだけで、レイヤーの選択は変えない（Premiere と Resolve の見出しのボタンと同じ）。
            Hit::HeaderButton { index, button } => {
                return vec![Action::HeaderButton { index, button }];
            }
            _ => {}
        }
        let had_cut = self.selected_cut.take();
        match hit {
            Hit::Layer { index, part } => {
                self.selected = Some(index);
                self.drag = self.layer_press(index, part, mods, x);
                deselect_cut(Action::Select(Some(index)), had_cut)
            }
            Hit::Cut { item, cut, part } => self.press_cut((item, cut, part), x, mods),
            Hit::BarMark { item, keyframe } => {
                self.press_bar_mark((item, keyframe), had_cut, model)
            }
            Hit::Header { index } => {
                self.press_header(index, y);
                deselect_cut(Action::Select(Some(index)), had_cut)
            }
            _ => {
                self.selected = None;
                deselect_cut(Action::Select(None), had_cut)
            }
        }
    }

    /// 帯のマーカーを押した。レイヤーを選び、再生位置をそのキーフレームの時刻へ動かす（ドラッグは始めない）。
    fn press_bar_mark(
        &mut self,
        (item, keyframe): (usize, usize),
        had_cut: Option<(usize, usize)>,
        model: &TimelineModel,
    ) -> Vec<Action> {
        self.selected = Some(item);
        let t = model
            .items
            .get(item)
            .and_then(|it| it.keyframes.get(keyframe));
        let mut out = deselect_cut(Action::Select(Some(item)), had_cut);
        out.extend(t.map(|t| Action::Seek(Self::seek_time(*t, model))));
        out
    }

    /// カットを押した。レイヤーとカットを選び、端ならつまみの、中ならカットのドラッグを始める。
    fn press_cut(
        &mut self,
        (item, cut, part): (usize, usize, Part),
        x: f32,
        mods: Mods,
    ) -> Vec<Action> {
        self.selected = Some(item);
        self.selected_cut = Some((item, cut));
        self.drag = self.cut_press((item, cut, part), mods, x);
        vec![
            Action::Select(Some(item)),
            Action::SelectCut(self.selected_cut),
        ]
    }

    /// スクロールバーを押した。スクロールボックスなら掴み、溝なら押した側へ 1 画面ぶんスクロールする（Windows のスクロールバーと同じ）。
    fn press_scrollbar(&mut self, on_thumb: bool, y: f32) {
        let thumb = self.viewport().thumb();
        if on_thumb {
            self.drag = Drag::Thumb {
                grab_offset: y - thumb.y,
            };
            return;
        }
        let page = self.layout.row_area.h;
        let by = if y < thumb.y { -page } else { page };
        self.scroll_to(self.scroll_y + by);
    }

    fn moved(&mut self, x: f32, y: f32, model: &TimelineModel, playhead: f64) -> Vec<Action> {
        self.pointer = (x, y);
        match self.drag {
            Drag::None => {
                self.hover = hit_test(model, &self.viewport(), x, y);
                self.update_blade_line(model, playhead);
                Vec::new()
            }
            Drag::ShiftLayer { .. } => {
                let edit = self.shift_layer_drag(x, model, playhead);
                self.report(edit)
            }
            Drag::Scrub => vec![Action::Seek(self.seek_at(x, model))],
            Drag::Header(drag) => self.drag_header(drag, y, model.items.len()),
            Drag::Thumb { grab_offset } => {
                let content = self.rows.content_height();
                self.scroll_y = scroll_for_thumb(self.layout.scrollbar, content, y - grab_offset);
                self.hover = Hit::Scrollbar { thumb: true };
                Vec::new()
            }
            Drag::ZoomBar(drag) => {
                self.drag_zoombar(&drag, x, model.duration);
                Vec::new()
            }
            Drag::Edge { .. } => {
                let edit = self.edge_drag(x, model, playhead);
                self.report(edit)
            }
            Drag::Body {
                item,
                cut,
                mods,
                from,
            } => {
                let dt = quantize(self.viewport().t_of(x) - from, model.fps);
                self.report(body_edit(model, item, cut, mods, dt))
            }
        }
    }

    fn report(&mut self, edit: Option<(EditKind, f64)>) -> Vec<Action> {
        self.last_edit = edit;
        edit.map(|(kind, by)| Action::Edit {
            kind,
            by,
            done: false,
        })
        .into_iter()
        .collect()
    }

    fn up(&mut self, model: &TimelineModel) -> Vec<Action> {
        let (drag, last) = (self.drag, self.last_edit);
        self.end_drag();
        let frame = frame_secs(model);
        match (drag, last) {
            (Drag::Header(header), _) => header.release(),
            // 動かさずに離したクリックは編集にしない。
            (Drag::Edge { .. } | Drag::Body { .. } | Drag::ShiftLayer { .. }, Some((kind, by)))
                if by.abs() >= frame / 2.0 =>
            {
                vec![Action::Edit {
                    kind,
                    by,
                    done: true,
                }]
            }
            _ => Vec::new(),
        }
    }
}

/// レイヤーの選択の操作に、カットの選択が外れたことを添える。
fn deselect_cut(select: Action, had_cut: Option<(usize, usize)>) -> Vec<Action> {
    let mut out = vec![select];
    if had_cut.is_some() {
        out.push(Action::SelectCut(None));
    }
    out
}

mod header_drag;
mod snapping;

#[cfg(test)]
mod cut_tests;
#[cfg(test)]
mod scroll_tests;
#[cfg(test)]
mod snap_tests;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod tool_tests;
