// 2 つの入力を p0 の割合で混ぜる。
float4 mm_main(float2 uv) {
    return lerp(mm_input(uv), mm_input1(uv), u.p0.x);
}
