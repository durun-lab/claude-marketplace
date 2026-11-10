use super::*;

fn src(at: f64, src: f64, speed: f64) -> WaveSource {
    WaveSource {
        content_hash: "a".into(),
        at,
        src,
        speed,
    }
}

/// ピーク `peak` の見た目の振幅（平方根を 1 バイトに丸めたもの）。
fn seen(peak: f32) -> f32 {
    f32::from(shown(peak)) / FULL
}

/// 1 秒に 10 個、i 番目のピークが i / 100 のモノラル。
fn ramp(n: usize) -> Peaks {
    let v: Vec<f32> = (0..n).map(|i| i as f32 / 100.0).collect();
    Peaks::new(10.0, 1, &v)
}

/// 時刻 0 が左端で 1 秒 `px_per_sec` px の、横 `[x0, x1)` を覆う列。
fn grid(px_per_sec: f64, span: (f32, f32), step: f32) -> Grid {
    let time = TimeView {
        origin: 0.0,
        px_per_sec,
    };
    Grid::covering(&time, 0.0, span, step)
}

#[test]
fn コンポジションの時刻を波形の時刻にする_速さは波形の進み方() {
    let s = src(5.0, 2.0, 2.0);
    assert_eq!(s.src_at(5.0), 2.0);
    assert_eq!(s.src_at(6.0), 4.0, "2 倍速なら 1 秒で波形が 2 秒進む");
    assert_eq!(src(5.0, 2.0, 0.5).src_at(7.0), 3.0);
}

#[test]
fn チャンネルは最大にまとめ_符号は絶対値_見た目は平方根で持ち上げる() {
    let p = Peaks::new(10.0, 2, &[0.1, -0.4, 0.3, 0.2]);
    assert_eq!(p.max_in(0.0, 0.1), seen(0.4));
    assert_eq!(p.max_in(0.1, 0.2), seen(0.3));
    assert!((seen(0.25) - 0.5).abs() < 0.003, "0.25 の音は高さの半分");
}

#[test]
fn 範囲の最大は_素朴に取った最大以上で_全体の最大を超えない() {
    let p = ramp(100);
    for (from, to) in [(0.0, 10.0), (1.0, 3.7), (2.05, 2.06), (9.9, 10.0)] {
        let first = (from * 10.0_f64).floor() as usize;
        let last = ((to * 10.0_f64).ceil() as usize).clamp(first + 1, 100);
        let want = seen((last - 1) as f32 / 100.0);
        let got = p.max_in(from, to);
        assert!(
            got >= want && got <= seen(0.99),
            "{from}..{to}: {got} < {want}"
        );
    }
    assert_eq!(p.max_in(0.0, 10.0), seen(0.99));
    assert_eq!(p.max_in(0.0, 0.1), 0.0, "1 個の範囲は含まれる 1 個");
    assert_eq!(p.max_in(2.0, 2.05), seen(0.2));
}

#[test]
fn 範囲の外は_0() {
    let p = ramp(10);
    assert_eq!(p.max_in(5.0, 6.0), 0.0);
    assert_eq!(p.max_in(-3.0, -1.0), 0.0);
    assert_eq!(Peaks::new(10.0, 1, &[]).max_in(0.0, 1.0), 0.0);
}

#[test]
fn 段を全部足しても元のピークの数の_2_倍ほどに収まる() {
    // 1 時間のステレオ。ピークは 1 個 1 バイトなので、全部の段で約 720 KB。端数の切り上げで段ごとに 1 個まで増える。
    let p = Peaks::new(100.0, 2, &vec![0.5; 2 * 360_000]);
    let total: usize = p.levels.iter().map(Vec::len).sum();
    assert!(total <= 2 * 360_000 + p.levels.len(), "{total}");
}

#[test]
fn 縮小すると_1_列が受け持つ時間が長くなり_大きい音を落とさない() {
    let mut v = vec![0.0; 1000];
    v[437] = 0.9;
    let p = Peaks::new(10.0, 1, &v);
    // 1 秒 1 px（1 列 2 px = 2 秒）でも、100 秒の素材のどこかにある大きな音は列に出る。
    let cols = amplitudes(&p, &src(0.0, 0.0, 1.0), &grid(1.0, (0.0, 100.0), 2.0));
    assert_eq!(cols.len(), 50);
    let loud: Vec<usize> = (0..cols.len()).filter(|i| cols[*i] >= seen(0.9)).collect();
    assert!(loud.contains(&21), "43.7 秒は 42..44 秒の列（21 番目）");
    assert!(loud.len() <= 2, "段の境に揃えるので隣の列に少しはみ出す");
}

#[test]
fn 拡大すると_1_列がピーク_1_個より短くなり_同じピークが続く() {
    let p = ramp(100);
    // 1 秒 1000 px。1 個のピーク（0.1 秒）が 100 px で、1 列 10 px。
    let cols = amplitudes(&p, &src(0.0, 0.0, 1.0), &grid(1000.0, (0.0, 300.0), 10.0));
    assert_eq!(cols.len(), 30);
    assert_eq!(cols[1], cols[8], "最初のピーク（0 から 0.1 秒）の中");
    assert_ne!(cols[8], cols[11], "次のピークで変わる");
}

#[test]
fn 速さとイン点で波形の進みが変わる() {
    let p = ramp(100);
    // 1 秒 10 px、1 列 10 px（1 秒）。
    let g = grid(10.0, (0.0, 10.0), 10.0);
    let one = amplitudes(&p, &src(0.0, 0.0, 1.0), &g);
    let two = amplitudes(&p, &src(0.0, 0.0, 2.0), &g);
    let cut = amplitudes(&p, &src(0.0, 5.0, 1.0), &g);
    assert!(two[0] > one[0], "2 倍速は同じ 1 秒で素材の先まで進む");
    assert!(cut[0] > one[0], "イン点が先なら素材の途中から始まる");
}

#[test]
fn 列の境は時刻に固定し_横に動かしても同じ時間の列は同じ高さ() {
    let v: Vec<f32> = (0..1000).map(|i| ((i * 37) % 100) as f32 / 100.0).collect();
    let p = Peaks::new(10.0, 1, &v);
    let s = src(0.0, 0.0, 1.0);
    // 1 秒 3 px（1 列 2 px は 0.67 秒で、ピーク 6〜7 個をまとめる）。
    let time = |origin| TimeView {
        origin,
        px_per_sec: 3.0,
    };
    let a = Grid::covering(&time(0.0), 0.0, (0.0, 200.0), 2.0);
    let b = Grid::covering(&time(5.1234), 0.0, (0.0, 200.0), 2.0);
    assert!(b.x(0) <= 0.0 && b.x(1) > 0.0, "最初の列は左端を含む");
    let (ca, cb) = (amplitudes(&p, &s, &a), amplitudes(&p, &s, &b));
    let shift = (b.first - a.first) as usize;
    assert!(shift > 0);
    for i in 0..cb.len().min(ca.len() - shift) {
        assert_eq!(cb[i], ca[shift + i], "列 {i}");
    }
}

#[test]
fn 行の高さで波形の領域が変わる_small_は出さない() {
    let small = Rect::new(10.0, 20.0, 100.0, 15.0);
    assert_eq!(region(RowHeight::Small, small), None);
    let bar = Rect::new(10.0, 20.0, 100.0, 25.0);
    assert_eq!(
        region(RowHeight::Medium, bar),
        Some(Rect::new(10.0, 36.0, 100.0, MARK_PX)),
        "下端のマーカーの高さ"
    );
    let big = Rect::new(10.0, 20.0, 100.0, 53.0);
    assert_eq!(
        region(RowHeight::Large, big),
        Some(Rect::new(10.0, 47.0, 100.0, 26.0)),
        "帯の下半分"
    );
}

#[test]
fn 同じ入力なら矩形を作り直さない() {
    let w = Waves::default();
    let key = WaveKey {
        time: TimeView {
            origin: 0.0,
            px_per_sec: 100.0,
        },
        scroll_y: 0.0,
        area: Rect::new(0.0, 0.0, 10.0, 10.0),
        preview: None,
    };
    let mut built = 0;
    for _ in 0..3 {
        w.quads(key, || {
            built += 1;
            Vec::new()
        });
    }
    assert_eq!(built, 1);
    let zoomed = WaveKey {
        time: TimeView {
            px_per_sec: 200.0,
            ..key.time
        },
        ..key
    };
    w.quads(zoomed, || {
        built += 1;
        Vec::new()
    });
    assert_eq!(built, 2, "倍率が変われば作り直す");
}
