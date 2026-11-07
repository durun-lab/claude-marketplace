//! 利用者の HLSL に前置きと入口を足して、DXC に渡す全文を作る。
//!
//! 名前と束ね方は `shader.wgsl` に揃える。`#line` で利用者の源の行を 1 から数え直すので、DXC の誤りは利用者の行で出る。

/// 入口の部分に付ける源の名前。利用者の源と見分ける。
pub(crate) const ENTRY_LABEL: &str = "mm-entry";
/// 入力のテクスチャの数の上限。`mm_input`、`mm_input1`。
pub const MAX_INPUTS: usize = 2;

/// uniform（binding 0）とサンプラー。レジスタの番号は README の約束。
const PRELUDE: &str = "struct MmU {
    float2 size;
    float time;
    float scale;
    float4 p0;
    float4 p1;
};
ConstantBuffer<MmU> u : register(b0);
SamplerState mm_samp : register(s0);
";

/// 利用者の関数の宣言と入口。入口は前置きの後ろに置くので、宣言を先に出す。
const DECLARE_MAIN: &str = "float4 mm_main(float2 uv);\n";
const ENTRY: &str =
    "float4 mm_fs(float2 uv : TEXCOORD0) : SV_Target {\n    return mm_main(uv);\n}\n";

/// 入力 `index` を引く関数。繋がっていなければ透明を返す。
fn input_fn(index: usize, connected: bool) -> String {
    let name = if index == 0 {
        "mm_input".to_string()
    } else {
        format!("mm_input{index}")
    };
    if connected {
        format!(
            "Texture2D<float4> mm_t{index} : register(t{index});\n\
             float4 {name}(float2 uv) {{ return mm_t{index}.SampleLevel(mm_samp, uv, 0); }}\n"
        )
    } else {
        format!("float4 {name}(float2 uv) {{ return 0; }}\n")
    }
}

/// `#line` に書ける名前。引用符と改行を落とす。
pub(crate) fn line_label(label: &str) -> String {
    label
        .chars()
        .filter(|c| !matches!(c, '"' | '\\' | '\n' | '\r'))
        .collect()
}

/// DXC に渡す全文。`inputs` は繋がっている入力の数（`MAX_INPUTS` まで）。
pub(crate) fn full_source(code: &str, label: &str, inputs: usize) -> String {
    let mut s = String::from(PRELUDE);
    for i in 0..MAX_INPUTS {
        s += &input_fn(i, i < inputs);
    }
    s += DECLARE_MAIN;
    s += &format!("#line 1 \"{}\"\n", line_label(label));
    s += code;
    if !code.ends_with('\n') {
        s.push('\n');
    }
    s += &format!("#line 1 \"{ENTRY_LABEL}\"\n{ENTRY}");
    s
}
