//! `Ui` をオフスクリーンのテクスチャに描いて、画素をリードバックする。GPU が無ければスキップする（`MM_REQUIRE_GPU` があれば失敗）。

use mm_types::{Rgba, RgbaImage, Size};
use mm_ui_layout::{
    fit_rect, frame_point, palette, DrawingShape, DrawingShapeKind, Item, ItemKind, PointerEvent,
    RowHeight, Rows, TimelineModel, HANDLE_PX, HEADER_W, VIEW_MARGIN,
};
use mm_ui_wgpu::{Region, Ui};

use crate::support::{bar_rect, close, gpu, hex, px_in, read_back, render_sized, FORMAT, H, W};

fn model() -> TimelineModel {
    TimelineModel {
        duration: 10.0,
        fps: 30.0,
        items: vec![Item::layer(0.0, 10.0, ItemKind::Footage, true)],
        ..TimelineModel::default()
    }
}

/// 描いた結果を RGBA8 で返す（行は詰めてある）。
fn render(device: &wgpu::Device, queue: &wgpu::Queue, ui: &mut Ui) -> Vec<u8> {
    render_sized(device, queue, ui, (W, H))
}

fn px(img: &[u8], x: u32, y: u32) -> [u8; 3] {
    px_in(img, W, x, y)
}

#[test]
fn program_は線形の画像を_rec1886_の表示の値で出す() {
    let Some((device, queue)) = gpu() else { return };
    let mut ui = Ui::new(&device, FORMAT);
    ui.set_model(model());
    ui.resize(W, H);
    // 線形 0.5 は Rec.1886 で 0.5^(1/2.4) = 0.749 → 191。
    let img = RgbaImage::filled(Size::new(16, 9), Rgba::new(0.5, 0.5, 0.5, 1.0));
    ui.set_frame(&device, &queue, &img);
    let out = render(&device, &queue, &mut ui);
    let p = ui.layout().program;
    let centre = px(&out, (p.x + p.w / 2.0) as u32, (p.y + p.h / 2.0) as u32);
    assert!(close(centre, [191, 191, 191], 2), "{centre:?}");
}

/// HDR の素材の上限を渡すと、1 を超えるハイライトを白に潰さず SDR の表示に収める。上限 1 なら今までどおり。
#[test]
fn program_は_hdr_の上限を渡すとハイライトを_bt2390_で収める() {
    let Some((device, queue)) = gpu() else { return };
    let mut ui = Ui::new(&device, FORMAT);
    ui.set_model(model());
    ui.resize(W, H);
    let shade = |ui: &mut Ui, v: f32| {
        let img = RgbaImage::filled(Size::new(16, 9), Rgba::new(v, v, v, 1.0));
        ui.set_frame(&device, &queue, &img);
        let out = render(&device, &queue, ui);
        let p = ui.layout().program;
        px(&out, (p.x + p.w / 2.0) as u32, (p.y + p.h / 2.0) as u32)[0]
    };
    assert_eq!(shade(&mut ui, 2.0), 255, "上限 1 では 1 を超える値は白");
    assert!(
        close([shade(&mut ui, 0.5); 3], [191; 3], 2),
        "上限 1 の SDR は恒等"
    );
    ui.set_hdr_peak(1000.0 / 203.0);
    let (mid, high, peak) = (
        shade(&mut ui, 1.0),
        shade(&mut ui, 3.0),
        shade(&mut ui, 4.9),
    );
    assert!(
        mid < high && high < peak,
        "ハイライトの階調が残る: {mid} {high} {peak}"
    );
    assert!(peak >= 250, "素材の上限は白の付近: {peak}");
    assert!(
        close(
            [shade(&mut ui, 0.05); 3],
            [mm_types::transfer::linear_to_code8(0.05); 3],
            2
        ),
        "暗部は変えない"
    );
}

/// 1 色で塗った Rgba16Float のテクスチャ（レンダラーの出力の形）。
fn filled_texture(device: &wgpu::Device, queue: &wgpu::Queue, value: f64) -> wgpu::Texture {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: None,
        size: wgpu::Extent3d {
            width: 16,
            height: 9,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba16Float,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = texture.create_view(&Default::default());
    let mut enc = device.create_command_encoder(&Default::default());
    enc.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: None,
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view: &view,
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(wgpu::Color {
                    r: value,
                    g: value,
                    b: value,
                    a: 1.0,
                }),
                store: wgpu::StoreOp::Store,
            },
        })],
        ..Default::default()
    });
    queue.submit([enc.finish()]);
    texture
}

#[test]
fn program_は同じデバイスのテクスチャをそのまま出す() {
    let Some((device, queue)) = gpu() else { return };
    let mut ui = Ui::new(&device, FORMAT);
    ui.set_model(model());
    ui.resize(W, H);
    let texture = filled_texture(&device, &queue, 0.5);
    ui.set_frame_texture(&device, &texture);
    let out = render(&device, &queue, &mut ui);
    let p = ui.layout().program;
    let centre = px(&out, (p.x + p.w / 2.0) as u32, (p.y + p.h / 2.0) as u32);
    assert!(close(centre, [191, 191, 191], 2), "{centre:?}");
    // 借用したテクスチャの後に CPU の画像が来たら、自前のテクスチャに上げ直す（借用したものには書かない）。
    let img = RgbaImage::filled(Size::new(16, 9), Rgba::new(1.0, 1.0, 1.0, 1.0));
    ui.set_frame(&device, &queue, &img);
    let out = render(&device, &queue, &mut ui);
    let centre = px(&out, (p.x + p.w / 2.0) as u32, (p.y + p.h / 2.0) as u32);
    assert!(close(centre, [255, 255, 255], 2), "{centre:?}");
}

#[test]
fn source_のフレームが来ると_program_の左に_source_の領域が現れ_閉じると消える() {
    let Some((device, queue)) = gpu() else { return };
    let mut ui = Ui::new(&device, FORMAT);
    ui.set_model(model());
    ui.resize(W, H);
    assert_eq!(ui.layout().source, None);
    let program = RgbaImage::filled(Size::new(16, 9), Rgba::new(0.5, 0.5, 0.5, 1.0));
    ui.set_frame(&device, &queue, &program);
    let full = ui.layout().program;

    let source = RgbaImage::filled(Size::new(16, 9), Rgba::new(0.0, 0.5, 0.0, 1.0));
    ui.set_source_frame(&device, &queue, &source);
    let l = ui.layout();
    let area = l.source.expect("Source の領域");
    assert!(area.right() <= l.program.x && l.program.w < full.w);
    assert_eq!(ui.region_at(area.x + 1.0, area.y + 1.0), Region::Source);
    assert_eq!(ui.region_at(l.program.x + 1.0, 1.0), Region::Program);
    assert_eq!(ui.region_at(1.0, l.timeline.y + 1.0), Region::Timeline);

    let out = render(&device, &queue, &mut ui);
    let at = |r: mm_ui_layout::Rect| px(&out, (r.x + r.w / 2.0) as u32, (r.y + r.h / 2.0) as u32);
    // 緑 0.5 は Rec.1886 で 191。Program の灰色 0.5 は 191 が 3 成分。
    assert!(close(at(area), [0, 191, 0], 2), "Source {:?}", at(area));
    assert!(
        close(at(l.program), [191, 191, 191], 2),
        "Program {:?}",
        at(l.program)
    );

    ui.close_source();
    assert_eq!(ui.layout().source, None);
    assert_eq!(ui.layout().program, full, "Program が元の広さに戻る");
}

#[test]
fn 画像の縦横比を保ち_余白は背景のまま() {
    let Some((device, queue)) = gpu() else { return };
    let mut ui = Ui::new(&device, FORMAT);
    ui.set_model(model());
    ui.resize(W, H);
    let img = RgbaImage::filled(Size::new(16, 9), Rgba::new(1.0, 1.0, 1.0, 1.0));
    ui.set_frame(&device, &queue, &img);
    let out = render(&device, &queue, &mut ui);
    let p = ui.layout().program;
    // Program の領域の左端は 16:9 の画像より横長なので余白。
    let edge = px(&out, 2, (p.y + p.h / 2.0) as u32);
    assert!(close(edge, hex(palette::PROGRAM_BACKDROP), 2), "{edge:?}");
}

#[test]
fn 再生位置の線が時刻の位置に立つ() {
    let Some((device, queue)) = gpu() else { return };
    let mut ui = Ui::new(&device, FORMAT);
    ui.set_model(model());
    ui.resize(W, H);
    ui.set_playhead(5.0);
    let out = render(&device, &queue, &mut ui);
    let t = ui.layout().timeline;
    // 全体表示は 10 秒が余白 16 px を除いた幅に収まる。5 秒はその中央。
    let x = (t.x + HEADER_W + 16.0 + (t.w - HEADER_W - 32.0) / 2.0) as u32;
    let y = (t.y + t.h - 3.0) as u32;
    let found =
        (x.saturating_sub(2)..=x + 2).any(|xx| close(px(&out, xx, y), hex(palette::PLAYHEAD), 3));
    assert!(found, "x={x} の近くに再生位置の線");
}

fn point_shape(u: f32, v: f32) -> DrawingShape {
    DrawingShape {
        kind: DrawingShapeKind::Point,
        points: vec![[u, v]],
    }
}

/// 描き込みはサーフェスにだけ重ね、レンダラーから借りたフレームのテクスチャ（キャッシュと書き出しの元）には書かない。
#[test]
fn 描き込みは映像の矩形に重ね_フレームのテクスチャには書かない() {
    let Some((device, queue)) = gpu() else { return };
    let mut ui = Ui::new(&device, FORMAT);
    ui.set_model(model());
    ui.resize(W, H);
    let texture = filled_texture(&device, &queue, 0.0);
    let before = read_back(&device, &queue, &texture, 8);
    ui.set_frame_texture(&device, &texture);
    ui.set_drawing(vec![point_shape(0.25, 0.5)]);
    let out = render(&device, &queue, &mut ui);

    let frame = fit_rect(16.0, 9.0, ui.layout().program);
    let [x, y] = frame_point(frame, [0.25, 0.5]);
    let at = px(&out, x as u32, y as u32);
    assert!(close(at, hex(palette::DRAWING), 3), "描き込みの点 {at:?}");
    let [fx, fy] = frame_point(frame, [0.75, 0.5]);
    let outside = px(&out, fx as u32, fy as u32);
    assert!(
        close(outside, [0, 0, 0], 2),
        "点の外は映像のまま {outside:?}"
    );
    assert_eq!(
        read_back(&device, &queue, &texture, 8),
        before,
        "フレームのテクスチャは変わらない"
    );

    ui.set_drawing(Vec::new());
    let cleared = render(&device, &queue, &mut ui);
    let after = px(&cleared, x as u32, y as u32);
    assert!(close(after, [0, 0, 0], 2), "空で消える {after:?}");
}

#[test]
fn 描き込みはレターボックスの余白でなく映像の縁に合わせる() {
    let Some((device, queue)) = gpu() else { return };
    let mut ui = Ui::new(&device, FORMAT);
    ui.set_model(model());
    ui.resize(W, H);
    // 縦長の映像は Program の領域の左右に余白ができる。
    let img = RgbaImage::filled(Size::new(9, 16), Rgba::new(0.0, 0.0, 0.0, 1.0));
    ui.set_frame(&device, &queue, &img);
    ui.set_drawing(vec![point_shape(0.0, 0.5)]);
    let out = render(&device, &queue, &mut ui);
    let area = ui.layout().program;
    let frame = fit_rect(9.0, 16.0, area);
    assert!(frame.x > area.x + 20.0, "左に余白がある");
    let y = (frame.y + frame.h / 2.0) as u32;
    let edge = px(&out, (frame.x + 1.0) as u32, y);
    assert!(close(edge, hex(palette::DRAWING), 3), "映像の左端 {edge:?}");
    let margin = px(&out, (area.x + 2.0) as u32, y);
    assert!(
        close(margin, hex(palette::PROGRAM_BACKDROP), 2),
        "余白 {margin:?}"
    );
}

/// 頂点は条件が同じ間は作り直さないので、表示の倍率とウィンドウの大きさを変えたら描き直されることを確かめる。
#[test]
fn 描き込みは表示の倍率とウィンドウの大きさが変わると描き直す() {
    let Some((device, queue)) = gpu() else { return };
    let mut ui = Ui::new(&device, FORMAT);
    ui.set_model(model());
    ui.resize(W, H);
    let img = RgbaImage::filled(Size::new(16, 9), Rgba::new(0.0, 0.0, 0.0, 1.0));
    ui.set_frame(&device, &queue, &img);
    ui.set_drawing(vec![point_shape(0.5, 0.5)]);
    let first = render(&device, &queue, &mut ui);
    let centre = |ui: &Ui| frame_point(fit_rect(16.0, 9.0, ui.layout().program), [0.5, 0.5]);
    let [x, y] = centre(&ui);
    // 点の中心から 8 画素の所は、倍率 1（直径 12）では点の外、倍率 2（直径 24）では点の中。
    let base = px(&first, x as u32 + 8, y as u32);
    assert!(close(base, [0, 0, 0], 2), "倍率 1 では点の外 {base:?}");
    ui.set_scale_factor(2.0);
    let hidpi = render(&device, &queue, &mut ui);
    // 表示の倍率が変わると Timeline の物理ピクセルの高さも変わり、Program の領域が動く。
    let [x, y] = centre(&ui);
    let wide = px(&hidpi, x as u32 + 8, y as u32);
    assert!(
        close(wide, hex(palette::DRAWING), 3),
        "倍率 2 では点の中 {wide:?}"
    );

    // 再生の操作のバーの分だけ映像の領域の高さが減るので、映像が残る大きさにする。
    let small = (W * 3 / 4, H * 3 / 4);
    ui.resize(small.0, small.1);
    let out = render_sized(&device, &queue, &mut ui, small);
    let [sx, sy] = frame_point(fit_rect(16.0, 9.0, ui.layout().program), [0.5, 0.5]);
    let at = px_in(&out, small.0, sx as u32, sy as u32);
    assert!(
        close(at, hex(palette::DRAWING), 3),
        "小さくしても映像の中央 {at:?}"
    );
}

/// 走査線の上で、`hit` に当てはまる画素が続く最も長い数。
fn longest_run(points: impl Iterator<Item = [u8; 3]>, hit: impl Fn([u8; 3]) -> bool) -> u32 {
    let (mut best, mut run) = (0, 0);
    for p in points {
        run = if hit(p) { run + 1 } else { 0 };
        best = best.max(run);
    }
    best
}

/// 帯の高さとつまみの幅は論理ピクセルで決まり、表示の倍率 1.5 と 2 でも同じ見た目の大きさになる。
/// つまみは物理ピクセルのポインターで光らせるので、当たり判定も同じ表示の倍率で換算されていることを確かめる。
#[test]
fn timeline_の帯とつまみは表示の倍率を掛けても論理ピクセルの寸法を保つ() {
    let Some((device, queue)) = gpu() else { return };
    let bar = hex(palette::lighten(
        palette::item_color(ItemKind::Footage),
        0.18,
    ));
    for scale in [1.0_f32, 1.5, 2.0] {
        let size = ((W as f32 * scale) as u32, (H as f32 * scale) as u32);
        let mut ui = Ui::new(&device, FORMAT);
        ui.set_scale_factor(scale);
        ui.set_model(model());
        ui.resize(size.0, size.1);
        let t = ui.layout().timeline;
        // 全体表示の帯の右端は右の余白（論理）の内側。その 2 論理ピクセル内側でつまみに乗る。
        let end = t.right() - VIEW_MARGIN * scale;
        let rect = bar_rect(&ui, &model(), scale, 0, (0.0, 10.0));
        let row_y = rect.y + rect.h / 2.0;
        ui.pointer(PointerEvent::Move {
            x: end - 2.0 * scale,
            y: row_y,
        });
        let out = render_sized(&device, &queue, &mut ui, size);
        let at = |x: u32, y: u32| px_in(&out, size.0, x, y);
        let mid_x = (t.x + t.w / 2.0) as u32;
        let column = (t.y as u32..t.bottom() as u32).map(|y| at(mid_x, y));
        let height = longest_run(column, |p| close(p, bar, 3));
        let row = (t.x as u32..t.right() as u32).map(|x| at(x, row_y as u32));
        let handle = longest_run(row, |p| p.iter().all(|c| *c >= 200));
        let bar_h = Rows::new([RowHeight::Medium]).bar(0).1;
        for (name, got, logical) in [
            ("帯の高さ", height, bar_h),
            ("つまみの幅", handle, HANDLE_PX),
        ] {
            let want = logical * scale;
            assert!(
                (got as f32 - want).abs() <= 1.0,
                "表示の倍率 {scale}: {name} {got} 物理ピクセル（期待 {want}）"
            );
        }
    }
}
