//! 再生の操作のバー。Program と Source のそれぞれの映像の真下に、時刻の行、ジョグバー（全体の上の再生位置）、
//! ボタンの行を上から重ねる（Premiere の Program モニターの下端と同じ並び）。座標はサーフェスの論理ピクセル。
//!
//! バーの中の矩形は [`cells`] 1 か所で決め、描く関数（`transport/paint.rs`）、当たり判定（[`TransportController`]）、
//! `--play-test` のクリックの位置（[`transport_button`]、[`jog_track`]）が同じ値を見る。
//! ボタンはコマンドの表（アプリの `commands.rs`）の ID を持ち、押して同じボタンの上で離すと、その ID を
//! [`TransportAction::Command`] で返す。押せるかはアプリがコマンドの表から作った [`ViewerState::disabled`] で決める。

mod controller;
mod icons;
mod paint;

use crate::geom::Rect;
use crate::screen::SurfaceLayout;

pub use controller::TransportController;
pub use paint::{transport_labels, transport_quads};

/// バーの高さ（時刻の行、ジョグバー、ボタンの行）。
pub const TRANSPORT_H: f32 = TIME_H + JOG_H + ROW_H;
/// 時刻の行の高さ。字の大きさ 12 の 1 行（15）が収まる。
const TIME_H: f32 = 16.0;
/// ジョグバーの、クリックを受ける高さ。
const JOG_H: f32 = 14.0;
/// ボタンの行の高さ。
const ROW_H: f32 = 26.0;
/// 時刻の行、ジョグバーの線、ボタンの行の左右の余白。
const PAD: f32 = 8.0;
/// 再生位置の時刻を出す、時刻の行の左の欄の幅の上限。
const NOW_W: f32 = 100.0;
/// アイコンのボタンの幅、ボタンの間隔、グループの間隔。
const BUTTON_W: f32 = 28.0;
const BUTTON_GAP: f32 = 2.0;
const GROUP_GAP: f32 = 10.0;
/// 名前のボタンの幅の見積もり。字の大きさ 12 の英字 1 字の平均の幅（Tahoma で約 6）と、名前の左右の余白。
const NAME_CHAR_W: f32 = 6.0;
const NAME_PAD: f32 = 6.0;

/// バーを付けるビューアー。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Viewer {
    Program,
    Source,
}

/// ボタンのアイコン。三角、四角、縦線、括弧、矢印を組み合わせて描く（`transport/icons.rs`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Icon {
    GoStart,
    StepBack,
    PlayReverse,
    Stop,
    PlayForward,
    StepForward,
    GoEnd,
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
    /// 幅が足りるときにアイコンの代わりに出す短い名前。再生の記号（三角と四角）で通じないボタンだけが持つ。
    pub name: Option<&'static str>,
    /// 左に広い間隔を空ける（グループの区切り）。
    pub gap_before: bool,
}

const fn icon_button(command: &'static str, icon: Icon, gap_before: bool) -> ButtonSpec {
    ButtonSpec {
        command,
        icon,
        name: None,
        gap_before,
    }
}

const fn named_button(
    command: &'static str,
    icon: Icon,
    name: &'static str,
    gap_before: bool,
) -> ButtonSpec {
    ButtonSpec {
        command,
        icon,
        name: Some(name),
        gap_before,
    }
}

/// Program のバーのボタン。Resolve の Viewer の並び（先頭、逆再生、停止、再生、末尾）に、Premiere の 1 フレームの送りを足す。
/// 再生と停止を別のボタンにし、同じ形のボタンを 2 つ置かない（Space は今どおり再生と停止を切り替える）。
const PROGRAM_BUTTONS: &[ButtonSpec] = &[
    icon_button("playback.go_to_start", Icon::GoStart, false),
    icon_button("playback.step_back", Icon::StepBack, false),
    icon_button("playback.play_reverse", Icon::PlayReverse, true),
    icon_button("playback.stop", Icon::Stop, false),
    icon_button("playback.play_forward", Icon::PlayForward, false),
    icon_button("playback.step_forward", Icon::StepForward, true),
    icon_button("playback.go_to_end", Icon::GoEnd, false),
];

/// Source のバーのボタン。イン点とアウト点のボタン、J/K/L、Timeline に入れるボタン。
const SOURCE_BUTTONS: &[ButtonSpec] = &[
    named_button("source.mark_in", Icon::MarkIn, "In", false),
    named_button("source.mark_out", Icon::MarkOut, "Out", false),
    named_button("source.clear_in_out", Icon::ClearMarks, "Clear", false),
    icon_button("playback.play_reverse", Icon::PlayReverse, true),
    icon_button("playback.stop", Icon::Stop, false),
    icon_button("playback.play_forward", Icon::PlayForward, false),
    named_button("clip.insert", Icon::Insert, "Insert", true),
    named_button("clip.overwrite", Icon::Overwrite, "Overwrite", false),
];

fn buttons_of(viewer: Viewer) -> &'static [ButtonSpec] {
    match viewer {
        Viewer::Program => PROGRAM_BUTTONS,
        Viewer::Source => SOURCE_BUTTONS,
    }
}

/// 1 つのバーが出す状態。時刻と長さは秒。
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ViewerState {
    pub t: f64,
    pub duration: f64,
    pub fps: f64,
    /// 順方向に再生している（Play Forward を入の見た目にする）。
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

/// 割り付けのバーの矩形。上の段が低くてバーを出していなければ `None`。
pub(crate) fn bar_of(layout: &SurfaceLayout, viewer: Viewer) -> Option<Rect> {
    let bar = match viewer {
        Viewer::Program => Some(layout.program_bar),
        Viewer::Source => layout.source_bar,
    };
    bar.filter(|b| b.h > 0.0 && b.w > 0.0)
}

/// 1 つのボタンの矩形。
#[derive(Debug, Clone, Copy)]
pub(crate) struct Slot {
    pub spec: &'static ButtonSpec,
    pub rect: Rect,
    /// アイコンの代わりに名前を出す。
    pub named: bool,
}

/// バーの中の矩形。
#[derive(Debug, Clone)]
pub(crate) struct Cells {
    /// 時刻の行の左（再生位置）と右（Program は全体の長さ、Source はイン点とアウト点）。
    pub now: Rect,
    pub extra: Rect,
    /// ジョグバーのクリックを受ける矩形（バーの全幅）。
    pub jog: Rect,
    pub buttons: Vec<Slot>,
}

impl Cells {
    /// ジョグバーの全体の線（クリックを受ける矩形の左右を余白の分だけ縮めた所）。
    pub(crate) fn track(&self) -> Rect {
        Rect::new(
            self.jog.x + PAD,
            self.jog.y,
            (self.jog.w - 2.0 * PAD).max(0.0),
            self.jog.h,
        )
    }
}

/// `bar` の中の矩形。上から時刻の行、ジョグバー、ボタンの行。
pub(crate) fn cells(bar: Rect, viewer: Viewer) -> Cells {
    let (x, w) = (bar.x + PAD, (bar.w - 2.0 * PAD).max(0.0));
    let time_h = TIME_H.min(bar.h);
    let now_w = NOW_W.min(w / 2.0);
    let now = Rect::new(x, bar.y, now_w, time_h);
    let extra = Rect::new(x + now_w, bar.y, w - now_w, time_h);
    let jog = Rect::new(bar.x, now.bottom(), bar.w, JOG_H.min(bar.h - time_h));
    let row = Rect::new(x, jog.bottom(), w, (bar.bottom() - jog.bottom()).max(0.0));
    Cells {
        now,
        extra,
        jog,
        buttons: place_buttons(row, buttons_of(viewer)),
    }
}

/// ボタンを行の中央に並べる。名前のボタンは、全部の名前が入るときだけ名前で出し、入らなければアイコンにする。
/// アイコンでも入らなければ、幅を縮める。
fn place_buttons(row: Rect, specs: &'static [ButtonSpec]) -> Vec<Slot> {
    let gaps: f32 = specs.iter().skip(1).map(gap_before).sum();
    let named_total = specs.iter().map(named_width).sum::<f32>() + gaps;
    let named = specs.iter().any(|s| s.name.is_some()) && named_total <= row.w;
    let icon_w = BUTTON_W.min(((row.w - gaps) / specs.len() as f32).max(0.0));
    let width = |s: &ButtonSpec| if named { named_width(s) } else { icon_w };
    let total = specs.iter().map(width).sum::<f32>() + gaps;
    let mut x = row.x + (row.w - total) / 2.0;
    specs
        .iter()
        .enumerate()
        .map(|(i, spec)| {
            if i > 0 {
                x += gap_before(spec);
            }
            let rect = Rect::new(x, row.y, width(spec), row.h);
            x += rect.w;
            Slot {
                spec,
                rect,
                named: named && spec.name.is_some(),
            }
        })
        .collect()
}

fn gap_before(spec: &ButtonSpec) -> f32 {
    if spec.gap_before {
        GROUP_GAP
    } else {
        BUTTON_GAP
    }
}

/// 名前で出すときのボタンの幅。名前の無いボタンはアイコンの幅。
fn named_width(spec: &ButtonSpec) -> f32 {
    spec.name.map_or(BUTTON_W, |n| {
        (n.chars().count() as f32 * NAME_CHAR_W + 2.0 * NAME_PAD).max(BUTTON_W)
    })
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

/// `(x, y)` の下のバーとその部品。
pub(crate) fn under(layout: &SurfaceLayout, x: f32, y: f32) -> Option<(Viewer, Target)> {
    [Viewer::Program, Viewer::Source].into_iter().find_map(|v| {
        let bar = bar_of(layout, v).filter(|b| b.contains(x, y))?;
        hit(bar, v, x, y).map(|t| (v, t))
    })
}

/// ジョグバーの `x` の時刻。線の外は端に寄せる。
pub(crate) fn jog_time(track: Rect, duration: f64, x: f32) -> f64 {
    if track.w <= 0.0 {
        return 0.0;
    }
    f64::from(((x - track.x) / track.w).clamp(0.0, 1.0)) * duration.max(0.0)
}

/// ボタン `command` の矩形（論理ピクセル）。バーを出していないか、そのバーにボタンが無ければ `None`。
pub fn transport_button(layout: &SurfaceLayout, viewer: Viewer, command: &str) -> Option<Rect> {
    let bar = bar_of(layout, viewer)?;
    cells(bar, viewer)
        .buttons
        .into_iter()
        .find(|b| b.spec.command == command)
        .map(|b| b.rect)
}

/// ジョグバーの全体の線（論理ピクセル）。左端が 0 秒、右端が全体の長さ。バーを出していなければ `None`。
pub fn jog_track(layout: &SurfaceLayout, viewer: Viewer) -> Option<Rect> {
    bar_of(layout, viewer).map(|bar| cells(bar, viewer).track())
}

/// バーの操作の結果。
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TransportAction {
    /// ボタンのコマンドを実行する（コマンドの表の ID）。
    Command(&'static str),
    /// ジョグバーで再生位置を動かす（`viewer` の秒）。
    Seek { viewer: Viewer, t: f64 },
}

#[cfg(test)]
mod tests;
