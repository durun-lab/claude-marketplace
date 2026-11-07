// トラックマット: 切り抜くレイヤーの画像に、同じ画素のマットの被覆率を掛ける。
// どちらも出力の大きさで、変形・切り抜き・マスク・不透明度を掛けた後の線形の光の premultiplied の画像。
// 大きさが同じなので標本化せず、画素をそのまま読む。

struct Matte {
    // 1 が輝度（無ければアルファ）、2 が反転。
    flags: u32,
    _pad0: u32,
    _pad1: u32,
    _pad2: u32,
}

@group(0) @binding(0) var<uniform> matte: Matte;
@group(0) @binding(1) var layer_tex: texture_2d<f32>;
@group(0) @binding(2) var matte_tex: texture_2d<f32>;

const FLAG_LUMA: u32 = 1u;
const FLAG_INVERTED: u32 = 2u;
// 作業色空間（Rec.709 の原色）の輝度の係数。Rust の `LUMA` を `{{LUMA}}` の位置に埋めてから読み込む。
const LUMA: vec3<f32> = vec3<f32>({{LUMA}});

// 画面を覆う 1 枚の三角形。
@vertex
fn vs_main(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    let uv = vec2<f32>(f32((index << 1u) & 2u), f32(index & 2u));
    return vec4<f32>(uv * 2.0 - 1.0, 0.0, 1.0);
}

// premultiplied のまま輝度を取ると、straight の輝度にアルファを掛けた値になる。
// HDR のハイライト（1 を超える値）は全部見せ、広色域の負の値は隠す側に切る。
fn coverage(m: vec4<f32>) -> f32 {
    var c = m.a;
    if ((matte.flags & FLAG_LUMA) != 0u) {
        c = dot(m.rgb, LUMA);
    }
    c = clamp(c, 0.0, 1.0);
    if ((matte.flags & FLAG_INVERTED) != 0u) {
        c = 1.0 - c;
    }
    return c;
}

@fragment
fn fs_main(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let p = vec2<i32>(position.xy);
    return textureLoad(layer_tex, p, 0) * coverage(textureLoad(matte_tex, p, 0));
}
