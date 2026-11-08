//! 提案の区間の短い動画の配信。URL にキーを入れ、範囲の中が編集されたら別の URL にする（HLS の区間と同じ）。

use std::path::Path;

use mm_doc::Project;
use mm_types::{MmError, Result};

use super::{Streamer, CLIP_CACHE_LIMIT_BYTES};
use crate::cache::touch;
use crate::clip::{render_clip, ClipPlan, ClipRequest, ClipStart, CLIP_TIMEOUT};
use crate::key::{clip_key, ClipKey, KeyEnv};
use crate::payload::{ranged_file, Payload, IMMUTABLE};
use crate::worker::Urgent;

impl Streamer {
    /// 提案の区間の短い動画のキー（提案の操作、範囲、範囲に掛かるもののハッシュ）。`project` は提案を適用したもの。
    /// 端末は URL に入れて読むので、範囲の中が編集されると URL が変わる。
    pub fn clip_key(&self, project: &Project, version: u64, req: &ClipRequest) -> Result<ClipKey> {
        Ok(self.clip_plan(project, version, req)?.0)
    }

    /// 作り終えた短い動画をキーで返す。ディスクに無ければ `None`。
    /// 範囲の中が編集された後も、再生の途中の `Range` の続きは前のキーの同じバイトで答える。
    pub fn cached_clip(&self, key: ClipKey, range: Option<&str>) -> Result<Option<Payload>> {
        let path = self.shared.cache.clip(key);
        match clip_file(&path, key, range) {
            Err(MmError::Io(e)) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            other => other.map(|p| {
                touch(&path);
                Some(p)
            }),
        }
    }

    /// 提案の区間の短い動画（MP4）。`project` は提案を適用したもの。無ければ作り終えるまで待ち、`Range` に答える。
    /// 今の中身のキーが `key` と違えば `NotFound`（端末は提案を読み直して新しい URL を得る）。
    /// 同時に作る短い動画が上限なら `Unsupported`、待つ上限を超えたら `Timeout`。
    pub fn clip(
        &self,
        project: &Project,
        version: u64,
        req: &ClipRequest,
        key: ClipKey,
        range: Option<&str>,
    ) -> Result<Payload> {
        let (current, plan) = self.clip_plan(project, version, req)?;
        if current != key {
            return Err(MmError::NotFound(format!(
                "短い動画 {key} は今の中身のもの（{current}）でない"
            )));
        }
        let path = self.shared.cache.clip(key);
        for _ in 0..2 {
            self.ensure_clip(key, &path, project, &plan, &req.touched)?;
            touch(&path);
            match clip_file(&path, key, range) {
                // 読む前にキャッシュの上限で捨てられたら、もう一度作る。
                Err(MmError::Io(e)) if e.kind() == std::io::ErrorKind::NotFound => continue,
                other => return other,
            }
        }
        Err(MmError::Internal(format!(
            "短い動画 {key} を作っても読めない"
        )))
    }

    fn clip_plan(
        &self,
        project: &Project,
        version: u64,
        req: &ClipRequest,
    ) -> Result<(ClipKey, ClipPlan)> {
        let plan = ClipPlan::new(project, req)?;
        let keying = self.keying()?;
        let env = KeyEnv {
            project,
            nodes: &keying.registry,
            backends: &keying.backends,
            version,
        };
        Ok((clip_key(&env, &plan, &req.ops)?, plan))
    }

    /// 短い動画が無ければ描画のスレッドで作って待つ。同じキーの要求は先の要求の出来上がりを待つ。
    fn ensure_clip(
        &self,
        key: ClipKey,
        path: &Path,
        project: &Project,
        plan: &ClipPlan,
        touched: &[String],
    ) -> Result<()> {
        let slot = match self.shared.clips.begin(key, || path.exists())? {
            ClipStart::Ready => return Ok(()),
            ClipStart::Wait(slot) => slot,
            ClipStart::Render(slot) => {
                let (clips, cache) = (self.shared.clips.clone(), self.shared.cache.clone());
                let (project, plan) = (project.clone(), plan.clone());
                let (out, touched) = (path.to_path_buf(), touched.to_vec());
                self.shared
                    .worker
                    .urgent(Urgent::Run(Box::new(move |engine| {
                        let result =
                            engine.and_then(|e| render_clip(e, &project, &plan, &out, &touched));
                        if result.is_ok() {
                            cache.evict_clips(CLIP_CACHE_LIMIT_BYTES);
                        }
                        clips.finish(key, result);
                    })));
                slot
            }
        };
        slot.wait_for("短い動画を作る", CLIP_TIMEOUT)
    }
}

/// 短い動画のファイルを返す。中身はキーで決まり URL にも入るので `immutable` にし、`ETag` もキーにする。
fn clip_file(path: &Path, key: ClipKey, range: Option<&str>) -> Result<Payload> {
    ranged_file(path, "video/mp4", (&format!("\"{key}\""), IMMUTABLE), range)
}
