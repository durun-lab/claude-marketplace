//! コメントの描き込みを Program に重ねる。どのコメントの描き込みを出すかを再生位置と Review で選んだコメントから決め、
//! 描画スレッドの `Ui` に渡す。描き込みはサーフェスへの重ね描きだけで、レンダラーのフレームと書き出しには入らない。
//!
//! 出し方は Frame.io と DaVinci Resolve のレビューに倣う。時刻のコメントはそのフレームで止めている間だけ、
//! 範囲のコメントは範囲に掛かるフレームにいる間（再生中も）出す。選んだコメントがその位置にあれば、それだけを出す。

use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};

use mm_serve::Core;
use mm_store::{Comment, CommentFilter, CommentState, ShapeKind};
use mm_types::{Rational, RationalTime};
use mm_ui_layout::{DrawingShape, DrawingShapeKind};
use mm_ui_wgpu::Ui;

use crate::win::state::Shared;
use crate::win::{surface, timebase};

/// 描き込みを持つコメントと、Review で選んだコメント。コメントの通知と選択のたびに置き直す。
#[derive(Default)]
pub struct DrawingSlot {
    inner: Mutex<DrawingState>,
}

#[derive(Default, Clone)]
struct DrawingState {
    /// 置き直すたびに進む。描画スレッドが渡し直すかどうかの判定に使う。
    serial: u64,
    focused: Option<String>,
    comments: Arc<Vec<Comment>>,
}

impl DrawingSlot {
    /// Review で選んだコメント。`None` で外す。
    pub fn focus(&self, id: Option<String>) {
        let mut s = self.inner.lock().expect("drawing");
        s.focused = id;
        s.serial += 1;
    }

    /// コメントの一覧を置き直す。描き込みの無いものは持たない。
    pub fn replace(&self, comments: Vec<Comment>) {
        let drawn = comments
            .into_iter()
            .filter(|c| !c.drawing.is_empty())
            .collect();
        let mut s = self.inner.lock().expect("drawing");
        s.comments = Arc::new(drawn);
        s.serial += 1;
    }

    fn snapshot(&self) -> DrawingState {
        self.inner.lock().expect("drawing").clone()
    }
}

/// `project.mmdb` のコメントを読み直す。読めなければ前の一覧のまま出し続ける。
pub fn reload(shared: &Shared, core: &Core) {
    match core.comments(&CommentFilter::default()) {
        Ok(comments) => shared.review_drawing.replace(comments),
        Err(e) => tracing::warn!("描き込みを重ねるコメントを読めない: {e}"),
    }
}

/// 再生位置。描き込みを出すかどうかの判定に使う。
#[derive(Debug, Clone, Copy)]
struct Playhead<'a> {
    /// 開いているコンポジションの ID。
    comp: &'a str,
    /// 秒。
    secs: f64,
    /// コンポジションのフレームレート。Program に映るフレームを決める。
    fps: Rational,
    playing: bool,
}

/// コメントの描き込みをこの位置で出すか。Program に映るフレーム（`timebase::frame_index`）で比べる。
/// 時刻のコメントは止めている間、その時刻を含むフレームだけ。範囲のコメントは、映るフレームが範囲に掛かる間。
fn shows_at(c: &Comment, at: &Playhead<'_>) -> bool {
    if c.comp != at.comp {
        return false;
    }
    let frame = timebase::frame_index(at.secs, at.fps.to_f64());
    if let Some(t) = c.at {
        return !at.playing && t.frame_index(at.fps) == frame;
    }
    c.range.is_some_and(|r| {
        let starts = RationalTime::from_frames(frame, at.fps);
        r.start.frame_index(at.fps) <= frame && starts.is_ok_and(|s| s < r.out)
    })
}

/// この位置で描き込みを出すコメント。選んだコメントがこの位置にあればそれだけ、無ければ人の確認がまだのもの全部。
fn visible<'a>(
    comments: &'a [Comment],
    focused: Option<&str>,
    at: &Playhead<'_>,
) -> Vec<&'a Comment> {
    let here: Vec<&Comment> = comments.iter().filter(|c| shows_at(c, at)).collect();
    if let Some(c) = here.iter().find(|c| Some(c.id.as_str()) == focused) {
        return vec![*c];
    }
    here.into_iter()
        .filter(|c| matches!(c.state, CommentState::Open | CommentState::Fresh))
        .collect()
}

/// コメントの描き込みを `Ui` に渡す形にする。
fn shapes(comments: &[&Comment]) -> Vec<DrawingShape> {
    comments
        .iter()
        .flat_map(|c| &c.drawing)
        .map(|s| DrawingShape {
            kind: match s.kind {
                ShapeKind::Stroke => DrawingShapeKind::Stroke,
                ShapeKind::Rect => DrawingShapeKind::Rect,
                ShapeKind::Point => DrawingShapeKind::Point,
            },
            points: s
                .points
                .iter()
                .map(|p| [p[0] as f32, p[1] as f32])
                .collect(),
        })
        .collect()
}

/// 描画スレッドの側で、最後に `Ui` に渡したコメントを持つ。出すコメントが変わったときだけ渡し直す。
#[derive(Default)]
pub struct DrawingSync {
    serial: u64,
    shown: Vec<String>,
}

impl DrawingSync {
    /// 今の再生位置で出す描き込みと、`hwnd` の表示の倍率を `ui` に渡す。
    pub fn refresh(&mut self, shared: &Shared, hwnd: isize, ui: &mut Ui) {
        ui.set_scale_factor(surface::scale_factor(hwnd));
        let state = shared.review_drawing.snapshot();
        let doc = shared.doc();
        let at = doc.as_ref().and_then(|doc| {
            Some(Playhead {
                comp: doc.comp.as_str(),
                secs: shared.playhead(),
                fps: doc.project.composition(&doc.comp).ok()?.fps,
                playing: shared.playing.load(Ordering::SeqCst),
            })
        });
        if let Some(shapes) = self.update(&state, at.as_ref()) {
            ui.set_drawing(shapes);
        }
    }

    /// 出すコメントが前と変わったときだけ、渡す描き込みを返す。描画のループは毎周呼ぶので、変わらない間は作らない。
    fn update(
        &mut self,
        state: &DrawingState,
        at: Option<&Playhead<'_>>,
    ) -> Option<Vec<DrawingShape>> {
        let shown = at
            .map(|at| visible(&state.comments, state.focused.as_deref(), at))
            .unwrap_or_default();
        let same_ids = shown.iter().map(|c| &c.id).eq(self.shown.iter());
        if state.serial == self.serial && same_ids {
            return None;
        }
        self.serial = state.serial;
        self.shown = shown.iter().map(|c| c.id.clone()).collect();
        Some(shapes(&shown))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mm_store::{Actor, CommentRange, Shape};

    const FPS: u32 = 25;

    fn time(s: f64) -> RationalTime {
        RationalTime::new((s * 1000.0).round() as i128, 1000).expect("時刻")
    }

    fn comment(id: &str, at: Option<f64>, range: Option<(f64, f64)>) -> Comment {
        Comment {
            id: id.into(),
            comp: "main".into(),
            at: at.map(time),
            range: range.map(|(a, b)| CommentRange {
                start: time(a),
                out: time(b),
            }),
            text: String::new(),
            drawing: vec![Shape {
                kind: ShapeKind::Rect,
                points: vec![[0.1, 0.2], [0.3, 0.4]],
            }],
            author: Actor {
                kind: "mobile".into(),
                name: "Pixel".into(),
            },
            client_id: None,
            replies: Vec::new(),
            state: CommentState::Open,
            created_ms: 0,
            responded_txn: None,
        }
    }

    fn at(secs: f64, playing: bool) -> Playhead<'static> {
        Playhead {
            comp: "main",
            secs,
            fps: Rational::new(FPS, 1).expect("fps"),
            playing,
        }
    }

    /// Program にフレーム `n` が映る再生位置（フレームの 4 分の 1 の所）。
    fn frame(n: u32) -> f64 {
        (f64::from(n) + 0.25) / f64::from(FPS)
    }

    fn ids(v: &[&Comment]) -> Vec<String> {
        v.iter().map(|c| c.id.clone()).collect()
    }

    #[test]
    fn 時刻のコメントはそのフレームで止めている間だけ出す() {
        let list = [comment("a", Some(2.0), None)];
        assert_eq!(ids(&visible(&list, None, &at(2.0, false))), ["a"]);
        assert_eq!(
            ids(&visible(&list, None, &at(2.039, false))),
            ["a"],
            "同じフレームの中"
        );
        assert!(
            visible(&list, None, &at(frame(51), false)).is_empty(),
            "次のフレームで消える"
        );
        assert!(
            visible(&list, None, &at(frame(49), false)).is_empty(),
            "前のフレームも出さない"
        );
        assert!(
            visible(&list, None, &at(2.0, true)).is_empty(),
            "再生中は出さない"
        );
    }

    #[test]
    fn 時刻のコメントはフレームの途中の時刻でも_program_に映るフレームで出す() {
        // 2.03 秒はフレーム 50（2.00〜2.04 秒）の中。フレーム 50 で止めたら出し、51 では出さない。
        let list = [comment("a", Some(2.03), None)];
        assert_eq!(ids(&visible(&list, None, &at(frame(50), false))), ["a"]);
        assert!(visible(&list, None, &at(frame(51), false)).is_empty());
        // ちょうどフレームの半ば（2.02 秒）でも、映るフレーム 50 で出す。
        let half = [comment("h", Some(2.02), None)];
        assert_eq!(ids(&visible(&half, None, &at(frame(50), false))), ["h"]);
        assert!(visible(&half, None, &at(frame(51), false)).is_empty());
    }

    #[test]
    fn 範囲のコメントは範囲に掛かるフレームで再生中も出し_終わりで消える() {
        let list = [comment("r", None, Some((1.0, 3.0)))];
        assert_eq!(ids(&visible(&list, None, &at(1.0, true))), ["r"]);
        assert_eq!(ids(&visible(&list, None, &at(2.5, false))), ["r"]);
        assert!(
            visible(&list, None, &at(3.0, false)).is_empty(),
            "終わりは含まない"
        );
        assert!(visible(&list, None, &at(frame(24), false)).is_empty());
        // 始めと終わりがフレームの途中でも、そのフレームに掛かっていれば出す。
        let mid = [comment("m", None, Some((1.03, 2.01)))];
        assert_eq!(ids(&visible(&mid, None, &at(frame(25), false))), ["m"]);
        assert_eq!(ids(&visible(&mid, None, &at(frame(50), false))), ["m"]);
        assert!(visible(&mid, None, &at(frame(51), false)).is_empty());
        // 1 フレームより短い範囲も、掛かるフレームでは出す。
        let short = [comment("s", None, Some((2.005, 2.015)))];
        assert_eq!(ids(&visible(&short, None, &at(frame(50), true))), ["s"]);
    }

    #[test]
    fn 選んだコメントがその位置にあればそれだけを出す() {
        let list = [
            comment("a", Some(2.0), None),
            comment("r", None, Some((1.0, 3.0))),
        ];
        assert_eq!(ids(&visible(&list, None, &at(2.0, false))), ["a", "r"]);
        assert_eq!(ids(&visible(&list, Some("r"), &at(2.0, false))), ["r"]);
        // 選んだコメントの外へ動いたら、その位置のものに戻す。
        let list = [
            comment("b", Some(9.0), None),
            comment("r", None, Some((1.0, 3.0))),
        ];
        assert_eq!(ids(&visible(&list, Some("b"), &at(2.0, false))), ["r"]);
    }

    #[test]
    fn 済みのコメントは選んだときだけ_ほかのコンポジションは出さない() {
        let mut done = comment("d", Some(2.0), None);
        done.state = CommentState::Done;
        let mut other = comment("o", Some(2.0), None);
        other.comp = "short".into();
        let list = [done, other];
        assert!(visible(&list, None, &at(2.0, false)).is_empty());
        assert_eq!(ids(&visible(&list, Some("d"), &at(2.0, false))), ["d"]);
    }

    #[test]
    fn 描き込みの形と座標をそのまま渡し_描き込みの無いコメントは持たない() {
        let c = comment("a", Some(2.0), None);
        let m = shapes(&[&c]);
        assert_eq!(m.len(), 1);
        assert_eq!(m[0].kind, DrawingShapeKind::Rect);
        assert_eq!(m[0].points, [[0.1, 0.2], [0.3, 0.4]]);
        let slot = DrawingSlot::default();
        let mut plain = comment("p", Some(1.0), None);
        plain.drawing.clear();
        slot.replace(vec![c, plain]);
        assert_eq!(
            ids(&slot.snapshot().comments.iter().collect::<Vec<_>>()),
            ["a"]
        );
    }

    #[test]
    fn 出すコメントか一覧が変わったときだけ渡し直し_外へ出たら空を渡す() {
        let slot = DrawingSlot::default();
        slot.replace(vec![comment("a", Some(2.0), None)]);
        let mut sync = DrawingSync::default();
        let here = at(2.0, false);
        let first = sync.update(&slot.snapshot(), Some(&here));
        assert_eq!(first.map(|s| s.len()), Some(1), "初めは渡す");
        assert!(
            sync.update(&slot.snapshot(), Some(&here)).is_none(),
            "同じなら渡さない"
        );
        let away = at(5.0, false);
        let cleared = sync.update(&slot.snapshot(), Some(&away));
        assert_eq!(cleared, Some(Vec::new()), "外へ出たら空で消す");
        let mut done = comment("a", Some(2.0), None);
        done.state = CommentState::Done;
        slot.replace(vec![done]);
        let after = sync.update(&slot.snapshot(), Some(&here));
        assert_eq!(after, Some(Vec::new()), "済みにしたら消す");
        assert_eq!(
            sync.update(&slot.snapshot(), None),
            None,
            "開いていないときも空のまま"
        );
    }
}
