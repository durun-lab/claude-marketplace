use super::*;

fn key(s: &str) -> Arc<str> {
    s.into()
}

#[test]
fn スロットに無いサムネイルは一度だけ頼み_届くまでは描かない() {
    let mut s = Slots::new(4);
    s.begin_frame();
    assert_eq!(s.look(&key("a")), None);
    s.begin_frame();
    assert_eq!(s.look(&key("a")), None, "頼み済みは頼み直さない");
    assert_eq!(s.take_requests(), [key("a")]);
    let slot = s.insert("a").unwrap();
    s.begin_frame();
    assert_eq!(s.look(&key("a")), Some(slot));
    assert!(s.take_requests().is_empty());
}

#[test]
fn スロットが埋まったら見えていないサムネイルのうち_いちばん前に使ったものから捨てる() {
    let mut s = Slots::new(3);
    for name in ["a", "b", "c"] {
        s.begin_frame();
        s.look(&key(name));
        s.insert(name).unwrap();
    }
    // a と c だけを見せ続け、b は前のフレームに使ったきり。
    s.begin_frame();
    let slot_a = s.look(&key("a")).unwrap();
    let slot_b = s.look(&key("b")).unwrap();
    s.begin_frame();
    s.look(&key("a")).unwrap();
    s.look(&key("c")).unwrap();
    let got = s.insert("d").unwrap();
    assert_eq!(got, slot_b, "見えていない b のスロット");
    assert_ne!(got, slot_a);
    s.begin_frame();
    assert_eq!(
        s.look(&key("b")),
        None,
        "捨てたサムネイルは無い（見えたら頼み直す）"
    );
    assert_eq!(s.look(&key("d")), Some(got));
    assert_eq!(s.resident(), 3);
}

#[test]
fn 全部が見えていれば捨てず_少し待ってから頼み直す() {
    let mut s = Slots::new(1);
    s.begin_frame();
    s.look(&key("a"));
    s.insert("a").unwrap();
    s.look(&key("a"));
    s.look(&key("b"));
    assert_eq!(s.insert("b"), None, "a は今のフレームで見えている");
    s.take_requests();
    for _ in 0..DEFER_FRAMES - 1 {
        s.begin_frame();
        s.look(&key("b"));
    }
    assert!(s.take_requests().is_empty(), "待つ間は頼まない");
    s.begin_frame();
    s.look(&key("b"));
    assert_eq!(s.take_requests(), [key("b")]);
}

#[test]
fn 読めなかったサムネイルは頼み直さず_取り込みを終えたら頼み直す() {
    let mut s = Slots::new(2);
    s.begin_frame();
    s.look(&key("a"));
    s.take_requests();
    s.missing("a");
    assert!(s.is_missing("a") && s.has_missing());
    s.begin_frame();
    assert_eq!(s.look(&key("a")), None);
    assert!(s.take_requests().is_empty());
    s.retry_missing();
    assert!(!s.has_missing());
    s.begin_frame();
    s.look(&key("a"));
    assert_eq!(s.take_requests(), [key("a")]);
}

#[test]
fn スロットは並びの順に左から右へ_上から下へ置く() {
    assert_eq!(cell_origin(0), (0, 0));
    assert_eq!(cell_origin(COLS), (0, CELL_H));
    assert_eq!(cell_origin(COLS + 2), (2 * CELL_W, CELL_H));
    assert_eq!(cell_origin(CAPACITY - 1).1 + CELL_H, ROWS * CELL_H);
}

#[test]
fn スロットの縁は半画素内側に取り_全体の範囲がスロットに収まる() {
    let uv = atlas_uv(0, Rect::new(0.0, 0.0, 1.0, 1.0));
    let (w, h) = ((COLS * CELL_W) as f32, (ROWS * CELL_H) as f32);
    assert_eq!(uv[0], 0.5 / w);
    assert_eq!(uv[2], (CELL_W as f32 - 0.5) / w);
    assert_eq!(uv[3], (CELL_H as f32 - 0.5) / h);
    let lower = atlas_uv(0, Rect::new(0.0, 0.5, 1.0, 0.5));
    assert!(lower[1] > uv[1] && lower[3] == uv[3], "下半分だけ");
}

#[test]
fn 横長の画像は中央を_16_対_9_に切り出してスロットの大きさに縮める() {
    // 左 1/4 が赤、中央が緑、右 1/4 が青の 400x100。16:9 に切ると幅 177.8 の中央が残り、左右の色は入らない。
    let (w, h) = (400u32, 100u32);
    let rgb: Vec<u8> = (0..h)
        .flat_map(|_| {
            (0..w).flat_map(|x| match x {
                0..=99 => [255, 0, 0],
                100..=299 => [0, 255, 0],
                _ => [0, 0, 255],
            })
        })
        .collect();
    let cell = cell_from_rgb(w, h, &rgb).unwrap();
    assert_eq!(cell.len(), cell_len());
    let px = |x: u32, y: u32| {
        let i = ((y * CELL_W + x) * 4) as usize;
        [cell[i], cell[i + 1], cell[i + 2], cell[i + 3]]
    };
    assert_eq!(px(0, 0), [0, 255, 0, 255]);
    assert_eq!(px(CELL_W - 1, CELL_H - 1), [0, 255, 0, 255]);
}

#[test]
fn 縮めるときは範囲を平均する() {
    // 1 画素おきの白黒の縞は、間引くと全部白か全部黒になるが、平均すると灰色になる。
    let (w, h) = (256u32, 144u32);
    let rgb: Vec<u8> = (0..h)
        .flat_map(|_| (0..w).flat_map(|x| if x % 2 == 0 { [255; 3] } else { [0; 3] }))
        .collect();
    let cell = cell_from_rgb(w, h, &rgb).unwrap();
    let v = i32::from(cell[(10 * CELL_W + 10) as usize * 4]);
    assert!((v - 128).abs() <= 2, "{v}");
}

#[test]
fn 大きさの合わない画素の並びは受けない() {
    assert!(cell_from_rgb(4, 4, &[0; 10]).is_none());
    assert!(cell_from_rgb(0, 4, &[]).is_none());
}
