//! ディスクのキャッシュ（`cache/mobile/`）の置き場所。消しても作り直せる。

use std::fs;
use std::path::PathBuf;
use std::time::SystemTime;

use crate::key::{SegmentKey, SEGMENT_EXT};

const HLS_DIR: &str = "hls";
const AUDIO_DIR: &str = "audio";
/// 書きかけのファイルの拡張子。完成してから名前を替える。
pub(crate) const PARTIAL_EXT: &str = "partial";

/// キャッシュの根。
#[derive(Debug, Clone)]
pub(crate) struct CacheDir {
    root: PathBuf,
}

impl CacheDir {
    pub(crate) fn new(root: PathBuf) -> Self {
        Self { root }
    }

    /// 区間のファイルの置き場所。キーが中身で決まるので、プロジェクトを開き直しても使える。
    pub(crate) fn segment(&self, key: SegmentKey) -> PathBuf {
        self.root.join(HLS_DIR).join(key.file_name())
    }

    /// 素材の音だけのファイルの置き場所。素材の内容のハッシュで決まる。
    pub(crate) fn audio(&self, content_hash: &str) -> PathBuf {
        self.root
            .join(AUDIO_DIR)
            .join(format!("{}.m4a", sanitize(content_hash)))
    }

    /// 前の起動が残した書きかけ（区間で終わらない名前）を捨てる。
    pub(crate) fn remove_partials(&self) {
        for entry in fs::read_dir(self.root.join(HLS_DIR))
            .into_iter()
            .flatten()
            .flatten()
        {
            if !is_segment(&entry.path()) {
                let _ = fs::remove_file(entry.path());
            }
        }
    }

    /// 区間の合計が `limit` バイトを超えたら、使われた時刻の古いものから捨てる。
    pub(crate) fn evict_segments(&self, limit: u64) {
        let mut files: Vec<(SystemTime, u64, PathBuf)> = fs::read_dir(self.root.join(HLS_DIR))
            .into_iter()
            .flatten()
            .flatten()
            .filter(|e| is_segment(&e.path()))
            .filter_map(|e| {
                let meta = e.metadata().ok()?;
                Some((meta.modified().ok()?, meta.len(), e.path()))
            })
            .collect();
        let mut total: u64 = files.iter().map(|f| f.1).sum();
        if total <= limit {
            return;
        }
        files.sort();
        for (_, len, path) in files {
            if total <= limit {
                break;
            }
            if fs::remove_file(&path).is_ok() {
                total -= len;
            }
        }
    }
}

/// 使った時刻を今にする。捨てる順は更新時刻で決めるので、読んだ区間を残す側に回す。
pub(crate) fn touch(path: &std::path::Path) {
    if let Ok(f) = fs::File::options().write(true).open(path) {
        let _ = f.set_modified(SystemTime::now());
    }
}

/// `<キー>.ts` の形のファイル。書き出しの一時ファイル（`<キー>.partial.ts`）は配信の区間に数えない。
fn is_segment(path: &std::path::Path) -> bool {
    path.extension().is_some_and(|e| e == SEGMENT_EXT)
        && path
            .file_stem()
            .and_then(|s| s.to_str())
            .is_some_and(|s| s.parse::<SegmentKey>().is_ok())
}

/// ファイル名に使える文字だけにする。
pub(crate) fn sanitize(name: &str) -> String {
    let s: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if s.is_empty() {
        "_".into()
    } else {
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn key(n: u64) -> SegmentKey {
        format!("{n:016x}").parse().unwrap()
    }

    #[test]
    fn 上限を超えたら使った時刻の古い区間から捨てる() {
        let dir = tempfile::tempdir().unwrap();
        let cache = CacheDir::new(dir.path().to_path_buf());
        let paths: Vec<PathBuf> = (0..3).map(|n| cache.segment(key(n))).collect();
        fs::create_dir_all(paths[0].parent().unwrap()).unwrap();
        let base = SystemTime::now() - Duration::from_secs(60);
        for (i, p) in paths.iter().enumerate() {
            fs::write(p, [0u8; 10]).unwrap();
            let f = fs::File::options().write(true).open(p).unwrap();
            f.set_modified(base + Duration::from_secs(i as u64))
                .unwrap();
        }
        touch(&paths[0]);
        cache.evict_segments(20);
        assert!(paths[0].exists(), "読んだばかりの区間は残す");
        assert!(!paths[1].exists());
        assert!(paths[2].exists());
    }

    #[test]
    fn 書きかけだけを捨てる() {
        let dir = tempfile::tempdir().unwrap();
        let cache = CacheDir::new(dir.path().to_path_buf());
        let done = cache.segment(key(1));
        fs::create_dir_all(done.parent().unwrap()).unwrap();
        let partial = done.with_extension("partial.ts");
        fs::write(&done, b"x").unwrap();
        fs::write(&partial, b"x").unwrap();
        cache.remove_partials();
        assert!(done.exists());
        assert!(!partial.exists());
    }

    #[test]
    fn ファイル名にできない文字は置き換える() {
        assert_eq!(sanitize("a/b:c"), "a_b_c");
        assert_eq!(sanitize(""), "_");
    }
}
