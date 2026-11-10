use super::slot_image::halve;
use super::*;

fn key(s: &str) -> Arc<str> {
    s.into()
}

#[test]
fn スロットに無いサムネイルは一度だけ頼み_届くまでは描かない() {
    let mut s = Slots::new(4);
    s.begin_frame();
    assert_eq!(s.slot_to_draw(&key("a")), None);
    s.begin_frame();
    assert_eq!(s.slot_to_draw(&key("a")), None, "頼み済みは頼み直さない");
    assert_eq!(s.take_requests(), [key("a")]);
    let slot = s.insert("a").unwrap();
    s.begin_frame();
    assert_eq!(s.slot_to_draw(&key("a")), Some(slot));
    assert!(s.take_requests().is_empty());
}

#[test]
fn スロットが埋まったら見えていないサムネイルのうち_いちばん前に使ったものから捨てる() {
    let mut s = Slots::new(3);
    for name in ["a", "b", "c"] {
        s.begin_frame();
        s.slot_to_draw(&key(name));
        s.insert(name).unwrap();
    }
    // a と c だけを見せ続け、b は前のフレームに使ったきり。
    s.begin_frame();
    let slot_a = s.slot_to_draw(&key("a")).unwrap();
    let slot_b = s.slot_to_draw(&key("b")).unwrap();
    s.begin_frame();
    s.slot_to_draw(&key("a")).unwrap();
    s.slot_to_draw(&key("c")).unwrap();
    let got = s.insert("d").unwrap();
    assert_eq!(got, slot_b, "見えていない b のスロット");
    assert_ne!(got, slot_a);
    s.begin_frame();
    assert_eq!(
        s.slot_to_draw(&key("b")),
        None,
        "捨てたサムネイルは無い（見えたら頼み直す）"
    );
    assert_eq!(s.slot_to_draw(&key("d")), Some(got));
    assert_eq!(s.resident(), 3);
}

#[test]
fn 全部が見えていれば捨てず_少し待ってから頼み直す() {
    let mut s = Slots::new(1);
    s.begin_frame();
    s.slot_to_draw(&key("a"));
    s.insert("a").unwrap();
    s.slot_to_draw(&key("a"));
    s.slot_to_draw(&key("b"));
    assert_eq!(s.insert("b"), None, "a は今のフレームで見えている");
    s.take_requests();
    for _ in 0..DEFER_FRAMES - 1 {
        s.begin_frame();
        s.slot_to_draw(&key("b"));
    }
    assert!(s.take_requests().is_empty(), "待つ間は頼まない");
    s.begin_frame();
    s.slot_to_draw(&key("b"));
    assert_eq!(s.take_requests(), [key("b")]);
}

#[test]
fn 読めなかったサムネイルは合図で頼み直し_読めるまでは読めなかったものとして扱う() {
    let mut s = Slots::new(2);
    s.begin_frame();
    s.slot_to_draw(&key("a"));
    s.take_requests();
    s.missing("a");
    assert!(s.is_missing("a") && s.has_missing());
    s.begin_frame();
    assert_eq!(s.slot_to_draw(&key("a")), None);
    assert!(s.take_requests().is_empty(), "合図までは頼み直さない");
    s.retry_missing();
    assert_eq!(s.take_requests(), [key("a")]);
    assert!(s.is_missing("a"), "読み直している途中も読めなかったもの");
    s.retry_missing();
    assert_eq!(s.take_requests(), [key("a")], "途中でも次の合図で頼み直す");
    assert!(s.insert("a").is_some());
    s.missing("a");
    assert!(
        !s.has_missing(),
        "前の読み込みで読めていたら、後の読めなかった結果で捨てない"
    );
}

#[test]
fn スロットは並びの順に左から右へ_上から下へ置く() {
    assert_eq!(slot_origin(0), (0, 0));
    assert_eq!(slot_origin(COLS), (0, SLOT_H));
    assert_eq!(slot_origin(COLS + 2), (2 * SLOT_W, SLOT_H));
    assert_eq!(slot_origin(CAPACITY - 1).1 + SLOT_H, ROWS * SLOT_H);
}

#[test]
fn スロットの縁はいちばん縮めたミップレベルの半画素だけ内側に取り_全体の範囲がスロットに収まる() {
    let uv = atlas_uv(0, Rect::new(0.0, 0.0, 1.0, 1.0));
    let (w, h) = ((COLS * SLOT_W) as f32, (ROWS * SLOT_H) as f32);
    assert_eq!(
        EDGE, 2.0,
        "2 回半分にしたレベルの半画素は、レベル 0 の 2 画素"
    );
    assert_eq!(uv[0], EDGE / w);
    assert_eq!(uv[2], (SLOT_W as f32 - EDGE) / w);
    assert_eq!(uv[3], (SLOT_H as f32 - EDGE) / h);
    let lower = atlas_uv(0, Rect::new(0.0, 0.5, 1.0, 0.5));
    assert!(lower[1] > uv[1] && lower[3] == uv[3], "下半分だけ");
}

#[test]
fn ミップレベルは_2x2_の画素の平均で縦横半分にし_スロットの画像の後ろにレベルの順に並べる() {
    // 左上だけ白、ほかは黒の 2x2 を 2 つ横に並べた 4x2。半分にすると 2x1 で、どちらも白の 4 分の 1。
    let px = |v: u8| [v, v, v, 255];
    let row0: Vec<u8> = [px(255), px(0), px(255), px(0)].concat();
    let row1: Vec<u8> = [px(0); 4].concat();
    let half = halve(&[row0, row1].concat(), 4, 2);
    assert_eq!(half, [[64, 64, 64, 255], [64, 64, 64, 255]].concat());
    assert!(SLOT_W.is_multiple_of(1 << (MIPS - 1)));
    assert!(SLOT_H.is_multiple_of(1 << (MIPS - 1)));
    // スロットと同じ大きさの縦縞は、ミップレベル 0 では縞のまま、レベル 1 から灰色。
    let rgb: Vec<u8> = (0..SLOT_W * SLOT_H)
        .flat_map(|i| {
            [if (i % SLOT_W).is_multiple_of(2) {
                255
            } else {
                0
            }; 3]
        })
        .collect();
    let image = slot_image_from_rgb(SLOT_W, SLOT_H, &rgb).unwrap();
    assert_eq!(level_range(MIPS - 1).end, image.len());
    assert_eq!(
        &image[level_range(0)][..8],
        [255, 255, 255, 255, 0, 0, 0, 255]
    );
    assert!(image[level_range(1)].iter().all(|v| *v == 128 || *v == 255));
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
    let image = slot_image_from_rgb(w, h, &rgb).unwrap();
    assert_eq!(image.len(), slot_image_len());
    let px = |x: u32, y: u32| {
        let i = ((y * SLOT_W + x) * 4) as usize;
        [image[i], image[i + 1], image[i + 2], image[i + 3]]
    };
    assert_eq!(px(0, 0), [0, 255, 0, 255]);
    assert_eq!(px(SLOT_W - 1, SLOT_H - 1), [0, 255, 0, 255]);
}

#[test]
fn 縮めるときは範囲を平均する() {
    // 1 画素おきの白黒の縞は、間引くと全部白か全部黒になるが、平均すると灰色になる。
    let (w, h) = (256u32, 144u32);
    let rgb: Vec<u8> = (0..h)
        .flat_map(|_| (0..w).flat_map(|x| if x % 2 == 0 { [255; 3] } else { [0; 3] }))
        .collect();
    let image = slot_image_from_rgb(w, h, &rgb).unwrap();
    let v = i32::from(image[(10 * SLOT_W + 10) as usize * 4]);
    assert!((v - 128).abs() <= 2, "{v}");
}

#[test]
fn 大きさの合わない画素の並びは受けない() {
    assert!(slot_image_from_rgb(4, 4, &[0; 10]).is_none());
    assert!(slot_image_from_rgb(0, 4, &[]).is_none());
}
