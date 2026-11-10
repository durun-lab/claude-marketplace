//! describe の結果のキャッシュ。OpenFX の HostSupport と同じく、バンドルの中の共有ライブラリの更新時刻と大きさで
//! 無効にする。describe は別プロセスを起こしてプラグインを読み込むので、フレームごとやパラメータの一覧を引くたびには行わない。

use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};

use mm_types::{MmError, ParamSchema, Result};

/// describe の結果のうち、種類が使う分。
#[derive(Debug, Clone, PartialEq)]
pub struct OfxPlugin {
    /// `pluginIdentifier`。
    pub id: String,
    /// 種類のパラメータに載せる一覧（隠しのパラメータと値を持たないものを除く）。
    pub schema: ParamSchema,
    /// ホストに値を渡せるパラメータの名前（隠しのものも含む）。ほかの名前の値は渡さない。
    pub settable: Vec<String>,
}

/// バンドルの実体のパスを describe して、描ける効果を返す関数。組み立てがホストの実行ファイルで作って渡す。
pub type Describe = Arc<dyn Fn(&Path) -> Result<Vec<OfxPlugin>> + Send + Sync>;

/// 1 つのバンドルの describe の結果。失敗もキャッシュし、壊れたプラグインを毎フレーム読み込み直さない。
#[derive(Clone)]
struct Entry {
    stamp: u64,
    plugins: std::result::Result<Arc<Vec<OfxPlugin>>, String>,
}

/// バンドルごとの describe の結果。
pub struct Catalog {
    describe: Describe,
    entries: Mutex<HashMap<PathBuf, Entry>>,
}

impl Catalog {
    /// `describe` で引く。
    pub fn new(describe: Describe) -> Self {
        Self {
            describe,
            entries: Mutex::new(HashMap::new()),
        }
    }

    /// バンドルの効果の一覧と、バンドルの中身のスタンプ。中身が変わっていなければ前の結果を返す。
    pub fn plugins(&self, bundle: &Path) -> Result<(Arc<Vec<OfxPlugin>>, u64)> {
        let stamp = stamp(bundle);
        let cached = self
            .lock()
            .get(bundle)
            .filter(|e| e.stamp == stamp)
            .cloned();
        let entry = match cached {
            Some(entry) => entry,
            None => {
                // describe は数十 ms かかるので、ロックの外で行う。
                let entry = Entry {
                    stamp,
                    plugins: (self.describe)(bundle)
                        .map(Arc::new)
                        .map_err(|e| e.to_string()),
                };
                self.lock().insert(bundle.to_path_buf(), entry.clone());
                entry
            }
        };
        entry.plugins.map(|p| (p, stamp)).map_err(|e| {
            MmError::Internal(format!("{} を describe できない: {e}", bundle.display()))
        })
    }

    /// `id` の効果。空なら最初の効果。
    pub fn plugin(&self, bundle: &Path, id: &str) -> Result<(OfxPlugin, u64)> {
        let (plugins, stamp) = self.plugins(bundle)?;
        plugins
            .iter()
            .find(|p| id.is_empty() || p.id == id)
            .map(|p| (p.clone(), stamp))
            .ok_or_else(|| {
                let ids: Vec<&str> = plugins.iter().map(|p| p.id.as_str()).collect();
                MmError::NotFound(format!(
                    "{} に効果 {id:?} が無い（あるのは {ids:?}）",
                    bundle.display()
                ))
            })
    }

    /// 表は 1 回の代入で書き換えるだけなので、パニックの後もそのまま使える。
    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<PathBuf, Entry>> {
        self.entries.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl std::fmt::Debug for Catalog {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Catalog")
            .field("bundles", &self.lock().len())
            .finish()
    }
}

/// バンドルの中身のスタンプ。`Contents/<アーキテクチャ>/` の直下のファイルの名前、大きさ、更新時刻から作る。
/// 共有ライブラリを差し替えると変わり、describe とホストを作り直す。
pub fn stamp(bundle: &Path) -> u64 {
    let mut files: Vec<(PathBuf, u64, Option<std::time::SystemTime>)> = Vec::new();
    let arches = std::fs::read_dir(bundle.join("Contents"))
        .into_iter()
        .flatten();
    for arch in arches.flatten().filter(|e| e.path().is_dir()) {
        let inner = std::fs::read_dir(arch.path()).into_iter().flatten();
        for file in inner.flatten() {
            if let Ok(meta) = file.metadata() {
                files.push((file.path(), meta.len(), meta.modified().ok()));
            }
        }
    }
    files.sort();
    let mut h = DefaultHasher::new();
    files.hash(&mut h);
    h.finish()
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;

    fn counting(calls: Arc<AtomicUsize>) -> Describe {
        Arc::new(move |_bundle: &Path| {
            calls.fetch_add(1, Ordering::SeqCst);
            Ok(vec![OfxPlugin {
                id: "a.b".into(),
                schema: ParamSchema::default(),
                settable: Vec::new(),
            }])
        })
    }

    #[test]
    fn 中身が変わるまで_describe_を繰り返さない() {
        let guard = tempfile::Builder::new()
            .prefix("mm-nodes-ofx-cat-")
            .tempdir()
            .unwrap();
        let dir = guard.path().to_path_buf();
        let arch = dir.join("A.ofx.bundle/Contents/Win64");
        std::fs::create_dir_all(&arch).unwrap();
        std::fs::write(arch.join("A.ofx"), b"1").unwrap();
        let bundle = dir.join("A.ofx.bundle");
        let calls = Arc::new(AtomicUsize::new(0));
        let catalog = Catalog::new(counting(calls.clone()));
        let (first, s1) = catalog.plugin(&bundle, "").unwrap();
        let (_, s2) = catalog.plugin(&bundle, "a.b").unwrap();
        assert_eq!((first.id.as_str(), s1), ("a.b", s2));
        assert_eq!(calls.load(Ordering::SeqCst), 1, "2 回目はキャッシュ");
        assert!(matches!(
            catalog.plugin(&bundle, "x.y"),
            Err(MmError::NotFound(_))
        ));

        std::fs::write(arch.join("A.ofx"), b"22").unwrap();
        let (_, s3) = catalog.plugin(&bundle, "").unwrap();
        assert_ne!(s1, s3, "共有ライブラリが変わればスタンプも変わる");
        assert_eq!(calls.load(Ordering::SeqCst), 2, "変わったら引き直す");
    }

    #[test]
    fn 失敗もキャッシュして繰り返さない() {
        let calls = Arc::new(AtomicUsize::new(0));
        let c = calls.clone();
        let catalog = Catalog::new(Arc::new(move |_: &Path| {
            c.fetch_add(1, Ordering::SeqCst);
            Err(MmError::HostCrashed("クラッシュした".into()))
        }));
        let bundle = Path::new("無い.ofx.bundle");
        assert!(catalog.plugins(bundle).is_err());
        assert!(catalog.plugins(bundle).is_err());
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }
}
