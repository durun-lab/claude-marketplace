## HDR

| 用語 | 定義 | 識別子 |
|---|---|---|
| HDR の基準の白 | 作業色空間の線形の 1.0 に置く明るさ。BT.2408 の 203 cd/m²。SDR の白と HDR の素材の白をここで揃え、1 を超える値をハイライトとして持ち回る | `transfer::HDR_REFERENCE_WHITE_NITS` |
| PQ | SMPTE ST 2084 の伝達関数。符号化値 1.0 が 10000 cd/m² | `bt2100_pq`、`transfer::pq_to_linear` |
| HLG | ARIB STD-B67（BT.2100）の伝達関数。表示の上限 1000 cd/m² の OOTF をチャンネルごとに掛けて線形にする | `bt2100_hlg`、`transfer::hlg_to_linear` |
| シェーパー | 3D LUT を引く前に入力に掛ける 1D の曲線。線形の光を PQ で 0..1 の格子に収め、1 を超える値を切らない | `LutShaper`、`TransformProgram::Lut3d.shaper` |
| 書き出しの信号 | 書き出す映像の原色と伝達関数（SDR の Rec.709、HDR の Rec.2020 の PQ と HLG） | `OutputSignal`、`hevc_pq`、`hevc_hlg` |
| プレビューのトーンマップ | HDR の線形を SDR の表示に収める BT.2390 の EETF。素材の上限が 1 なら恒等 | `tonemap::bt2390_to_sdr`、`Ui::set_hdr_peak` |
