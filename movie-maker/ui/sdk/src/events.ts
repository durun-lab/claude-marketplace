import { listen as tauriListen } from "@tauri-apps/api/event";
import type { MediaListen } from "./ipc.ts";

/** パネルが素材を選んだときに投げる DOM のイベント名。アプリの `web/transport.js` が受け、Source で開く。 */
export const ASSET_SELECTED_EVENT = "mm:asset-selected";

/** Source の状態が変わったことを知らせる Tauri のイベント名。アプリの `source_ipc.rs` と同じ。 */
export const SOURCE_EVENT = "mm:source";

/** 素材を選んだことを知らせる（`detail.asset` が素材の ID）。 */
export const announceAsset = (asset: string): void => {
  window.dispatchEvent(new CustomEvent(ASSET_SELECTED_EVENT, { detail: { asset } }));
};

/**
 * 直近の編集の結果の文言（失敗の理由、取り消したもの）を受けるたびに `handler` を呼ぶ。空なら伝えることが無い。
 * 文言は Source の状態（`mm:source`）が運ぶ。返り値で購読を止める。
 */
export const onEditMessage = (handler: (message: string) => void, listen: MediaListen = tauriListen): Promise<() => void> =>
  listen(SOURCE_EVENT, (e) => handler((e.payload as { message?: string }).message ?? ""));
