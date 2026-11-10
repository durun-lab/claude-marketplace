//! ユーザーの割り付け。仕切りの位置（WebView の列の幅、Timeline の高さの比）と、WebView の列の開け閉め。
//! プロジェクトのデータではなく、ユーザーの設定として `%LOCALAPPDATA%/movie-maker/workspace.json` に置く（Resolve と
//! Premiere のワークスペースも、プロジェクトではなくアプリの設定）。幅は論理ピクセル、高さは割合で持つので、表示の倍率や
//! ウィンドウの大きさが変わっても割り付けは崩れない。ドラッグを離したときと、列の開け閉めと Reset UI Layout のときに書く。

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

use mm_ui_layout::{timeline_ratio, SPLITTER_W, TIMELINE_RATIO};
use serde::{Deserialize, Serialize};

use crate::win::theme;

/// 左の列の幅の下限（論理ピクセル）。Tasks のタブが 3 つまで 1 行に入り、Media の一覧が読める幅。
pub const LEFT_MIN: f64 = 200.0;
/// 右の列の幅の下限（論理ピクセル）。Inspector のパラメータの欄が読める幅。上限は theme.json の `right-column`。
pub const RIGHT_MIN: f64 = 300.0;
/// サーフェスの幅の下限（論理ピクセル）。Program の再生の操作のバーのタイムコードとボタンが 1 行に入る幅。
pub const MIN_SURFACE_W: f64 = 400.0;
/// 設定のファイルの形式の版。
const VERSION: u32 = 1;
/// 設定のファイルの名前。
const FILE: &str = "workspace.json";
/// 設定のファイルの場所を替える環境変数。試験とスクリーンショットで、本物の設定に触れないために使う。
pub const FILE_ENV: &str = "MM_WORKSPACE_FILE";

/// 割り付け。既定は `Default`（View > Reset UI Layout で戻る形）。
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Workspace {
    /// 2 列とも開いているときの WebView の幅。左の列は残りで、右の列は上限の幅から、左の列が下限を割るときだけ縮む。
    pub both_w: f64,
    /// 左の列だけ開いているときの幅。
    pub left_w: f64,
    /// 右の列だけ開いているときの幅。
    pub right_w: f64,
    pub left_open: bool,
    pub right_open: bool,
    /// Timeline の高さの、サーフェスの高さに対する割合。
    pub timeline_ratio: f32,
}

impl Default for Workspace {
    fn default() -> Self {
        let (left, right) = theme::columns();
        Self {
            both_w: left + right,
            left_w: left,
            right_w: right,
            left_open: true,
            right_open: true,
            timeline_ratio: TIMELINE_RATIO,
        }
    }
}

/// 列の幅（論理ピクセル）。閉じている列は 0。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Columns {
    pub left: f64,
    pub right: f64,
}

impl Columns {
    /// WebView の幅。
    pub fn web(&self) -> f64 {
        self.left + self.right
    }
}

/// `v` を `lo` から `hi` に丸める。`hi` が `lo` を下回る狭いウィンドウでは `lo`。
fn clamp_to(v: f64, lo: f64, hi: f64) -> f64 {
    v.min(hi.max(lo)).max(lo)
}

impl Workspace {
    /// WebView の幅の上限。サーフェスの下限と縦の仕切りを残す。
    fn web_cap(window_w: f64) -> f64 {
        window_w - MIN_SURFACE_W - f64::from(SPLITTER_W)
    }

    /// 右の列の、2 列とも開いているときの幅。WebView の幅 `web` から左の列の下限を引いた残りで、上限は theme.json の
    /// `right-column`。左の列が下限より広いうちは右の列は動かない。
    fn right_in(web: f64) -> f64 {
        clamp_to(web - LEFT_MIN, RIGHT_MIN, theme::columns().1)
    }

    /// ウィンドウの幅 `window_w`（論理ピクセル）での列の幅。ウィンドウが狭いときはサーフェスの下限を守って縮める。
    pub fn columns(&self, window_w: f64) -> Columns {
        let cap = Self::web_cap(window_w);
        let (right_max, both_min) = (theme::columns().1, LEFT_MIN + RIGHT_MIN);
        match (self.left_open, self.right_open) {
            (true, true) => {
                let web = clamp_to(self.both_w, both_min, cap);
                let right = Self::right_in(web);
                Columns {
                    left: web - right,
                    right,
                }
            }
            (true, false) => Columns {
                left: clamp_to(self.left_w, LEFT_MIN, cap),
                right: 0.0,
            },
            (false, true) => Columns {
                left: 0.0,
                right: clamp_to(self.right_w, RIGHT_MIN, right_max.min(cap)),
            },
            (false, false) => Columns {
                left: 0.0,
                right: 0.0,
            },
        }
    }

    /// 縦の仕切りのドラッグ。WebView の幅を `web`（論理ピクセル）にする。開いている列の組ごとの幅を動かす。
    pub fn drag_web(&mut self, web: f64, window_w: f64) {
        let cap = Self::web_cap(window_w);
        match (self.left_open, self.right_open) {
            (true, true) => self.both_w = clamp_to(web, LEFT_MIN + RIGHT_MIN, cap),
            (true, false) => self.left_w = clamp_to(web, LEFT_MIN, cap),
            (false, true) => self.right_w = clamp_to(web, RIGHT_MIN, theme::columns().1.min(cap)),
            (false, false) => {}
        }
    }

    /// 横の仕切りのドラッグ。Timeline の高さを `th`（サーフェスの高さ `h` と同じ論理ピクセル）にする。
    pub fn drag_timeline(&mut self, th: f32, h: f32) {
        self.timeline_ratio = timeline_ratio(h, th);
    }

    /// 縦の仕切りを出すか（WebView が 1 列でも開いているか）。
    pub fn has_web(&self) -> bool {
        self.left_open || self.right_open
    }

    /// 読んだ値の足りないところ（壊れた数、負の数）を既定で埋める。
    fn sanitized(mut self) -> Self {
        let d = Self::default();
        let ok = |v: f64| v.is_finite() && v > 0.0;
        for (v, default) in [
            (&mut self.both_w, d.both_w),
            (&mut self.left_w, d.left_w),
            (&mut self.right_w, d.right_w),
        ] {
            if !ok(*v) {
                *v = default;
            }
        }
        if !(self.timeline_ratio.is_finite() && self.timeline_ratio > 0.0) {
            self.timeline_ratio = d.timeline_ratio;
        }
        self
    }
}

/// ファイルの中身。版を先頭に置く。
#[derive(Serialize, Deserialize)]
struct Stored {
    version: u32,
    #[serde(flatten)]
    workspace: Workspace,
}

/// 設定のファイルの場所。`MM_WORKSPACE_FILE`、なければ `%LOCALAPPDATA%/movie-maker/workspace.json`。どちらも無ければ `None`。
pub fn default_path() -> Option<PathBuf> {
    path_from(
        std::env::var_os(FILE_ENV).map(PathBuf::from),
        std::env::var_os("LOCALAPPDATA").map(PathBuf::from),
    )
}

fn path_from(file: Option<PathBuf>, local: Option<PathBuf>) -> Option<PathBuf> {
    file.or_else(|| local.map(|d| d.join("movie-maker").join(FILE)))
}

/// `path` の設定を読む。無い、壊れている、知らない版なら `None`（既定で始める）。
pub fn read(path: &Path) -> Option<Workspace> {
    let text = std::fs::read_to_string(path).ok()?;
    match serde_json::from_str::<Stored>(&text) {
        Ok(s) if s.version == VERSION => Some(s.workspace.sanitized()),
        Ok(s) => {
            tracing::warn!("割り付けの設定のバージョン {} は読まない", s.version);
            None
        }
        Err(e) => {
            tracing::warn!("割り付けの設定を読めない（{}）: {e}", path.display());
            None
        }
    }
}

/// 一時ファイルに書いてから名前を変え、途中で止まっても壊れたファイルを残さない。
pub fn write(path: &Path, workspace: &Workspace) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let stored = Stored {
        version: VERSION,
        workspace: *workspace,
    };
    let text = serde_json::to_string_pretty(&stored).map_err(std::io::Error::other)?;
    let tmp = path.with_extension(format!("json.{}.tmp", std::process::id()));
    std::fs::write(&tmp, text)?;
    std::fs::rename(&tmp, path)
}

/// 今の割り付けの置き場所。UI のスレッドが書き換え、描画スレッドが毎フレーム読む。
#[derive(Default)]
pub struct WorkspaceSlot {
    inner: Mutex<Workspace>,
    /// 値が変わるたびに進む。読む側の更新の判定に使う。
    serial: AtomicU64,
    /// 設定のファイル。`open` で決める。決めなければ（`--play-test`）読みも書きもしない。
    path: OnceLock<PathBuf>,
}

impl WorkspaceSlot {
    pub fn get(&self) -> Workspace {
        *self.inner.lock().expect("workspace")
    }

    pub fn serial(&self) -> u64 {
        self.serial.load(Ordering::SeqCst)
    }

    /// 値を置く。変わらなければ何もしない。
    pub fn set(&self, next: Workspace) {
        let mut inner = self.inner.lock().expect("workspace");
        if *inner != next {
            *inner = next;
            self.serial.fetch_add(1, Ordering::SeqCst);
        }
    }

    /// 今の値から作った値を置く。
    pub fn update(&self, f: impl FnOnce(&mut Workspace)) {
        let mut next = self.get();
        f(&mut next);
        self.set(next);
    }

    /// 設定のファイルを決め、あれば読んで置く。
    pub fn open(&self, path: PathBuf) {
        if let Some(saved) = read(&path) {
            self.set(saved);
        }
        let _ = self.path.set(path);
    }

    /// 今の値を設定のファイルに書く。書けなくても割り付けは続ける（消してよい設定なので）。
    pub fn save(&self) {
        let Some(path) = self.path.get() else {
            return;
        };
        if let Err(e) = write(path, &self.get()) {
            tracing::warn!("割り付けの設定を書けない（{}）: {e}", path.display());
        }
    }
}

#[cfg(test)]
mod tests;
