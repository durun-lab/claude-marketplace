import type { DiagnosticsInfo, LayerInfo } from "./api.ts";

/** fps の小数の桁。29.97 のような値を、割り切れない分の桁を出さずに見せる。 */
const FPS_DIGITS = 3;

/** 描画スレッドが GPU を選ぶ前の GPU の欄。 */
export const NO_ADAPTER = "Not selected yet";

/** 時計の種類（アプリの `PlaybackInfo::clock`）。音の出力が無いときは壁時計で進める。 */
export const clockText = (clock: string): string => (clock === "audio" ? "Audio Output" : "System clock (no audio output)");

/** 再生位置を送ってから WebView が画面に出すまでの遅れ。 */
export const lagText = (ms: number): string => `${ms.toFixed(1)} ms`;

/** コンポジションの ID、大きさ、fps。 */
export const compositionText = (info: DiagnosticsInfo): string =>
  `${info.composition} ${info.width}x${info.height} ${Number(info.fps.toFixed(FPS_DIGITS))} fps`;

/** レイヤーの始まりと終わり（コンポジションの秒）。 */
export const rangeText = (layer: LayerInfo): string => `${layer.start.toFixed(1)}-${layer.end.toFixed(1)}s`;
