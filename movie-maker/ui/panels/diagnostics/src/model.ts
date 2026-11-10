import type { DiagnosticsInfo, LayerInfo } from "./api.ts";

/** fps の小数の桁。29.97 のような値を、割り切れない分の桁を出さずに見せる。 */
const FPS_DIGITS = 3;

/** レイヤーの範囲の小数の桁。小数 1 桁では 1 フレームのレイヤーが「0.0-0.0s」と長さ 0 に見えるので、ミリ秒まで出す。 */
const RANGE_DIGITS = 3;

/** 描画スレッドが GPU を選ぶ前の GPU の行。 */
export const NO_ADAPTER = "Not selected yet";

/** 時計の種類（アプリの `PlaybackInfo::clock`）。音の出力が無いときは壁時計で進める。 */
export const clockText = (clock: string): string => (clock === "audio" ? "Audio Output" : "System clock (no audio output)");

/** 再生位置を送ってから WebView が画面に出すまでの遅れ。 */
export const lagText = (ms: number): string => `${ms.toFixed(1)} ms`;

/** コンポジションの ID、大きさ、fps。 */
export const compositionText = (info: DiagnosticsInfo): string =>
  `${info.composition} ${info.width}x${info.height} ${Number(info.fps.toFixed(FPS_DIGITS))} fps`;

/** レイヤーの始まりと終わり（コンポジションの秒）。 */
export const rangeText = (layer: LayerInfo): string =>
  `${layer.start.toFixed(RANGE_DIGITS)}-${layer.end.toFixed(RANGE_DIGITS)}s`;
