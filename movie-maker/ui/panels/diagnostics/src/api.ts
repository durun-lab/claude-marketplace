import { invoke as tauriInvoke } from "@tauri-apps/api/core";
import type { MediaInvoke } from "@mm/sdk";

/** レイヤー 1 本の時間の範囲（アプリの `LayerInfo`）。上のレイヤーが先頭。 */
export interface LayerInfo {
  id: string;
  kind: string;
  /** コンポジションの秒。 */
  start: number;
  end: number;
}

/** 開いたプロジェクトと、再生と描画が選んだもの（アプリの `project_info` の `ProjectInfo`）。 */
export interface DiagnosticsInfo {
  name: string;
  composition: string;
  width: number;
  height: number;
  fps: number;
  /** 秒。 */
  duration: number;
  /** 時計の種類（`audio`、`wall`）。 */
  clock: string;
  /** 音の出力デバイスの名前とレート。無ければ null。 */
  audio_device: string | null;
  /** 選ばれた実装の名前（`render`、`decode` などと、その実装）。 */
  backends: [string, string][];
  layers: LayerInfo[];
  /** 描画スレッドが選んだ GPU。選ぶ前は空。 */
  adapter: string;
  /** 再生位置を送ってから WebView が画面に出すまでの直近の中央値（ミリ秒）。 */
  lag_ms: number;
}

/** Diagnostics が使うアプリの機能。コマンドは `apps/movie-maker/src/win/ipc.rs`。 */
export interface DiagnosticsApi {
  info: () => Promise<DiagnosticsInfo>;
}

const asError = (e: unknown): Error => (e instanceof Error ? e : new Error(String(e)));

/** Tauri の IPC で `project_info` を呼ぶ実装。 */
export const createIpcDiagnosticsApi = (call: MediaInvoke = tauriInvoke): DiagnosticsApi => ({
  info: async () => {
    try {
      return (await call("project_info")) as DiagnosticsInfo;
    } catch (e) {
      throw asError(e);
    }
  },
});
