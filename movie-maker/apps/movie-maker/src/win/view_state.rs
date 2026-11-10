//! Timeline の表示の状態（ズーム、スクロール、行の高さ、スナップの入/切）を `.mm/view.json` に置く。
//! プロジェクトのデータではないので操作にせず、取り消しの列と `versions/` にも入らない。消すと既定の表示に戻る。
//!
//! 描画スレッドが変わった状態を [`ViewSlot::publish`] で置き、書くスレッドが変化の落ち着いたところで書く。
//! 閉じるときは描画スレッドが止まる前に [`ViewSlot::flush`] で書き残す。自動試験は今の状態を [`ViewSlot::current`] で読む。

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use mm_types::CompositionId;
use mm_ui_layout::{
    RowHeight, RowHeights, TimeView, ViewState, MAX_PX_PER_SEC, MIN_FIT_PX_PER_SEC,
};
use serde::{Deserialize, Serialize};

use crate::win::state::Shared;

/// 書く前に変化が止まるのを待つ時間。スクロールやズームの途中で何度も書かない。
const SETTLE: Duration = Duration::from_millis(500);
/// ファイルの形のバージョン。
const VERSION: u32 = 1;
/// プロジェクトのフォルダーの下の置き場所。
const DIR: &str = ".mm";
const FILE: &str = "view.json";

/// `.mm/view.json` の中身。コンポジションごとの Timeline の表示の状態。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ViewFile {
    version: u32,
    #[serde(default)]
    timelines: BTreeMap<String, TimelineView>,
}

/// 1 つのコンポジションの Timeline の表示の状態。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TimelineView {
    /// 左端の時刻（秒）。
    origin: f64,
    /// 1 秒の論理ピクセル。
    px_per_sec: f64,
    /// 縦のスクロール（論理ピクセル）。
    scroll_y: f32,
    /// 全部の行の高さ（View > Track Height）。無ければ既定。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    track_height: Option<Height>,
    /// レイヤーごとの行の高さ。キーはレイヤーの ID。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    layer_heights: BTreeMap<String, Height>,
    /// スナップの入/切。無ければ入（Premiere と Resolve の既定と同じ）。
    #[serde(default = "snapping_on")]
    snapping: bool,
    /// Shy のフラグを立てたレイヤーの ID。
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    shy: BTreeSet<String>,
    /// フラグを立てた行を Timeline から外す。
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    hide_shy: bool,
}

fn snapping_on() -> bool {
    true
}

/// 行の高さの、ファイルの中の名前。
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum Height {
    Small,
    Medium,
    Large,
}

impl From<RowHeight> for Height {
    fn from(h: RowHeight) -> Self {
        match h {
            RowHeight::Small => Self::Small,
            RowHeight::Medium => Self::Medium,
            RowHeight::Large => Self::Large,
        }
    }
}

impl From<Height> for RowHeight {
    fn from(h: Height) -> Self {
        match h {
            Height::Small => Self::Small,
            Height::Medium => Self::Medium,
            Height::Large => Self::Large,
        }
    }
}

impl TimelineView {
    fn of(s: &ViewState, shy: &Shy) -> Self {
        Self {
            shy: shy.layers.clone(),
            hide_shy: shy.hide,
            origin: s.time.origin,
            px_per_sec: s.time.px_per_sec,
            scroll_y: s.scroll_y,
            track_height: s.heights.all.map(Height::from),
            layer_heights: s
                .heights
                .layers
                .iter()
                .map(|(k, v)| (k.clone(), (*v).into()))
                .collect(),
            snapping: s.snapping,
        }
    }

    fn shy(&self) -> Shy {
        Shy {
            layers: self.shy.clone(),
            hide: self.hide_shy,
        }
    }

    /// 表示の状態。手で書き換えた値も、拡大の範囲（全体表示の下限から）と 0 以上のスクロールに収める。
    /// 有限でない値なら `None`。
    fn state(&self) -> Option<ViewState> {
        let finite =
            self.origin.is_finite() && self.px_per_sec.is_finite() && self.scroll_y.is_finite();
        finite.then(|| ViewState {
            time: TimeView {
                origin: self.origin,
                px_per_sec: self.px_per_sec.clamp(MIN_FIT_PX_PER_SEC, MAX_PX_PER_SEC),
            },
            scroll_y: self.scroll_y.max(0.0),
            heights: RowHeights {
                all: self.track_height.map(RowHeight::from),
                layers: self
                    .layer_heights
                    .iter()
                    .map(|(k, v)| (k.clone(), (*v).into()))
                    .collect(),
            },
            snapping: self.snapping,
        })
    }
}

/// 書き先のファイルと、ほかのコンポジションの分を含む中身。
struct Target {
    path: PathBuf,
    comp: String,
    file: ViewFile,
}

#[derive(Default)]
struct Inner {
    /// 今の状態。開いた直後は `.mm/view.json` から読んだもの。
    current: Option<ViewState>,
    /// `current` をまだ書いていない。
    dirty: bool,
    target: Option<Target>,
    /// Shy のフラグと、Shy の行を隠すかどうか。
    shy: Shy,
}

/// 表示の状態の置き場所。描画スレッドが置き、書くスレッドと自動試験が読む。
#[derive(Default)]
pub struct ViewSlot {
    inner: Mutex<Inner>,
    /// ファイルに書く間だけ取る。書くスレッドと閉じるときの書き残しを 1 つずつにし、`inner` は書く間に持たない
    /// （描画スレッドが毎フレーム `publish` で取るので、ファイルの遅さで描画を待たせない）。
    writing: Mutex<()>,
    /// 書くスレッドを起こす。
    wake: Mutex<Option<Sender<()>>>,
    /// Shy の状態を変えるたびに進む。
    shy_serial: AtomicU64,
}

impl ViewSlot {
    /// 今の状態を置く。変わっていなければ何もしない。
    pub fn publish(&self, state: ViewState) {
        {
            let mut inner = self.inner.lock().expect("view");
            if inner.current.as_ref() == Some(&state) {
                return;
            }
            inner.current = Some(state);
            inner.dirty = true;
        }
        self.wake();
    }

    /// 書くスレッドを起こす。
    fn wake(&self) {
        if let Some(tx) = self.wake.lock().expect("view wake").as_ref() {
            let _ = tx.send(());
        }
    }

    /// 今の状態。描画スレッドが最初の表示を当てるまでは、開いたときに読んだもの（無ければ `None`）。
    pub fn current(&self) -> Option<ViewState> {
        self.inner.lock().expect("view").current.clone()
    }

    /// スナップの入/切。表示の状態がまだ無ければ既定の入。コマンドの表のチェックの状態に出す。
    pub fn snapping(&self) -> bool {
        let inner = self.inner.lock().expect("view");
        inner.current.as_ref().is_none_or(|s| s.snapping)
    }

    /// 書いていない状態を `.mm/view.json` に書く。書けなくても表示は続ける（消してよいファイルなので）。
    pub fn flush(&self) {
        let _writing = self.writing.lock().expect("view writing");
        let Some((path, file)) = self.take_dirty() else {
            return;
        };
        if let Err(e) = write(&path, &file) {
            tracing::warn!("表示の状態を書けない（{}）: {e}", path.display());
        }
    }

    /// まだ書いていない状態を書き先の中身に入れ、書くファイルの場所と中身を返す。書くものが無ければ `None`。
    fn take_dirty(&self) -> Option<(PathBuf, ViewFile)> {
        let mut inner = self.inner.lock().expect("view");
        if !inner.dirty {
            return None;
        }
        // 表示の状態がまだ無ければ書かず、次に置かれたときに一緒に書く。
        let state = inner.current.clone()?;
        inner.dirty = false;
        let shy = inner.shy.clone();
        let target = inner.target.as_mut()?;
        target
            .file
            .timelines
            .insert(target.comp.clone(), TimelineView::of(&state, &shy));
        target.file.version = VERSION;
        Some((target.path.clone(), target.file.clone()))
    }
}

/// `project` の `.mm/view.json` から、コンポジション `comp` の表示の状態を読んで置き、書くスレッドを始める。
/// 読むだけで開いたとき（`read_only`。別のプロセスが書き手）は戻すだけで書かない（`.mm/` は書き手の置き場所なので）。
pub fn open(shared: &Arc<Shared>, project: &Path, comp: &CompositionId, read_only: bool) {
    let path = view_path(project);
    let file = read(&path);
    let current = file
        .timelines
        .get(comp.as_str())
        .and_then(TimelineView::state);
    let shy = file
        .timelines
        .get(comp.as_str())
        .map(TimelineView::shy)
        .unwrap_or_default();
    {
        let mut inner = shared.view.inner.lock().expect("view");
        inner.current = current;
        inner.shy = shy;
        shared.view.shy_serial.fetch_add(1, Ordering::SeqCst);
        inner.target = (!read_only).then(|| Target {
            path,
            comp: comp.to_string(),
            file,
        });
    }
    if read_only {
        return;
    }
    let (tx, rx) = mpsc::channel();
    *shared.view.wake.lock().expect("view wake") = Some(tx);
    let s = shared.clone();
    let spawned = std::thread::Builder::new()
        .name("view-state".into())
        .spawn(move || save_when_settled(&s, &rx));
    if let Err(e) = spawned {
        tracing::warn!("表示の状態を書くスレッドを始められない: {e}");
    }
}

/// 変化が `SETTLE` のあいだ止まるたびに 1 回書く。
fn save_when_settled(shared: &Shared, rx: &Receiver<()>) {
    while rx.recv().is_ok() {
        while rx.recv_timeout(SETTLE).is_ok() {}
        shared.view.flush();
    }
}

/// `.mm/view.json` の場所。`project.mmdb` のフォルダーか、プロジェクトの JSON のあるフォルダーの下。
fn view_path(project: &Path) -> PathBuf {
    let dir = mm_store::store_dir(project)
        .unwrap_or_else(|| project.parent().unwrap_or(Path::new(".")).to_path_buf());
    dir.join(DIR).join(FILE)
}

/// 読めない、形が違う、知らないバージョンのファイルは空として扱う（次に書くときに置き換わる）。
fn read(path: &Path) -> ViewFile {
    let Ok(text) = std::fs::read_to_string(path) else {
        return ViewFile::default();
    };
    match serde_json::from_str::<ViewFile>(&text) {
        Ok(file) if file.version == VERSION => file,
        Ok(file) => {
            tracing::warn!("表示の状態のバージョン {} は読まない", file.version);
            ViewFile::default()
        }
        Err(e) => {
            tracing::warn!("表示の状態を読めない（{}）: {e}", path.display());
            ViewFile::default()
        }
    }
}

/// 一時ファイルに書いてから名前を変え、途中で止まっても壊れたファイルを残さない。
fn write(path: &Path, file: &ViewFile) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let text = serde_json::to_string_pretty(file).map_err(std::io::Error::other)?;
    let tmp = path.with_extension(format!("json.{}.tmp", std::process::id()));
    std::fs::write(&tmp, text)?;
    std::fs::rename(&tmp, path)
}

mod shy;

pub use shy::Shy;

#[cfg(test)]
mod tests;
