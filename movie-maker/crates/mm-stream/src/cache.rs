//! ディスクのキャッシュ（`cache/mobile/`）の置き場所。消しても作り直せる。

use std::fs;
use std::path::{Path, PathBuf};

use mm_types::Result;

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

    /// 区間のファイルの置き場所。名前の先頭の `v<版>-` で版ごとに捨てる。
    pub(crate) fn segment(
        &self,
        sequence: &str,
        quality: &str,
        version: u64,
        index: u64,
    ) -> PathBuf {
        self.root
            .join(HLS_DIR)
            .join(sanitize(sequence))
            .join(quality)
            .join(format!("{}{index}.ts", version_prefix(version)))
    }

    /// 素材の音だけのファイルの置き場所。素材の内容のハッシュで決まるので版は要らない。
    pub(crate) fn audio(&self, content_hash: &str) -> PathBuf {
        self.root
            .join(AUDIO_DIR)
            .join(format!("{}.m4a", sanitize(content_hash)))
    }

    /// 区間のキャッシュを全部捨てる。
    pub(crate) fn clear_hls(&self) -> Result<()> {
        match fs::remove_dir_all(self.root.join(HLS_DIR)) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.into()),
            _ => Ok(()),
        }
    }

    /// `keep` 以外の版の区間と書きかけを捨てる。
    pub(crate) fn prune_hls(&self, keep: u64) {
        let prefix = version_prefix(keep);
        for dir in leaf_dirs(&self.root.join(HLS_DIR)) {
            for entry in fs::read_dir(dir).into_iter().flatten().flatten() {
                let name = entry.file_name();
                if !name.to_string_lossy().starts_with(&prefix) {
                    let _ = fs::remove_file(entry.path());
                }
            }
        }
    }
}

fn version_prefix(version: u64) -> String {
    format!("v{version}-")
}

/// `hls/<列>/<画質>/` の 2 段下のディレクトリ。
fn leaf_dirs(hls: &Path) -> Vec<PathBuf> {
    let subdirs = |p: &Path| -> Vec<PathBuf> {
        fs::read_dir(p)
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.is_dir())
            .collect()
    };
    subdirs(hls).iter().flat_map(|s| subdirs(s)).collect()
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

    #[test]
    fn 版が違う区間だけを捨てる() {
        let dir = tempfile::tempdir().unwrap();
        let cache = CacheDir::new(dir.path().to_path_buf());
        let keep = cache.segment("main", "360p", 2, 0);
        let drop_old = cache.segment("main", "360p", 1, 0);
        for p in [&keep, &drop_old] {
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(p, b"x").unwrap();
        }
        cache.prune_hls(2);
        assert!(keep.exists());
        assert!(!drop_old.exists());
        cache.clear_hls().unwrap();
        assert!(!keep.exists());
    }

    #[test]
    fn ファイル名にできない文字は置き換える() {
        assert_eq!(sanitize("a/b:c"), "a_b_c");
        assert_eq!(sanitize(""), "_");
    }
}
