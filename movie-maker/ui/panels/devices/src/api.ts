import { invoke as tauriInvoke } from "@tauri-apps/api/core";
import type { MediaInvoke } from "@mm/sdk";

/** スマホから届くまでに足りないこと。 */
export type NoticeKind = "noTailscale" | "serveNotConfigured" | "noPwa";

export interface Notice {
  kind: NoticeKind;
  message: string;
}

/**
 * 人の待ち受けの状態（アプリの `devices_status`）。`unavailable` の `message` と `notices` の `message` は
 * アプリが画面に出す文言で、そのまま出す。
 */
export type MobileStatus =
  | { state: "starting" }
  | { state: "unavailable"; message: string }
  | {
      state: "running";
      port: number;
      /** モバイル版の URL。tailnet の名前が分からなければ `null`。 */
      url: string | null;
      /** `tailscale serve` の設定のコマンド。 */
      serveCommand: string;
      notices: Notice[];
    };

/** 登録した端末（アプリの `devices_list`。形は `POST /m/pair` の `device` と同じ）。 */
export interface Device {
  id: string;
  name: string;
  /** 登録した時の `Tailscale-User-Login`。 */
  login: string;
  paired_at_ms: number;
}

/** 登録コードと QR（アプリの `devices_issue_code`）。 */
export interface Pairing {
  /** 8 文字。 */
  code: string;
  expiresAtMs: number;
  /** スマホで開く URL。tailnet の名前が分からなければ `null`（登録コードを手で入れる）。 */
  url: string | null;
  /** URL の QR（SVG の本体）。 */
  qrSvg: string | null;
}

/** 端末の登録の画面が使うアプリの機能。コマンドは `apps/movie-maker/src/win/devices_ipc.rs`。 */
export interface DevicesApi {
  status: () => Promise<MobileStatus>;
  issueCode: () => Promise<Pairing>;
  list: () => Promise<Device[]>;
  /** 登録を外す。無い ID なら `false`。 */
  revoke: (id: string) => Promise<boolean>;
}

const asError = (e: unknown): Error => (e instanceof Error ? e : new Error(String(e)));

/** Tauri の IPC で Devices のコマンドを呼ぶ実装。 */
export const createIpcDevicesApi = (call: MediaInvoke = tauriInvoke): DevicesApi => {
  const run = async <T>(command: string, args?: Record<string, unknown>): Promise<T> => {
    try {
      return (await call(command, args)) as T;
    } catch (e) {
      throw asError(e);
    }
  };
  return {
    status: () => run("devices_status"),
    issueCode: () => run("devices_issue_code"),
    list: () => run("devices_list"),
    revoke: (id) => run("devices_revoke", { id }),
  };
};
