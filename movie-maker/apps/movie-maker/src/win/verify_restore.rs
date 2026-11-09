//! `--play-test` が適用した編集を、テストの前の取り消しのスタックの位置（セーブポイント）まで戻す。
//! 位置より前の、テストの前からある履歴（ユーザーの操作）は取り消さない。データベースの `SAVEPOINT` と
//! `ROLLBACK TO` と同じ考えで、止める所は取り消しのスタックの位置で決め、プロジェクトのデータは結果として比べるだけにする。

use std::sync::Arc;
use std::time::{Duration, Instant};

use mm_doc::Project;
use mm_serve::Core;
use serde_json::{json, Value};

use crate::win::edit;
use crate::win::state::Shared;
use crate::win::verify_stage::wait_until;

/// 保存のバージョンがこの長さ変わらなければ、送った入力の編集が届き終わったとみなす。
const QUIET: Duration = Duration::from_millis(500);
/// 編集が届き終わるのを待つ上限。
const QUIET_LIMIT: Duration = Duration::from_secs(5);
/// 位置まで戻した後に、遅れて届いた編集をもう一度戻す回数の上限（最初の 1 回を含む）。
const ROUNDS: usize = 3;
/// 取り消しとやり直しを送る数の上限。ほかの発行元が編集し続けても止まる。
const MAX_STEPS: usize = 64;

/// テストの前の取り消しのスタックの位置と、プロジェクトのデータ。
pub struct Savepoint {
    core: Arc<Core>,
    depth: i64,
    project: Arc<Project>,
}

impl Savepoint {
    /// 今の位置とデータを記録する。`project.mmdb` を書き手として開いていなければ、編集は保存されないので `None`。
    pub fn take(shared: &Shared) -> Option<Self> {
        let core = shared.core.get()?.clone();
        let depth = undo_depth(&core)?;
        let project = shared.doc()?.project;
        Some(Self {
            core,
            depth,
            project,
        })
    }

    /// 取り消しのスタックが記録した位置に戻るまで取り消し、テストが取り消しすぎていればやり直す。
    fn restore(&self, shared: &Shared) -> Value {
        let mut steps = Steps::default();
        for _ in 0..ROUNDS {
            wait_quiet(&self.core);
            if !steps.walk(shared, &self.core, self.depth) {
                break;
            }
        }
        let depth = undo_depth(&self.core);
        let same = shared.doc().is_some_and(|d| *d.project == *self.project);
        json!({
            "restored": depth == Some(self.depth) && same,
            "undos": steps.undos,
            "redos": steps.redos,
            "undo_depth": { "before": self.depth, "after": depth },
        })
    }
}

/// テストの編集を戻して、結果の JSON の `restore` の値を返す。
pub fn restore(savepoint: Option<&Savepoint>, shared: &Shared) -> Value {
    match savepoint {
        Some(s) => s.restore(shared),
        None => {
            json!({ "skipped": "編集を保存しないプロジェクト（JSON か、ほかのプロセスが書き手）" })
        }
    }
}

/// 送った取り消しとやり直しの数。
#[derive(Default)]
struct Steps {
    undos: usize,
    redos: usize,
}

impl Steps {
    /// 取り消しのスタックが位置 `target` に来るまで 1 つずつ取り消し・やり直す。1 つでも動かしたら真。
    fn walk(&mut self, shared: &Shared, core: &Core, target: i64) -> bool {
        let mut moved = false;
        while let Some(depth) = undo_depth(core).filter(|d| *d != target) {
            if self.undos + self.redos >= MAX_STEPS {
                break;
            }
            let undo = depth > target;
            edit::step_saved(shared, core, undo);
            if undo_depth(core) == Some(depth) {
                break;
            }
            if undo {
                self.undos += 1;
            } else {
                self.redos += 1;
            }
            moved = true;
        }
        moved
    }
}

fn undo_depth(core: &Core) -> Option<i64> {
    core.project().ok().map(|view| view.undo_depth)
}

/// 送った入力の編集が届き終わるまで待つ。時間切れの工程が最後に送った入力の編集も、ここで届いてから戻す。
fn wait_quiet(core: &Core) {
    let mut last = (core.version().ok(), Instant::now());
    wait_until(QUIET_LIMIT, || {
        let version = core.version().ok();
        if version != last.0 {
            last = (version, Instant::now());
        }
        last.1.elapsed() >= QUIET
    });
}

#[cfg(test)]
mod tests {
    use mm_doc::Topic;
    use mm_store::Store;
    use mm_types::{Id, RationalTime};

    use super::*;
    use crate::win::sequence_edit::tests::sample_project;

    /// `project.mmdb` を書き手として開いたアプリと同じ共有状態。再生スレッドは無い。
    fn open(dir: &std::path::Path) -> Arc<Shared> {
        let store = Store::create(dir, &sample_project(), 0).expect("project.mmdb を作る");
        let core = Core::new(store, None);
        let shared = Arc::new(Shared::new());
        let _ = shared.core.set(core.clone());
        let (version, project) = core.render_project().expect("データを読む");
        shared.publish_saved(version, Arc::new(project), Id::new("main"));
        shared
    }

    /// 話題を 1 つ足す編集を適用する。
    fn add_topic(shared: &Shared, id: &str) {
        let topic = Topic {
            id: Id::new(id),
            asset: Id::new("a"),
            in_point: RationalTime::seconds(1),
            out_point: RationalTime::seconds(2),
            title: id.into(),
            rating: Default::default(),
            tags: Vec::new(),
        };
        edit::submit(shared, vec![mm_ops::AddTopic { topic }.into()]).expect("話題を足す");
    }

    fn has_topic(shared: &Shared, id: &str) -> bool {
        shared
            .doc()
            .is_some_and(|d| d.project.topic(&Id::new(id)).is_ok())
    }

    fn core(shared: &Shared) -> &Core {
        shared.core.get().expect("書き手")
    }

    #[test]
    fn テストの編集だけを取り消し_前からの履歴は残す() {
        let dir = tempfile::tempdir().unwrap();
        let shared = open(dir.path());
        add_topic(&shared, "user");
        let savepoint = Savepoint::take(&shared);
        add_topic(&shared, "t1");
        add_topic(&shared, "t2");
        let r = restore(savepoint.as_ref(), &shared);
        assert_eq!(
            (r["restored"].clone(), r["undos"].clone()),
            (json!(true), json!(2))
        );
        assert!(has_topic(&shared, "user") && !has_topic(&shared, "t1"));
    }

    #[test]
    fn テストが前からの履歴まで取り消したらやり直す() {
        let dir = tempfile::tempdir().unwrap();
        let shared = open(dir.path());
        add_topic(&shared, "user");
        let savepoint = Savepoint::take(&shared);
        edit::step_saved(&shared, core(&shared), true);
        let r = restore(savepoint.as_ref(), &shared);
        assert_eq!(
            (r["restored"].clone(), r["redos"].clone()),
            (json!(true), json!(1))
        );
        assert!(has_topic(&shared, "user"));
    }

    #[test]
    fn 位置が同じでデータが違うときは_それより前を取り消さずに失敗を返す() {
        let dir = tempfile::tempdir().unwrap();
        let shared = open(dir.path());
        add_topic(&shared, "user1");
        add_topic(&shared, "user2");
        let savepoint = Savepoint::take(&shared);
        edit::step_saved(&shared, core(&shared), true);
        add_topic(&shared, "t1");
        let r = restore(savepoint.as_ref(), &shared);
        assert_eq!(
            (r["restored"].clone(), r["undos"].clone()),
            (json!(false), json!(0))
        );
        assert!(
            has_topic(&shared, "user1"),
            "位置より前の履歴は取り消さない"
        );
    }

    #[test]
    fn 保存しないプロジェクトは戻さない() {
        let shared = Shared::new();
        assert!(Savepoint::take(&shared).is_none());
        assert!(restore(None, &shared)["skipped"].is_string());
    }
}
