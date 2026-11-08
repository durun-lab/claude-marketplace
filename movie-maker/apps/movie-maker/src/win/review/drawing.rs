//! コメントの描き込みを Program に重ねる。どのコメントの描き込みを出すかを再生位置と Review で選んだコメントから決め、
//! 描画スレッドの `Ui` に渡す。描き込みはサーフェスへの重ね描きだけで、レンダラーのフレームと書き出しには入らない。
//!
//! 出し方は Frame.io と DaVinci Resolve のレビューに倣う。時刻のコメントはそのフレームで止めている間だけ、
//! 範囲のコメントは範囲の中にいる間（再生中も）出す。選んだコメントがその位置にあれば、それだけを出す。

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use mm_serve::Core;
use mm_store::{Comment, CommentFilter, CommentState, ShapeKind};
use mm_ui_layout::{Mark, MarkKind};
use mm_ui_wgpu::Ui;

use crate::win::state::Shared;

/// フレームの長さが分からないときに使うフレームレート。
const FALLBACK_FPS: f64 = 30.0;

/// 描き込みを持つコメントと、Review で選んだコメント。コメントの通知と選択のたびに置き直す。
#[derive(Default)]
pub struct DrawingSlot {
    inner: Mutex<DrawingState>,
    /// 置き直すたびに進む。描画スレッドが渡し直すかどうかの判定に使う。
    serial: AtomicU64,
}

#[derive(Default, Clone)]
struct DrawingState {
    focused: Option<String>,
    comments: Arc<Vec<Comment>>,
}

impl DrawingSlot {
    /// Review で選んだコメント。`None` で外す。
    pub fn focus(&self, id: Option<String>) {
        self.inner.lock().expect("drawing").focused = id;
        self.serial.fetch_add(1, Ordering::SeqCst);
    }

    /// コメントの一覧を置き直す。描き込みの無いものは持たない。
    pub fn replace(&self, comments: Vec<Comment>) {
        let drawn = comments
            .into_iter()
            .filter(|c| !c.drawing.is_empty())
            .collect();
        self.inner.lock().expect("drawing").comments = Arc::new(drawn);
        self.serial.fetch_add(1, Ordering::SeqCst);
    }

    fn snapshot(&self) -> (u64, DrawingState) {
        let state = self.inner.lock().expect("drawing").clone();
        (self.serial.load(Ordering::SeqCst), state)
    }
}

/// `project.mmdb` のコメントを読み直す。
pub fn reload(shared: &Shared, core: &Core) {
    match core.comments(&CommentFilter::default()) {
        Ok(comments) => shared.review_drawing.replace(comments),
        Err(e) => tracing::warn!("描き込みを重ねるコメントを読めない: {e}"),
    }
}

/// 再生の位置。描き込みを出すかどうかの判定に使う。
#[derive(Debug, Clone, Copy)]
pub struct Playhead<'a> {
    /// 開いているコンポジションの ID。
    pub comp: &'a str,
    /// 秒。
    pub secs: f64,
    /// コンポジションのフレームレート。時刻のコメントを出すフレームの幅を決める。
    pub fps: f64,
    pub playing: bool,
}

/// コメントの描き込みをこの位置で出すか。時刻のコメントは止めている間、その時刻を含むフレームだけ。
fn shows_at(c: &Comment, at: &Playhead<'_>) -> bool {
    if c.comp != at.comp {
        return false;
    }
    if let Some(t) = c.at {
        let half = 0.5 / if at.fps > 0.0 { at.fps } else { FALLBACK_FPS };
        return !at.playing && (at.secs - t.to_f64()).abs() < half;
    }
    c.range
        .is_some_and(|r| r.start.to_f64() <= at.secs && at.secs < r.out.to_f64())
}

/// この位置で描き込みを出すコメント。選んだコメントがこの位置にあればそれだけ、無ければ人の確認がまだのもの全部。
pub fn visible<'a>(
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
pub fn marks(comments: &[&Comment]) -> Vec<Mark> {
    comments
        .iter()
        .flat_map(|c| &c.drawing)
        .map(|s| Mark {
            kind: match s.kind {
                ShapeKind::Stroke => MarkKind::Stroke,
                ShapeKind::Rect => MarkKind::Rect,
                ShapeKind::Point => MarkKind::Point,
            },
            points: s
                .points
                .iter()
                .map(|p| [p[0] as f32, p[1] as f32])
                .collect(),
        })
        .collect()
}

/// 描画スレッドが持つ、`Ui` に渡した描き込みの控え。出すコメントが変わったときだけ渡し直す。
#[derive(Default)]
pub struct DrawingSync {
    serial: u64,
    shown: Vec<String>,
}

impl DrawingSync {
    /// 今の再生位置で出す描き込みを `ui` に渡す。
    pub fn refresh(&mut self, shared: &Shared, ui: &mut Ui) {
        let (serial, state) = shared.review_drawing.snapshot();
        let ids_and_marks = shared.doc().and_then(|doc| {
            let fps = doc.project.composition(&doc.comp).ok()?.fps.to_f64();
            let at = Playhead {
                comp: doc.comp.as_str(),
                secs: shared.playhead(),
                fps,
                playing: shared.playing.load(Ordering::SeqCst),
            };
            let shown = visible(&state.comments, state.focused.as_deref(), &at);
            let ids: Vec<String> = shown.iter().map(|c| c.id.clone()).collect();
            Some((ids, marks(&shown)))
        });
        let (ids, marks) = ids_and_marks.unwrap_or_default();
        if serial == self.serial && ids == self.shown {
            return;
        }
        self.serial = serial;
        self.shown = ids;
        ui.set_drawing(marks);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mm_store::{Actor, CommentRange, Shape};
    use mm_types::RationalTime;

    fn comment(id: &str, at: Option<f64>, range: Option<(f64, f64)>) -> Comment {
        let time = |s: f64| RationalTime::new((s * 1000.0).round() as i128, 1000).expect("時刻");
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
            fps: 25.0,
            playing,
        }
    }

    fn ids(v: &[&Comment]) -> Vec<String> {
        v.iter().map(|c| c.id.clone()).collect()
    }

    #[test]
    fn 時刻のコメントはそのフレームで止めている間だけ出す() {
        let list = [comment("a", Some(2.0), None)];
        assert_eq!(ids(&visible(&list, None, &at(2.0, false))), ["a"]);
        assert_eq!(
            ids(&visible(&list, None, &at(2.019, false))),
            ["a"],
            "同じフレームの中"
        );
        assert!(
            visible(&list, None, &at(2.04, false)).is_empty(),
            "次のフレームで消える"
        );
        assert!(
            visible(&list, None, &at(1.96, false)).is_empty(),
            "前のフレームも出さない"
        );
        assert!(
            visible(&list, None, &at(2.0, true)).is_empty(),
            "再生中は出さない"
        );
    }

    #[test]
    fn 範囲のコメントは範囲の中で再生中も出し_終わりで消える() {
        let list = [comment("r", None, Some((1.0, 3.0)))];
        assert_eq!(ids(&visible(&list, None, &at(1.0, true))), ["r"]);
        assert_eq!(ids(&visible(&list, None, &at(2.5, false))), ["r"]);
        assert!(visible(&list, None, &at(3.0, false)).is_empty());
        assert!(visible(&list, None, &at(0.9, false)).is_empty());
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
        let m = marks(&[&c]);
        assert_eq!(m.len(), 1);
        assert_eq!(m[0].kind, MarkKind::Rect);
        assert_eq!(m[0].points, [[0.1, 0.2], [0.3, 0.4]]);
        let slot = DrawingSlot::default();
        let mut plain = comment("p", Some(1.0), None);
        plain.drawing.clear();
        slot.replace(vec![c, plain]);
        assert_eq!(
            ids(&slot.snapshot().1.comments.iter().collect::<Vec<_>>()),
            ["a"]
        );
    }
}
