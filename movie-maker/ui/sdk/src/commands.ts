import { invoke as tauriInvoke } from "@tauri-apps/api/core";
import { listen as tauriListen } from "@tauri-apps/api/event";
import type { MediaInvoke, MediaListen } from "./ipc.ts";

/** UI のコマンド 1 つ。アプリの UI のコマンドの表（`apps/movie-maker/src/win/commands.rs`）の 1 行と、今押せるか。 */
export interface UiCommand {
  /** `<組>.<動詞>`（`clip.split`）。Tasks の項目の `command` が指す。 */
  id: string;
  /** 画面の名前。 */
  name: string;
  /** キーの表の名前（`Ctrl+KeyZ`）。先頭が画面に出すキー。 */
  keys: string[];
  /** 先頭のキーの見た目（`Ctrl+Z`）。キーが無ければ null。 */
  key_label: string | null;
  /** ツールチップの 2 行目。名前だけで分かるコマンドは null。 */
  description: string | null;
  /** メニューバーでの場所。 */
  menu: string | null;
  /** 持ち主の WebView のパネルの id。アプリが実行するコマンドは null。 */
  panel: string | null;
  enabled: boolean;
  /** 入/切のあるコマンドのチェックの状態。それ以外は null。 */
  checked: boolean | null;
}

/** 表を読む Tauri のコマンドの名前。 */
export const IPC_COMMAND_TABLE = "command_table";
/** アプリが持ち主のコマンドを ID で実行する Tauri のコマンドの名前。 */
export const IPC_RUN_COMMAND = "run_command";
/** 押せるかが変わったときに新しい表を届ける Tauri のイベント名。 */
export const COMMANDS_CHANGED_EVENT = "mm:commands-changed";
/** 表に無いコマンド（実装の無い Tasks の項目）のツールチップ。 */
export const NOT_AVAILABLE = "Not available yet";

/** UI のコマンドの表を読んで実行する API。Shell が持ち、テストで差し替える。 */
export interface CommandsApi {
  table: () => Promise<UiCommand[]>;
  /** 失敗したら理由の文字を返す。 */
  run: (id: string) => Promise<string | null>;
  onChanged: (handler: (table: UiCommand[]) => void) => Promise<() => void>;
}

const errorText = (e: unknown): string => (e instanceof Error ? e.message : String(e));

/** Tauri の IPC で表を読み、コマンドを実行する API。 */
export const createIpcCommandsApi = (call: MediaInvoke = tauriInvoke, listen: MediaListen = tauriListen): CommandsApi => ({
  table: async () => (await call(IPC_COMMAND_TABLE)) as UiCommand[],
  run: async (id) => {
    try {
      await call(IPC_RUN_COMMAND, { id });
      return null;
    } catch (e) {
      return errorText(e);
    }
  },
  onChanged: (handler) => listen(COMMANDS_CHANGED_EVENT, (e) => handler(e.payload as UiCommand[])),
});

/** ボタンの `title` に出す文。1 行目は「名前 (キー)」、説明があれば 2 行目。表に無いコマンドは NOT_AVAILABLE。 */
export const commandTitle = (command: UiCommand | undefined): string => {
  if (!command) return NOT_AVAILABLE;
  const head = command.key_label ? `${command.name} (${command.key_label})` : command.name;
  return command.description ? `${head}\n${command.description}` : head;
};
