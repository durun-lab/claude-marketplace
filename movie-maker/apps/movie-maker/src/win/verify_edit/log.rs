//! Timeline 編集の通しの結果の記録と、手順ごとのスクリーンショット。

use serde_json::{json, Value};

use super::{Env, SETTLE};
use crate::win::capture;
use crate::win::verify_stage::checkpoint;

/// 結果の記録。段ごとの合否と、撮ったスクリーンショット。
#[derive(Default)]
pub(crate) struct Log {
    steps: Vec<(String, Value)>,
    shots: Vec<String>,
}

impl Log {
    pub(crate) fn step(&mut self, name: &str, ok: bool, detail: Value) -> bool {
        self.steps
            .push((name.to_owned(), json!({ "ok": ok, "detail": detail })));
        ok
    }

    pub(crate) fn into_json(self) -> Value {
        let all_ok = self.steps.iter().all(|(_, v)| v["ok"] == true);
        let steps: serde_json::Map<String, Value> = self.steps.into_iter().collect();
        json!({ "all_ok": all_ok, "steps": steps, "screenshots": self.shots })
    }
}

/// スクリーンショットを撮る。返るまで長くかかったら、時間切れの工程はここで止まり、後の入力を送らない。
pub(crate) fn shot(env: &Env<'_>, log: &mut Log, name: &str) {
    let Some(base) = &env.test.screenshot else {
        return;
    };
    std::thread::sleep(SETTLE);
    let path = base.with_extension(format!("edit-{name}.png"));
    match capture::capture_window(env.window_hwnd, &path) {
        Ok(()) => log.shots.push(path.display().to_string()),
        Err(e) => log.shots.push(format!("失敗: {e}")),
    }
    checkpoint();
}
