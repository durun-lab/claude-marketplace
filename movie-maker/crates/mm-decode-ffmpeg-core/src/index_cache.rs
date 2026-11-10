//! PTS の索引のキャッシュ。素材を開くたびに全パケットを読む（1 時間の 4K 50 Mbps で約 22 GB）のをやめ、
//! 1 回目に作った索引を `cache/` に置いて、2 回目からは読まない。
//!
//! - 引く名前: 素材の指紋（大きさと、先頭・中央・末尾の 64 KiB の BLAKE3）とストリーム番号。パスに依らないので、
//!   素材を動かしても、プロジェクトを別の場所にコピーしても当たる。指紋は素材の全体を読まずに計算できる
//!   （`Asset.contentHash` は全体を読む取り込みのハッシュで、デコーダーには渡らない）。
//! - コンテナの索引（mov の `index_entries` など）を使わない理由: エントリーの時刻は DTS で、B フレームの
//!   PTS には composition offset が要り、パケットを読まずには得られない。mkv の cues は粗くて全フレームに足りない。
//! - 置き場所は処理全体で 1 つ（`set_index_cache`）。3 つのデコーダー（ffmpeg、d3d12va、cuvid）が同じ
//!   `Timeline::build` を通るので、デコーダーのインターフェースを変えずに全部に効く。`Engine` の組み立ては
//!   最初に決めた置き場所を使い続ける（`init_index_cache`）。アプリは Program の後に Source、書き出し、配信の
//!   `Engine` も組むので、組むたびに置き直すと、先に作った索引が引けなくなる。
//! - 大きな素材（長い録画）の全パケットを読む走査は、処理全体で一度に 1 つ。長い録画を並べて読むとどれも遅くなり
//!   （4 本を並べると 4 GB の走査が 28 秒から 65 秒）、同じ素材を複数のデコーダーが同時に開くと同じファイルを読み直すため。
//!   待つ間に同じ素材の索引が置かれたら、読まずに引く。小さな素材（画像、短い素材）は走査のロックを待たない。

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError, RwLock};

use mm_cache_api::{CacheKey, CacheKind, CacheStore, ContentHash};
use mm_types::Result;

use crate::timeline::{scan_video, VideoScan};

/// 索引の形を変えたら上げる（古い索引は引かれなくなる）。
const INDEX_VERSION: u32 = 1;
/// 指紋に使う、素材の 1 か所あたりのバイト数。
const SAMPLE_BYTES: u64 = 64 * 1024;
/// 指紋に使う、素材の中の場所の数（先頭、中央、末尾）。
const SAMPLE_SPOTS: u64 = 3;
/// 走査のロックを取る素材の大きさの下限。150 MB/s のディスクで 2 秒ほど読む大きさ。
const LARGE_BYTES: u64 = 256 * 1024 * 1024;

static STORE: RwLock<Option<Arc<dyn CacheStore>>> = RwLock::new(None);
/// 大きな素材の全パケットを読む走査のロック。走査を処理全体で一度に 1 つにする。
static SCANNING: Mutex<()> = Mutex::new(());

/// 索引を置く場所を決める。`None` で置かない（毎回読む）。
pub fn set_index_cache(store: Option<Arc<dyn CacheStore>>) {
    *STORE.write().expect("index cache lock") = store;
}

/// 索引を置く場所がまだ無ければ `store` に決めて真を返す。決まっていれば変えずに偽を返す。
pub fn init_index_cache(store: Arc<dyn CacheStore>) -> bool {
    let mut slot = STORE.write().expect("index cache lock");
    let empty = slot.is_none();
    if empty {
        *slot = Some(store);
    }
    empty
}

fn store() -> Option<Arc<dyn CacheStore>> {
    STORE.read().expect("index cache lock").clone()
}

/// `scan_video` の結果を `cache/` から引く。無ければ読んで置く（大きな素材は走査のロックを待つ）。
pub fn scan_video_cached(path: &Path, stream_index: usize) -> Result<VideoScan> {
    let large = std::fs::metadata(path).is_ok_and(|m| m.len() >= LARGE_BYTES);
    let scan = || scan_video(path, stream_index);
    let key = store().map(|store| {
        let key = fingerprint(path).map(|hash| CacheKey {
            kind: CacheKind::Intermediate,
            content_hash: ContentHash(hash),
            variant: format!("video-index-v{INDEX_VERSION}-s{stream_index}"),
        });
        (store, key)
    });
    match key {
        Some((store, Ok(key))) => cached_or_scan(store.as_ref(), &key, large, scan),
        _ => {
            let _lock = scan_lock(large);
            scan()
        }
    }
}

/// 置いてある索引を引く。無ければ（大きな素材は走査のロックを取ってから）`scan` で作って置く。待つ間に置かれた索引は読まずに引く。
fn cached_or_scan(
    store: &dyn CacheStore,
    key: &CacheKey,
    large: bool,
    scan: impl FnOnce() -> Result<VideoScan>,
) -> Result<VideoScan> {
    if let Some(found) = cached(store, key) {
        return Ok(found);
    }
    let _lock = scan_lock(large);
    if let Some(found) = cached(store, key) {
        return Ok(found);
    }
    let made = scan()?;
    if let Err(e) = store.put(key, &made.to_bytes()) {
        tracing::warn!(error = %e, "PTS の索引をキャッシュに置けない");
    }
    Ok(made)
}

/// 大きな素材の走査のロック。小さな素材は取らない。
fn scan_lock(large: bool) -> Option<MutexGuard<'static, ()>> {
    large.then(|| SCANNING.lock().unwrap_or_else(PoisonError::into_inner))
}

/// 置いてある索引。壊れていれば無いことにする。
fn cached(store: &dyn CacheStore, key: &CacheKey) -> Option<VideoScan> {
    let scan = store
        .get(key)
        .ok()
        .flatten()
        .and_then(|b| VideoScan::from_bytes(&b))?;
    mm_trace::counter!("decode.index_cache_hits", 1);
    Some(scan)
}

/// 素材の指紋。大きさと、先頭・中央・末尾の `SAMPLE_BYTES`。
fn fingerprint(path: &Path) -> Result<String> {
    let mut file = File::open(path)?;
    let len = file.metadata()?.len();
    let mut hasher = blake3::Hasher::new();
    hasher.update(&len.to_le_bytes());
    let mut buf = vec![0u8; SAMPLE_BYTES as usize];
    for spot in 0..SAMPLE_SPOTS {
        let at = len.saturating_sub(SAMPLE_BYTES) * spot / (SAMPLE_SPOTS - 1);
        file.seek(SeekFrom::Start(at))?;
        let n = file.read(&mut buf)?;
        hasher.update(&buf[..n]);
    }
    Ok(hasher.finalize().to_hex().to_string())
}

impl VideoScan {
    /// キャッシュに置く形。8 バイトの整数の並び（バージョン、フレーム数、最後の長さ、キーフレーム数、PTS、キーフレーム）。
    pub fn to_bytes(&self) -> Vec<u8> {
        let words = 4 + self.raw_pts.len() + self.keyframes.len();
        let mut out = Vec::with_capacity(words * 8);
        let header = [
            u64::from(INDEX_VERSION),
            self.raw_pts.len() as u64,
            self.last_duration as u64,
            self.keyframes.len() as u64,
        ];
        for w in header {
            out.extend_from_slice(&w.to_le_bytes());
        }
        for v in self.raw_pts.iter().chain(&self.keyframes) {
            out.extend_from_slice(&v.to_le_bytes());
        }
        out
    }

    /// `to_bytes` の逆。形が合わない、PTS が昇順でない、キーフレームの番号が範囲外のものは `None`。
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        let words: Vec<i64> = bytes
            .chunks_exact(8)
            .map(|c| i64::from_le_bytes(c.try_into().expect("8 バイト")))
            .collect();
        if !bytes.len().is_multiple_of(8) || words.len() < 4 || words[0] != i64::from(INDEX_VERSION)
        {
            return None;
        }
        let (count, last_duration, keys) = (
            usize::try_from(words[1]).ok()?,
            words[2],
            usize::try_from(words[3]).ok()?,
        );
        if words.len() != 4 + count.checked_add(keys)? || count == 0 {
            return None;
        }
        let raw_pts = words[4..4 + count].to_vec();
        let keyframes = words[4 + count..].to_vec();
        let sorted = raw_pts.windows(2).all(|w| w[0] < w[1]);
        let in_range = keyframes.iter().all(|k| (0..count as i64).contains(k));
        (sorted && in_range).then_some(Self {
            raw_pts,
            last_duration,
            keyframes,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    use super::*;

    fn scan() -> VideoScan {
        VideoScan {
            raw_pts: vec![0, 512, 1024, 2048],
            last_duration: 512,
            keyframes: vec![0, 2],
        }
    }

    #[test]
    fn 索引は往復する() {
        let s = scan();
        let back = VideoScan::from_bytes(&s.to_bytes()).unwrap();
        assert_eq!(
            (back.raw_pts, back.last_duration, back.keyframes),
            (s.raw_pts, s.last_duration, s.keyframes)
        );
    }

    #[test]
    fn 壊れた索引は引かなかったことにする() {
        let good = scan().to_bytes();
        assert!(VideoScan::from_bytes(&good[..good.len() - 1]).is_none());
        assert!(VideoScan::from_bytes(&good[..good.len() - 8]).is_none());
        let mut bad_version = good.clone();
        bad_version[0] = 99;
        assert!(VideoScan::from_bytes(&bad_version).is_none());
        let unsorted = VideoScan {
            raw_pts: vec![5, 3],
            ..scan()
        };
        assert!(VideoScan::from_bytes(&unsorted.to_bytes()).is_none());
        let key_out = VideoScan {
            keyframes: vec![9],
            ..scan()
        };
        assert!(VideoScan::from_bytes(&key_out.to_bytes()).is_none());
        assert!(VideoScan::from_bytes(&[]).is_none());
    }

    /// 試験用の置き場所。置いた回数を数える。
    #[derive(Default)]
    struct Counting {
        map: Mutex<HashMap<CacheKey, Vec<u8>>>,
        puts: AtomicUsize,
    }

    impl CacheStore for Counting {
        fn id(&self) -> &'static str {
            "counting"
        }
        fn get(&self, key: &CacheKey) -> Result<Option<Vec<u8>>> {
            Ok(self.map.lock().unwrap().get(key).cloned())
        }
        fn put(&self, key: &CacheKey, bytes: &[u8]) -> Result<()> {
            self.puts.fetch_add(1, Ordering::SeqCst);
            self.map.lock().unwrap().insert(key.clone(), bytes.to_vec());
            Ok(())
        }
        fn remove(&self, key: &CacheKey) -> Result<()> {
            self.map.lock().unwrap().remove(key);
            Ok(())
        }
        fn size_bytes(&self) -> u64 {
            0
        }
        fn limit_bytes(&self) -> Option<u64> {
            None
        }
        fn clear(&self) -> Result<()> {
            self.map.lock().unwrap().clear();
            Ok(())
        }
    }

    fn key(hash: &str) -> CacheKey {
        CacheKey {
            kind: CacheKind::Intermediate,
            content_hash: ContentHash(hash.into()),
            variant: "video-index-v1-s0".into(),
        }
    }

    #[test]
    fn 大きな素材を同時に開いても全パケットを読むのは_1_回で_後の方は置かれた索引を引く() {
        let store = Arc::new(Counting::default());
        let scans = Arc::new(AtomicUsize::new(0));
        let open = || {
            let (store, scans) = (store.clone(), scans.clone());
            std::thread::spawn(move || {
                let slow = || {
                    scans.fetch_add(1, Ordering::SeqCst);
                    std::thread::sleep(Duration::from_millis(100));
                    Ok(scan())
                };
                let found = cached_or_scan(store.as_ref(), &key("abc"), true, slow);
                found.unwrap().raw_pts
            })
        };
        let (a, b) = (open(), open());
        assert_eq!(a.join().unwrap(), b.join().unwrap());
        let count = |n: &AtomicUsize| n.load(Ordering::SeqCst);
        assert_eq!((count(&scans), count(&store.puts)), (1, 1));
    }

    #[test]
    fn 置き場所は最初に決めたものを使い続け_後から組む_engine_は置き直さない() {
        let first: Arc<dyn CacheStore> = Arc::new(Counting::default());
        set_index_cache(None);
        assert!(init_index_cache(first.clone()));
        assert!(!init_index_cache(Arc::new(Counting::default())));
        let kept = store().expect("置き場所がある");
        set_index_cache(None);
        assert!(Arc::ptr_eq(&kept, &first));
    }

    #[test]
    fn 小さな素材は大きな素材の走査を待たずに読む() {
        let store = Arc::new(Counting::default());
        let (inside, scanning) = std::sync::mpsc::channel();
        let big = {
            let store = store.clone();
            std::thread::spawn(move || {
                let slow = || {
                    inside.send(()).unwrap();
                    std::thread::sleep(Duration::from_millis(600));
                    Ok(scan())
                };
                cached_or_scan(store.as_ref(), &key("big"), true, slow).unwrap();
            })
        };
        scanning.recv().unwrap();
        let started = std::time::Instant::now();
        cached_or_scan(store.as_ref(), &key("small"), false, || Ok(scan())).unwrap();
        let took = started.elapsed();
        assert!(took < Duration::from_millis(300), "待った: {took:?}");
        big.join().unwrap();
    }

    #[test]
    fn 指紋は中身で決まりパスに依らない() {
        let guard = tempfile::Builder::new().prefix("mm-fp-").tempdir().unwrap();
        let dir = guard.path().to_path_buf();
        let (a, b, c) = (dir.join("a.bin"), dir.join("b.bin"), dir.join("c.bin"));
        let body: Vec<u8> = (0..400_000u32).map(|i| (i % 251) as u8).collect();
        std::fs::write(&a, &body).unwrap();
        std::fs::write(&b, &body).unwrap();
        let mut changed = body.clone();
        *changed.last_mut().unwrap() ^= 1;
        std::fs::write(&c, &changed).unwrap();
        assert_eq!(fingerprint(&a).unwrap(), fingerprint(&b).unwrap());
        assert_ne!(fingerprint(&a).unwrap(), fingerprint(&c).unwrap());
    }
}
