//! 再生の操作のバー。Program と Source のそれぞれの映像の領域の真下に、タイムコードの行、ジョグバー（全体の上の再生位置）、
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
pub use paint::{transport_labels, transport_quads, transport_timecodes};

/// バーの高さ（タイムコードの行、ジョグバー、ボタンの行）。
pub const TRANSPORT_H: f32 = TIMECODE_ROW_H + JOG_H + BUTTON_ROW_H;
/// タイムコードの行の高さ。字の大きさ 12 の 1 行（15）が収まる。
const TIMECODE_ROW_H: f32 = 16.0;
/// ジョグバーの、クリックを受ける高さ。
const JOG_H: f32 = 14.0;
/// ボタンの行の高さ。
const BUTTON_ROW_H: f32 = 26.0;
/// タイムコードの行、ジョグバーの溝、ボタンの行の左右の余白。
const PAD: f32 = 8.0;
/// 再生位置のタイムコードを出す、タイムコードの行の左の欄の幅の上限。
const POSITION_W: f32 = 100.0;
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
    ClearInOut,
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

/// Program のバーのボタン。Resolve の Viewer の並び（Go to Start、Play Reverse、Stop、Play Forward、Go to End）に、
/// Premiere の Step One Frame Back / Forward を足す。再生と停止を別のボタンにし、同じ形のボタンを 2 つ置かない
/// （Space は今どおり再生と停止を切り替える）。
const PROGRAM_BUTTONS: &[ButtonSpec] = &[
    icon_button("playback.go_to_start", Icon::GoStart, false),
    icon_button("playback.step_back", Icon::StepBack, false),
    icon_button("playback.play_reverse", Icon::PlayReverse, true),
    icon_button("playback.stop", Icon::Stop, false),
    icon_button("playback.play_forward", Icon::PlayForward, false),
    icon_button("playback.step_forward", Icon::StepForward, true),
    icon_button("playback.go_to_end", Icon::GoEnd, false),
];

/// Source のバーのボタン。イン点とアウト点のボタン、Play Reverse、Stop、Play Forward（J、K、L）、Insert と Overwrite のボタン。
const SOURCE_BUTTONS: &[ButtonSpec] = &[
    named_button("source.mark_in", Icon::MarkIn, "In", false),
    named_button("source.mark_out", Icon::MarkOut, "Out", false),
    named_button("source.clear_in_out", Icon::ClearInOut, "Clear", false),
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

/// 1 つのバーが出す状態。再生位置と長さは秒。
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

/// 割り付けの、`viewer` の再生の操作のバーの矩形。ビューアーの領域が低くてバーを出していなければ `None`。
pub(crate) fn transport_rect(layout: &SurfaceLayout, viewer: Viewer) -> Option<Rect> {
    let bar = match viewer {
        Viewer::Program => Some(layout.program_transport),
        Viewer::Source => layout.source_transport,
    };
    bar.filter(|b| b.h > 0.0 && b.w > 0.0)
}

/// 1 つのボタンの矩形と定義。
#[derive(Debug, Clone, Copy)]
pub(crate) struct ButtonCell {
    pub spec: &'static ButtonSpec,
    pub rect: Rect,
    /// アイコンの代わりに名前を出す。
    pub named: bool,
}

/// バーの部品の矩形。
#[derive(Debug, Clone)]
pub(crate) struct Cells {
    /// タイムコードの行の左の欄（再生位置）と右の欄（Program は全体の長さ、Source はイン点とアウト点）。
    pub position: Rect,
    pub range: Rect,
    /// ジョグバーのクリックを受ける矩形（バーの全幅）。
    pub jog: Rect,
    pub buttons: Vec<ButtonCell>,
}

impl Cells {
    /// ジョグバーの溝（クリックを受ける矩形の左右を余白の分だけ縮めた所）。
    pub(crate) fn track(&self) -> Rect {
        Rect::new(
            self.jog.x + PAD,
            self.jog.y,
            (self.jog.w - 2.0 * PAD).max(0.0),
            self.jog.h,
        )
    }
}

/// 再生の操作のバー `bar` の部品の矩形。上からタイムコードの行、ジョグバー、ボタンの行。
pub(crate) fn cells(bar: Rect, viewer: Viewer) -> Cells {
    let (x, w) = (bar.x + PAD, (bar.w - 2.0 * PAD).max(0.0));
    let timecode_h = TIMECODE_ROW_H.min(bar.h);
    let position_w = POSITION_W.min(w / 2.0);
    let position = Rect::new(x, bar.y, position_w, timecode_h);
    let range = Rect::new(x + position_w, bar.y, w - position_w, timecode_h);
    let jog = Rect::new(
        bar.x,
        position.bottom(),
        bar.w,
        JOG_H.min(bar.h - timecode_h),
    );
    let row = Rect::new(x, jog.bottom(), w, (bar.bottom() - jog.bottom()).max(0.0));
    Cells {
        position,
        range,
        jog,
        buttons: place_buttons(row, buttons_of(viewer)),
    }
}

/// ボタンを行の中央に並べる。名前のボタンは、全部の名前が入るときだけ名前で出し、入らなければアイコンにする。
/// アイコンでも入らなければ、幅を縮める。
fn place_buttons(row: Rect, specs: &'static [ButtonSpec]) -> Vec<ButtonCell> {
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
            ButtonCell {
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

/// 再生の操作のバーの部品。ジョグバーか、コマンドの表の ID を持つボタン。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportPart {
    Jog,
    Button(&'static str),
}

/// バー `bar` の、`(x, y)` の下の部品。
pub(crate) fn part_in(bar: Rect, viewer: Viewer, x: f32, y: f32) -> Option<TransportPart> {
    let c = cells(bar, viewer);
    if c.jog.contains(x, y) {
        return Some(TransportPart::Jog);
    }
    c.buttons
        .iter()
        .find(|b| b.rect.contains(x, y))
        .map(|b| TransportPart::Button(b.spec.command))
}

/// `(x, y)` の下の再生の操作のバーのビューアーと、そのバーの部品。
pub(crate) fn part_at(layout: &SurfaceLayout, x: f32, y: f32) -> Option<(Viewer, TransportPart)> {
    [Viewer::Program, Viewer::Source].into_iter().find_map(|v| {
        let bar = transport_rect(layout, v).filter(|b| b.contains(x, y))?;
        part_in(bar, v, x, y).map(|part| (v, part))
    })
}

/// ジョグバーの `x` の時刻。溝の外は端に寄せる。
pub(crate) fn jog_time(track: Rect, duration: f64, x: f32) -> f64 {
    if track.w <= 0.0 {
        return 0.0;
    }
    f64::from(((x - track.x) / track.w).clamp(0.0, 1.0)) * duration.max(0.0)
}

/// ボタン `command` の矩形（論理ピクセル）。バーを出していないか、そのバーにボタンが無ければ `None`。
pub fn transport_button(layout: &SurfaceLayout, viewer: Viewer, command: &str) -> Option<Rect> {
    let bar = transport_rect(layout, viewer)?;
    cells(bar, viewer)
        .buttons
        .into_iter()
        .find(|b| b.spec.command == command)
        .map(|b| b.rect)
}

/// ジョグバーの溝（論理ピクセル）。左端が 0 秒、右端が全体の長さ。バーを出していなければ `None`。
pub fn jog_track(layout: &SurfaceLayout, viewer: Viewer) -> Option<Rect> {
    transport_rect(layout, viewer).map(|bar| cells(bar, viewer).track())
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
