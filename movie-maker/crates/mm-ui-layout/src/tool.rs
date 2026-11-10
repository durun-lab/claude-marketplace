//! Timeline のツール（Selection、Trim、Blade）。帯の中のドラッグとクリックが何になるかを選ぶ（Resolve の Selection Mode、
//! Trim Edit Mode、Blade Edit Mode と、Final Cut Pro の A、T、B と同じ）。取り消しの履歴には入れず、表示の状態として
//! `.mm/view.json` にも残さない（開くたびに Selection から始める）。

/// 今のツール。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Tool {
    /// レイヤーの帯は時間の上で動かし、カットは選ぶ。
    #[default]
    Selection,
    /// カットの中身をずらす（`slip`、Shift で `slide`）。
    Trim,
    /// カットをポインターの時刻で割る。
    Blade,
}

impl Tool {
    /// ツールを選ぶコマンドの表の ID。ツールバーのボタンと、今のツールを入の見た目にする判定が同じ値を見る。
    pub const fn command(self) -> &'static str {
        match self {
            Self::Selection => "timeline.selection_tool",
            Self::Trim => "timeline.trim_tool",
            Self::Blade => "timeline.blade_tool",
        }
    }
}

/// Blade のときにポインターの下のカットへ描く、割る時刻の縦線（Final Cut Pro の Blade の skimmer の線と同じ考え）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BladeLine {
    /// レイヤーの添字。線はその行の帯の高さに描く。
    pub item: usize,
    /// 割る時刻（秒。スナップとフレームへの丸めの後）。
    pub time: f64,
    /// 再生位置に吸い付いた。線をガイド線の色にする（FCP の skimmer も吸い付いたときだけ黄色になる）。
    pub snapped: bool,
}
