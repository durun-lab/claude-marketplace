use super::*;

fn default_rects(w: u32, h: u32, scale: f64) -> (PixelRect, PixelRect) {
    split_rects(w, h, scale, &Workspace::default())
}

#[test]
fn サーフェスと_webview_は隙間も重なりもなくウィンドウを埋める() {
    for (w, h, scale) in [
        (1600, 1000, 1.0),
        (1125, 750, 1.0),
        (2400, 1350, 1.5),
        (1, 1, 1.0),
    ] {
        let (s, v) = default_rects(w, h, scale);
        assert_eq!(
            s.0 + s.2,
            v.0,
            "サーフェスの右端と WebView の左端が一致する"
        );
        assert_eq!(v.0 + v.2, w as i32);
        assert!(s.2 >= 0 && v.2 >= 0, "{w}x{h}: {s:?} {v:?}");
        assert_eq!((s.1, v.1), (0, 0), "どちらもメニューバーのすぐ下から");
        assert_eq!((s.3, v.3), (h as i32, h as i32));
    }
}

#[test]
fn 既定の割り付けで_webview_は_2_列の幅の和を取り_サーフェスがウィンドウの残りを取る() {
    assert_eq!(theme::webview_width(), 660.0);
    // 既定のウィンドウ（幅 1600）でサーフェスは 940（約 59%）、幅 1920 で 1260（約 66%）。
    assert_eq!(
        default_rects(1600, 900, 1.0),
        ((0, 0, 940, 900), (940, 0, 660, 900))
    );
    assert_eq!(default_rects(1920, 1080, 1.0).0, (0, 0, 1260, 1080));
    // 表示の倍率が 150% なら、同じ論理ピクセルの幅を物理ピクセルで取る。
    assert_eq!(
        default_rects(2400, 1350, 1.5),
        ((0, 0, 1410, 1350), (1410, 0, 990, 1350))
    );
}

#[test]
fn 仕切りで広げた_webview_も_倍率に依らず同じ論理ピクセルの幅() {
    let mut ws = Workspace::default();
    ws.drag_web(900.0, 1600.0);
    assert_eq!(split_rects(1600, 900, 1.0, &ws).1, (700, 0, 900, 900));
    assert_eq!(split_rects(2400, 1350, 1.5, &ws).1, (1050, 0, 1350, 1350));
}

#[test]
fn 列を閉じた分だけサーフェスが広がり_全部閉じたら_webview_は幅を持たない() {
    let mut ws = Workspace::default();
    ws.left_open = false;
    assert_eq!(split_rects(1600, 900, 1.0, &ws).0 .2, 1200);
    ws.right_open = false;
    assert_eq!(
        split_rects(1600, 900, 1.0, &ws),
        ((0, 0, 1600, 900), (1600, 0, 0, 900))
    );
}

#[test]
fn ウィンドウの幅の下限でもサーフェスは下限の幅を守る() {
    let min = theme::webview_width() + MIN_SURFACE_W + f64::from(mm_ui_layout::SPLITTER_W);
    let (s, _) = default_rects(min as u32, 800, 1.0);
    assert!(f64::from(s.2) >= MIN_SURFACE_W, "{s:?}");
    // 保存した幅が広くても、狭いウィンドウでは WebView を縮める。
    let mut ws = Workspace::default();
    ws.drag_web(1200.0, 2400.0);
    let (s, _) = split_rects(min as u32, 800, 1.0, &ws);
    assert!(f64::from(s.2) >= MIN_SURFACE_W, "{s:?}");
}

#[test]
fn ウィンドウが_2_列より狭くても矩形は負にならない() {
    let (s, v) = default_rects(500, 400, 1.0);
    assert!(s.2 >= 0 && v.2 >= 0 && s.0 + s.2 == v.0, "{s:?} {v:?}");
}

#[test]
fn ウィンドウの幅の下限は_2_列とサーフェスの下限と仕切りの帯の和で_高さは決めない() {
    let c = size_constraints();
    assert_eq!(
        c.min_width,
        Some(PixelUnit::from(LogicalUnit::new(
            theme::webview_width() + MIN_SURFACE_W + 4.0
        )))
    );
    assert_eq!(c.min_height, None);
    assert!(INITIAL_SIZE.0 >= theme::webview_width() + MIN_SURFACE_W + 4.0);
}

#[test]
fn シェルに渡す見た目は右の列の幅と列の開け閉め() {
    let mut ws = Workspace::default();
    let s = shell_script(&ws, 1600.0);
    assert!(s.contains("--mm-right-w','400px'"), "{s}");
    assert!(s.contains("mmLeft='open'") && s.contains("mmRight='open'"), "{s}");
    ws.left_open = false;
    assert!(shell_script(&ws, 1600.0).contains("mmLeft='closed'"));
    // 左の列が下限まで縮んだ後は、右の列が縮む。
    ws.left_open = true;
    ws.drag_web(520.0, 1600.0);
    assert!(shell_script(&ws, 1600.0).contains("--mm-right-w','320px'"));
}

#[test]
fn 中継スクリプトがボタンから送らない編集のキーはキーの表の編集のキーと同じ() {
    let start = TRANSPORT_JS
        .find("const EDIT_KEYS = new Set([")
        .expect("中継スクリプトに EDIT_KEYS がある");
    let body = &TRANSPORT_JS[start..];
    let list = &body[..body.find("]);").expect("EDIT_KEYS の終わり")];
    let mut bridge: Vec<&str> = list.split('"').skip(1).step_by(2).collect();
    let mut table: Vec<&str> = commands::TABLE
        .iter()
        .filter(|s| {
            ["clip.", "edit.", "source."]
                .iter()
                .any(|p| s.id.starts_with(p))
        })
        .flat_map(|s| crate::win::keys::keys_of(s.id))
        .collect();
    bridge.sort_unstable();
    table.sort_unstable();
    assert_eq!(bridge, table);
}
