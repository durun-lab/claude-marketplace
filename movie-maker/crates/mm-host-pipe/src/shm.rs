//! アプリとホストの間で大きなデータ（画像、音のブロック）を渡す、名前つきの共有メモリとイベント。
//!
//! アプリが名前つきで作り、名前を JSON の制御（パイプ）か起動の引数でホストに渡し、ホストが名前で開く。
//! 中身を触ってよいのは番を持つ側だけで、番の受け渡しはパイプの要求と応答か、イベント（[`Event`]）で行う。
//! どちらも両方のプロセスのメモリの順序をそろえる。`mm-host-ofx`（画像、パイプで番を渡す）と
//! `mm-host-clap`（音、イベントで番を渡す）が使う。

#[cfg(windows)]
mod imp;
#[cfg(not(windows))]
mod stub;

#[cfg(windows)]
pub use imp::{Event, Mapping, Process};
#[cfg(not(windows))]
pub use stub::{Event, Mapping, Process};

/// 待った結果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Wake {
    /// イベントが立った。
    Signaled,
    /// 相手のプロセスが終わった。
    Exited,
    /// 期限を過ぎた。
    TimedOut,
}

/// このプロセスで重ならない元の名前（`Local\<prefix>-<pid>-<番号>`）。`Local\` はログオンのセッションの中だけで見える名前空間。
pub fn unique_link(prefix: &str) -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let n = NEXT.fetch_add(1, Ordering::Relaxed);
    format!("Local\\{prefix}-{}-{n}", std::process::id())
}

/// 共有メモリと 2 つのイベント（要求、完了）の名前。`link` はアプリが作る元の名前。
pub fn object_names(link: &str) -> [String; 3] {
    ["mem", "req", "done"].map(|suffix| format!("{link}-{suffix}"))
}

/// `bytes` をそのまま f32（リトルエンディアン）の列として見る。写さずに済むので、画像の受け渡しはこれを使う。
/// ビッグエンディアンの機械か、f32 の境界に合わないか、長さが 4 の倍数でなければ `None`。
pub fn as_f32(bytes: &[u8]) -> Option<&[f32]> {
    if cfg!(target_endian = "big") {
        return None;
    }
    // SAFETY: どのビット列も f32 として正しい。境界に合わない前後の余りがあれば使わない。
    let (pre, mid, post) = unsafe { bytes.align_to::<f32>() };
    (pre.is_empty() && post.is_empty()).then_some(mid)
}

/// [`as_f32`] の書ける版。
pub fn as_f32_mut(bytes: &mut [u8]) -> Option<&mut [f32]> {
    if cfg!(target_endian = "big") {
        return None;
    }
    // SAFETY: as_f32 と同じ。
    let (pre, mid, post) = unsafe { bytes.align_to_mut::<f32>() };
    (pre.is_empty() && post.is_empty()).then_some(mid)
}

/// `bytes` の `offset` バイト目から `n` 個の f32（リトルエンディアン）を読む。範囲の外なら `None`。
pub fn read_f32(bytes: &[u8], offset: usize, n: usize) -> Option<Vec<f32>> {
    let src = bytes.get(offset..offset.checked_add(n.checked_mul(4)?)?)?;
    if let Some(view) = as_f32(src) {
        return Some(view.to_vec());
    }
    Some(
        src.chunks_exact(4)
            .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
            .collect(),
    )
}

/// `bytes` の `offset` バイト目から `data` を f32（リトルエンディアン）で書く。範囲の外なら `None`。
pub fn write_f32(bytes: &mut [u8], offset: usize, data: &[f32]) -> Option<()> {
    let end = offset.checked_add(data.len().checked_mul(4)?)?;
    let dst = bytes.get_mut(offset..end)?;
    if let Some(view) = as_f32_mut(dst) {
        view.copy_from_slice(data);
        return Some(());
    }
    for (d, v) in dst.chunks_exact_mut(4).zip(data) {
        d.copy_from_slice(&v.to_le_bytes());
    }
    Some(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 名前は元の名前から_3_つ作り_重ならない() {
        let [mem, req, done] = object_names("Local\\x");
        assert_eq!(
            (mem.as_str(), req.as_str(), done.as_str()),
            ("Local\\x-mem", "Local\\x-req", "Local\\x-done")
        );
        assert_ne!(unique_link("mm-test"), unique_link("mm-test"));
    }

    #[test]
    fn f32_を往復し_範囲の外は断る() {
        let mut bytes = vec![0u8; 16];
        write_f32(&mut bytes, 4, &[1.5, -2.0]).unwrap();
        assert_eq!(read_f32(&bytes, 4, 2), Some(vec![1.5, -2.0]));
        assert_eq!(read_f32(&bytes, 12, 2), None);
        assert_eq!(write_f32(&mut bytes, 12, &[1.0, 2.0]), None);
        // 境界に合わない位置は 1 つずつ写す経路で読み書きする。
        write_f32(&mut bytes, 1, &[3.25]).unwrap();
        assert_eq!(read_f32(&bytes, 1, 1), Some(vec![3.25]));
    }

    #[cfg(windows)]
    #[test]
    fn 作った区画を名前で開いて読み書きできる() {
        let name = unique_link("mm-pipe-test");
        let mut made = Mapping::create(&name, 4096).expect("作れる");
        assert!(made.bytes().iter().all(|b| *b == 0), "0 で埋まっている");
        made.bytes_mut()[0] = 7;
        let opened = Mapping::open(&name, 4096).expect("開ける");
        assert_eq!(opened.bytes()[0], 7);
        assert!(
            Mapping::open(&name, 1 << 20).is_err(),
            "区画より大きくは開けない"
        );
        assert!(Mapping::open(&unique_link("mm-pipe-none"), 16).is_err());
    }
}
