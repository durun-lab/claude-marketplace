//! 再生の操作のバー。Program と Source のそれぞれの真下に、ジョグバー（全体の上の再生位置）、時刻、再生のボタンを並べる
//! （Premiere、Resolve、Final Cut はどれもモニターの真下に時刻と再生のボタンを置く）。座標はサーフェスの論理ピクセル。
//!
//! バーの中の矩形は [`cells`] 1 か所で決め、描く関数（`transport/paint.rs`）と当たり判定（[`TransportController`]）が同じ値を見る。
//! ボタンはコマンドの表（アプリの `commands.rs`）の ID を持ち、押すとその ID を [`TransportAction::Command`] で返す。
//! 押せるかはアプリがコマンドの表から作った [`ViewerState::disabled`] で決める。

mod icons;
mod paint;

use crate::controller::PointerEvent;
use crate::geom::Rect;
use crate::screen::SurfaceLayout;

pub use paint::{transport_labels, transport_quads};

/// バーの高さ（ジョグバーとボタンの行）。
pub const TRANSPORT_H: f32 = 44.0;
/// ジョグバーの当たりの高さ。
const JOG_H: f32 = 14.0;
/// ジョグバーの左右の余白。
const JOG_PAD: f32 = 8.0;
/// ボタン 1 つの幅、ボタンの間隔、グループの間隔。
const BUTTON_W: f32 = 28.0;
const BUTTON_GAP: f32 = 2.0;
const GROUP_GAP: f32 = 10.0;
/// 時刻を出す左右の欄の幅。
const SIDE_W: f32 = 112.0;
/// 時刻の欄と矩形の端の余白。
const SIDE_PAD: f32 = 6.0;

/// バーのもち主。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Viewer {
    Program,
    Source,
}

/// ボタンの絵柄。三角、四角、棒、括弧を組み合わせて描く（`transport/icons.rs`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Icon {
    GoStart,
    StepBack,
    PlayReverse,
    /// 止まっているときは再生、再生中は停止の絵。
    PlayStop,
    PlayForward,
    StepForward,
    GoEnd,
    Stop,
    MarkIn,
    MarkOut,
    ClearMarks,
    Insert,
    Overwrite,
}

/// ボタンの定義。`command` はコマンドの表の ID。
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct ButtonSpec {
    pub command: &'static str,
    pub icon: Icon,
    /// 左に広い間隔を空ける（グループの区切り）。
    pub gap_before: bool,
}

const fn button(command: &'static str, icon: Icon, gap_before: bool) -> ButtonSpec {
    ButtonSpec {
        command,
        icon,
        gap_before,
    }
}

/// Program のバーのボタン（Premiere と Resolve の並びに倣う）。
const PROGRAM_BUTTONS: &[ButtonSpec] = &[
    button("playback.go_to_start", Icon::GoStart, false),
    button("playback.step_back", Icon::StepBack, false),
    button("playback.play_reverse", Icon::PlayReverse, true),
    button("playback.toggle", Icon::PlayStop, false),
    button("playback.play_forward", Icon::PlayForward, false),
    button("playback.step_forward", Icon::StepForward, true),
    button("playback.go_to_end", Icon::GoEnd, false),
];

/// Source のバーのボタン。範囲を決めるボタン、J/K/L、Timeline に入れるボタン。
const SOURCE_BUTTONS: &[ButtonSpec] = &[
    button("source.mark_in", Icon::MarkIn, false),
    button("source.mark_out", Icon::MarkOut, false),
    button("source.clear_in_out", Icon::ClearMarks, false),
    button("playback.play_reverse", Icon::PlayReverse, true),
    button("playback.stop", Icon::Stop, false),
    button("playback.play_forward", Icon::PlayForward, false),
    button("clip.insert", Icon::Insert, true),
    button("clip.overwrite", Icon::Overwrite, false),
];

fn buttons_of(viewer: Viewer) -> &'static [ButtonSpec] {
    match viewer {
        Viewer::Program => PROGRAM_BUTTONS,
        Viewer::Source => SOURCE_BUTTONS,
    }
}

/// 片方のバーが出す状態。時刻と長さは秒。
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ViewerState {
    pub t: f64,
    pub duration: f64,
    pub fps: f64,
    pub playing: bool,
    /// Source のイン点とアウト点（素材の秒）。Program は持たない。
    pub mark_in: Option<f64>,
    pub mark_out: Option<f64>,
    /// 今は押せないコマンドの ID。
    pub disabled: Vec<&'static str>,
}

impl ViewerState {
    fn enabled(&self, command: &str) -> bool {
        !self.disabled.contains(&command)
    }
}

/// 2 つのバーの状態。Source のバーは素材を開いているときだけある。
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TransportState {
    pub program: ViewerState,
    pub source: Option<ViewerState>,
}

impl TransportState {
    pub(crate) fn of(&self, viewer: Viewer) -> Option<&ViewerState> {
        match viewer {
            Viewer::Program => Some(&self.program),
            Viewer::Source => self.source.as_ref(),
        }
    }
}

/// 割り付けのバーの矩形。
pub(crate) fn bar_of(layout: &SurfaceLayout, viewer: Viewer) -> Option<Rect> {
    match viewer {
        Viewer::Program => Some(layout.program_bar),
        Viewer::Source => layout.source_bar,
    }
}

/// 1 つのボタンの置き場所。
#[derive(Debug, Clone, Copy)]
pub(crate) struct Slot {
    pub spec: &'static ButtonSpec,
    pub rect: Rect,
}

/// バーの中の矩形。
#[derive(Debug, Clone)]
pub(crate) struct Cells {
    /// ジョグバーの当たり（バーの上端、全幅）。
    pub jog: Rect,
    /// 時刻を出す左と右の欄（ボタンの行）。
    pub time: Rect,
    pub side: Rect,
    pub buttons: Vec<Slot>,
}

impl Cells {
    /// ジョグバーの線（当たりの左右を余白の分だけ縮めた所）。
    pub(crate) fn track(&self) -> Rect {
        Rect::new(
            self.jog.x + JOG_PAD,
            self.jog.y,
            (self.jog.w - 2.0 * JOG_PAD).max(0.0),
            self.jog.h,
        )
    }
}

/// `bar` の中の矩形。ボタンは時刻の欄の間の中央に寄せ、足りなければ幅を縮める。
pub(crate) fn cells(bar: Rect, viewer: Viewer) -> Cells {
    let jog = Rect::new(bar.x, bar.y, bar.w, JOG_H.min(bar.h));
    let row = Rect::new(bar.x, jog.bottom(), bar.w, (bar.h - jog.h).max(0.0));
    let side_w = SIDE_W.min(row.w / 3.0);
    let time = Rect::new(row.x, row.y, side_w, row.h);
    let side = Rect::new(row.right() - side_w, row.y, side_w, row.h);
    let specs = buttons_of(viewer);
    let gaps: f32 = specs
        .iter()
        .enumerate()
        .map(|(i, s)| match (i, s.gap_before) {
            (0, _) => 0.0,
            (_, true) => GROUP_GAP,
            _ => BUTTON_GAP,
        })
        .sum();
    let room = (row.w - 2.0 * side_w - gaps).max(0.0);
    let w = BUTTON_W.min(room / specs.len() as f32);
    let total = w * specs.len() as f32 + gaps;
    let mut x = row.x + (row.w - total) / 2.0;
    let buttons = specs
        .iter()
        .enumerate()
        .map(|(i, spec)| {
            if i > 0 {
                x += if spec.gap_before {
                    GROUP_GAP
                } else {
                    BUTTON_GAP
                };
            }
            let rect = Rect::new(x, row.y, w, row.h);
            x += w;
            Slot { spec, rect }
        })
        .collect();
    Cells {
        jog,
        time,
        side,
        buttons,
    }
}

/// バーの上のポインターの下にあるもの。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    Jog,
    Button(&'static str),
}

/// `(x, y)` の下にあるもの。
pub(crate) fn hit(bar: Rect, viewer: Viewer, x: f32, y: f32) -> Option<Target> {
    let c = cells(bar, viewer);
    if c.jog.contains(x, y) {
        return Some(Target::Jog);
    }
    c.buttons
        .iter()
        .find(|b| b.rect.contains(x, y))
        .map(|b| Target::Button(b.spec.command))
}

/// ジョグバーの `x` の時刻。線の外は端に寄せる。
pub(crate) fn jog_time(track: Rect, duration: f64, x: f32) -> f64 {
    if track.w <= 0.0 {
        return 0.0;
    }
    f64::from(((x - track.x) / track.w).clamp(0.0, 1.0)) * duration.max(0.0)
}

/// バーの操作の結果。
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TransportAction {
    /// ボタンのコマンドを実行する（コマンドの表の ID）。
    Command(&'static str),
    /// ジョグバーで再生位置を動かす（`viewer` の秒）。
    Seek { viewer: Viewer, t: f64 },
}

/// バーのポインター操作。ジョグバーのドラッグ（押したバーに結び付く）とポインターの下の部品を覚える。
#[derive(Debug, Default)]
pub struct TransportController {
    drag: Option<Viewer>,
    hover: Option<(Viewer, Target)>,
}

impl TransportController {
    /// ポインターが乗っている部品。
    pub fn hover(&self) -> Option<(Viewer, Target)> {
        self.hover
    }

    /// 論理ピクセルのポインターの入力を受ける。ボタンは押したとき、ジョグバーは押したときとドラッグ中にシークする。
    pub fn handle(
        &mut self,
        ev: PointerEvent,
        layout: &SurfaceLayout,
        state: &TransportState,
    ) -> Vec<TransportAction> {
        match ev {
            PointerEvent::Down { x, y, .. } => self.down(x, y, layout, state),
            PointerEvent::Move { x, y } => {
                self.hover = under(layout, x, y);
                self.seek(x, layout, state).into_iter().collect()
            }
            PointerEvent::Up | PointerEvent::Cancel => {
                self.drag = None;
                Vec::new()
            }
            PointerEvent::Leave => {
                self.hover = None;
                Vec::new()
            }
            PointerEvent::Wheel { .. } => Vec::new(),
        }
    }

    fn down(
        &mut self,
        x: f32,
        y: f32,
        layout: &SurfaceLayout,
        state: &TransportState,
    ) -> Vec<TransportAction> {
        let Some((viewer, target)) = under(layout, x, y) else {
            return Vec::new();
        };
        match target {
            Target::Jog => {
                self.drag = Some(viewer);
                self.seek(x, layout, state).into_iter().collect()
            }
            Target::Button(command) => {
                let enabled = state.of(viewer).is_some_and(|s| s.enabled(command));
                enabled
                    .then_some(TransportAction::Command(command))
                    .into_iter()
                    .collect()
            }
        }
    }

    /// ドラッグ中のジョグバーの `x` へのシーク。
    fn seek(
        &self,
        x: f32,
        layout: &SurfaceLayout,
        state: &TransportState,
    ) -> Option<TransportAction> {
        let viewer = self.drag?;
        let bar = bar_of(layout, viewer)?;
        let duration = state.of(viewer)?.duration;
        if duration <= 0.0 {
            return None;
        }
        let t = jog_time(cells(bar, viewer).track(), duration, x);
        Some(TransportAction::Seek { viewer, t })
    }
}

/// `(x, y)` の下のバーの部品。
fn under(layout: &SurfaceLayout, x: f32, y: f32) -> Option<(Viewer, Target)> {
    [Viewer::Program, Viewer::Source].into_iter().find_map(|v| {
        let bar = bar_of(layout, v).filter(|b| b.contains(x, y))?;
        hit(bar, v, x, y).map(|t| (v, t))
    })
}

#[cfg(test)]
mod tests;
