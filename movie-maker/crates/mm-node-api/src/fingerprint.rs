//! 種類の指紋（`NodeKind::cache_fingerprint`）に入れる、外のファイルのスタンプとハッシュ。
//!
//! 種類が自分で読む素材のファイル（HLSL、Lottie、LUT）は中身のハッシュで表す（Bazel や Nix の内容でアドレスする考え方）。
//! 取り込んだときの `contentHash` は、取り込んだ後にファイルを書き換えると古くなるため。
//! 実行ファイルや共有ライブラリは大きさと更新時刻で表す（Nuke の Read のハッシュ）。数百 MB を読まずに済む。
//! 中身のハッシュは、大きさと更新時刻が同じ間は読み直さない（git の index）。

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock, PoisonError};
use std::time::{Duration, SystemTime};

/// 更新時刻がこれより新しいファイルの中身のハッシュはキャッシュしない。時刻の刻みの中で書き換えられると、
/// 大きさと更新時刻が同じまま中身が変わる（git の racy git）。
const RACY_WINDOW: Duration = Duration::from_secs(2);
/// キャッシュするファイルの数の上限。超えたら全部捨てる（プロジェクトの素材の数より十分に大きい）。
const MAX_REMEMBERED: usize = 4096;
/// 無いファイルのスタンプ。
const ABSENT: &str = "absent";

/// ファイルの大きさと更新時刻。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Stat {
    len: u64,
    modified: Option<SystemTime>,
}

impl Stat {
    fn of(path: &Path) -> Option<Self> {
        let meta = std::fs::metadata(path).ok()?;
        Some(Self {
            len: meta.len(),
            modified: meta.modified().ok(),
        })
    }

    /// 中身のハッシュをキャッシュしてよいか。更新時刻が無いか新しすぎると、次の書き換えを大きさと時刻で見分けられない。
    fn settled(&self) -> bool {
        self.modified
            .and_then(|m| SystemTime::now().duration_since(m).ok())
            .is_some_and(|age| age >= RACY_WINDOW)
    }

    fn stamp(&self) -> String {
        let nanos = self
            .modified
            .and_then(|m| m.duration_since(SystemTime::UNIX_EPOCH).ok())
            .map_or(0, |d| d.as_nanos());
        format!("{}:{nanos}", self.len)
    }
}

type Digests = HashMap<PathBuf, (Stat, String)>;

fn digests() -> std::sync::MutexGuard<'static, Digests> {
    static DIGESTS: OnceLock<Mutex<Digests>> = OnceLock::new();
    // 表は 1 回の代入で書き換えるだけなので、パニックの後もそのまま使える。
    DIGESTS
        .get_or_init(Mutex::default)
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
}

/// ファイルの中身のハッシュ（16 桁の 16 進）。読めなければ `None`（キャッシュしない）。
/// 大きさと更新時刻が前に読んだときと同じなら、読み直さずに前のハッシュを返す。
pub fn file_digest(path: &Path) -> Option<String> {
    let before = Stat::of(path)?;
    if let Some((stat, digest)) = digests().get(path) {
        if *stat == before {
            return Some(digest.clone());
        }
    }
    let bytes = std::fs::read(path).ok()?;
    let digest = format!("{:016x}", fnv1a(&bytes));
    // 読んでいる間に書き換えられたら、どちらの中身のハッシュかが分からないのでキャッシュしない。
    if Stat::of(path) == Some(before) && before.settled() {
        let mut memo = digests();
        if memo.len() >= MAX_REMEMBERED {
            memo.clear();
        }
        memo.insert(path.to_owned(), (before, digest.clone()));
    }
    Some(digest)
}

/// ファイルの大きさと更新時刻のスタンプ。無いファイルは `absent`。実行ファイルや共有ライブラリの差し替えを見分ける。
pub fn file_stamp(path: &Path) -> String {
    Stat::of(path).map_or_else(|| ABSENT.to_owned(), |s| s.stamp())
}

/// FNV-1a（64 bit）。`DefaultHasher` と違い、値がコンパイラーのバージョンで変わらない（ディスクのキャッシュのキーに入る）。
fn fnv1a(bytes: &[u8]) -> u64 {
    const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;
    bytes
        .iter()
        .fold(OFFSET, |h, b| (h ^ u64::from(*b)).wrapping_mul(PRIME))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 試験ごとに別のファイル。並行の試験が同じパスを書かない。
    fn temp_file(name: &str) -> (tempfile::TempDir, PathBuf) {
        let guard = tempfile::Builder::new()
            .prefix("mm-node-api-fp-")
            .tempdir()
            .unwrap();
        let dir = guard.path().to_path_buf();
        (guard, dir.join(name))
    }

    /// 更新時刻を `RACY_WINDOW` より前にずらす。
    fn settle(path: &Path) {
        let old = SystemTime::now() - RACY_WINDOW * 2;
        let file = std::fs::File::options()
            .write(true)
            .open(path)
            .expect("開ける");
        file.set_modified(old).expect("更新時刻を変えられる");
    }

    #[test]
    fn 中身を書き換えると大きさが同じでもハッシュが変わる() {
        let (_dir, path) = temp_file("rewrite.hlsl");
        std::fs::write(&path, "float4 a;").expect("書ける");
        settle(&path);
        let first = file_digest(&path).expect("読める");
        assert_eq!(
            file_digest(&path),
            Some(first.clone()),
            "キャッシュしたハッシュ"
        );
        std::fs::write(&path, "float4 b;").expect("書ける");
        let second = file_digest(&path).expect("読める");
        assert_ne!(first, second, "同じ大きさの書き換えも見分ける");
    }

    #[test]
    fn 中身が同じなら場所と更新時刻が違っても同じハッシュ() {
        let ((_da, a), (_db, b)) = (temp_file("same-a.json"), temp_file("same-b.json"));
        std::fs::write(&a, "{}").expect("書ける");
        std::fs::write(&b, "{}").expect("書ける");
        settle(&a);
        assert_eq!(file_digest(&a), file_digest(&b));
    }

    #[test]
    fn 新しすぎるファイルのハッシュはキャッシュしない() {
        let (_dir, path) = temp_file("racy.cube");
        std::fs::write(&path, "1").expect("書ける");
        file_digest(&path).expect("読める");
        assert!(
            !digests().contains_key(&path),
            "更新の直後はキャッシュしない"
        );
        settle(&path);
        file_digest(&path).expect("読める");
        assert!(digests().contains_key(&path), "落ち着いたらキャッシュする");
    }

    #[test]
    fn 無いファイルはハッシュが無く_スタンプは_absent() {
        let (_dir, path) = temp_file("none.json");
        assert_eq!(file_digest(&path), None);
        assert_eq!(file_stamp(&path), ABSENT);
    }

    #[test]
    fn スタンプは大きさか更新時刻が変われば変わる() {
        let (_dir, path) = temp_file("host.exe");
        std::fs::write(&path, "ab").expect("書ける");
        settle(&path);
        let first = file_stamp(&path);
        assert_eq!(file_stamp(&path), first);
        std::fs::write(&path, "abc").expect("書ける");
        assert_ne!(file_stamp(&path), first);
    }
}
