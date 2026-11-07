// 入力の色を反転する。premultiplied なので、反転した色に alpha を掛けた値は alpha - rgb。
float4 mm_main(float2 uv) {
    float4 c = mm_input(uv);
    return float4(c.a - c.rgb, c.a);
}
