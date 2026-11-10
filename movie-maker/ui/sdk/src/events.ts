import { listen as tauriListen } from "@tauri-apps/api/event";
import type { MediaListen } from "./ipc.ts";

/** パネルが素材を選んだときに投げる DOM のイベント名。アプリの `web/transport.js` が受け、Source で開く。 */
export const ASSET_SELECTED_EVENT = "mm:asset-selected";

/** 直近の編集の結果の文言を知らせる Tauri のイベント名。アプリの `ipc.rs` と同じ。 */
export const EDIT_MESSAGE_EVENT = "mm:edit-message";

/** `mm:edit-message` の中身（アプリの `EditMessage`）。 */
export interface EditMessage {
  /** 失敗の理由か、取り消したもの。空なら伝えることが無い。 */
  text: string;
  /** 編集を断った理由なら真。 */
  failed: boolean;
  /** 文言を置くたびに進む。同じ文言が続いても、置き直したことが分かる。 */
  serial: number;
}

/** 素材を選んだことを知らせる（`detail.asset` が素材の ID）。 */
export const announceAsset = (asset: string): void => {
  window.dispatchEvent(new CustomEvent(ASSET_SELECTED_EVENT, { detail: { asset } }));
};

/** アプリが編集の結果の文言を置くたびに `handler` を呼ぶ。返り値で購読を止める。 */
export const onEditMessage = (
  handler: (message: EditMessage) => void,
  listen: MediaListen = tauriListen,
): Promise<() => void> => listen(EDIT_MESSAGE_EVENT, (e) => handler(e.payload as EditMessage));
