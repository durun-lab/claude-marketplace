use super::*;
use mm_ui_wgpu::{cell_len, CELL_H, CELL_W};

/// 左半分が赤、右半分が青の 64x36 の JPEG。
const RED_BLUE: &[u8] = include_bytes!("red-blue.jpg");

/// スロットの `(x, y)` の画素。
fn px(cell: &[u8], x: u32, y: u32) -> [u8; 3] {
    let i = ((y * CELL_W + x) * 4) as usize;
    [cell[i], cell[i + 1], cell[i + 2]]
}

fn write_thumb(cache: &Path, hash: &str, bytes: &[u8]) {
    let path = CacheLayout::new(cache).thumbnail(hash);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, bytes).unwrap();
}

#[test]
fn jpeg_をスロットの大きさの_rgba_にする() {
    let dir = tempfile::tempdir().unwrap();
    write_thumb(dir.path(), "h", RED_BLUE);
    let cell = load_cell(&CacheLayout::new(dir.path()).thumbnail("h")).unwrap();
    assert_eq!(cell.len(), cell_len());
    let (r, b) = (px(&cell, 8, CELL_H / 2), px(&cell, CELL_W - 8, CELL_H / 2));
    assert!(r[0] > 200 && r[2] < 60, "左は赤 {r:?}");
    assert!(b[2] > 200 && b[0] < 60, "右は青 {b:?}");
}

#[test]
fn 無いファイルと壊れたファイルは_none() {
    let dir = tempfile::tempdir().unwrap();
    let layout = CacheLayout::new(dir.path());
    assert!(load_cell(&layout.thumbnail("none")).is_none());
    write_thumb(dir.path(), "broken", b"not a jpeg");
    assert!(load_cell(&layout.thumbnail("broken")).is_none());
}

#[test]
fn 読み込みは別のスレッドで行い_鍵ごとに結果を返す() {
    let dir = tempfile::tempdir().unwrap();
    write_thumb(dir.path(), "good", RED_BLUE);
    let sync = ThumbSync::new(&Shared::new(), CacheLayout::new(dir.path()));
    for key in ["good", "gone"] {
        sync.requests.send(key.into()).unwrap();
    }
    let timeout = std::time::Duration::from_secs(10);
    let first = sync.loaded.recv_timeout(timeout).unwrap();
    let second = sync.loaded.recv_timeout(timeout).unwrap();
    assert_eq!((&*first.0, first.1.is_some()), ("good", true));
    assert_eq!((&*second.0, second.1.is_none()), ("gone", true));
}
