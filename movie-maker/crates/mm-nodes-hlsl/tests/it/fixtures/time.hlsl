// 時刻で右へ流れる縞。入力は使わない。
float4 mm_main(float2 uv) {
    float w = 0.5 + 0.5 * sin((uv.x - u.time * 0.25) * 18.0);
    float3 c = lerp(float3(0.02, 0.03, 0.1), float3(0.9, 0.5, 0.1), w);
    return float4(c * (0.5 + 0.5 * uv.y), 1.0);
}
