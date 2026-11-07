// 期待画像の絵。入力 0 を反転したものと入力 1 を、時刻で流れる縞で混ぜ、p0 で全体の明るさを決める。
float4 mm_main(float2 uv) {
    float4 a = mm_input(uv);
    float4 inverted = float4(a.a - a.rgb, a.a);
    float w = smoothstep(0.3, 0.7, 0.5 + 0.5 * sin((uv.x + uv.y - u.time * 0.25) * 12.0));
    return lerp(inverted, mm_input1(uv), w) * u.p0.x;
}
