import { invoke as tauriInvoke } from "@tauri-apps/api/core";
import type { KindInfo, KindLookup } from "./kinds.ts";

/** Tauri の invoke と同じ形の関数。テストで差し替える。 */
export type KindsInvoke = (command: string, args?: Record<string, unknown>) => Promise<unknown>;

/** アプリの `inspector_kind`（名前と引数は ui/sdk/README.md）でパラメータの定義を引く。失敗は reject。 */
export const createIpcKindLookup =
  (call: KindsInvoke = tauriInvoke): KindLookup =>
  async (kind, params) => {
    const info = (await call("inspector_kind", { kind, params })) as KindInfo;
    return info.params.params;
  };
