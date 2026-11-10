import { describe, expect, it, vi } from "vitest";
import {
  COMMANDS_CHANGED_EVENT,
  commandTitle,
  createIpcCommandsApi,
  IPC_COMMAND_TABLE,
  IPC_RUN_COMMAND,
  NOT_AVAILABLE,
  PANEL_COMMAND_EVENT,
  type UiCommand,
} from "./commands.ts";

const lift: UiCommand = {
  id: "clip.lift",
  name: "Lift",
  keys: ["Delete"],
  key_label: "Delete",
  description: "Remove the selected clip and leave a gap",
  menu: "edit",
  menu_order: 2,
  panel: null,
  enabled: false,
  checked: null,
};

describe("commandTitle", () => {
  it("名前とキーを 1 行目、説明を 2 行目に出す", () => {
    expect(commandTitle(lift)).toBe("Lift (Delete)\nRemove the selected clip and leave a gap");
  });

  it("キーと説明の無いコマンドは名前だけ", () => {
    expect(commandTitle({ ...lift, key_label: null, description: null, name: "Import Media…" })).toBe("Import Media…");
  });

  it("表に無いコマンドは Not available yet", () => {
    expect(commandTitle(undefined)).toBe(NOT_AVAILABLE);
    expect(NOT_AVAILABLE).toBe("Not available yet");
  });
});

describe("createIpcCommandsApi", () => {
  it("表を command_table で読み、コマンドを run_command で実行する", async () => {
    const call = vi.fn().mockResolvedValueOnce([lift]).mockResolvedValueOnce(undefined);
    const api = createIpcCommandsApi(call, vi.fn());
    expect(await api.table()).toEqual([lift]);
    expect(await api.run("clip.split")).toBeNull();
    expect(call.mock.calls).toEqual([[IPC_COMMAND_TABLE], [IPC_RUN_COMMAND, { id: "clip.split" }]]);
  });

  it("実行できなかったら理由の文字を返す", async () => {
    const api = createIpcCommandsApi(vi.fn().mockRejectedValue("Unknown command: clip.nope"), vi.fn());
    expect(await api.run("clip.nope")).toBe("Unknown command: clip.nope");
  });

  it("押せるかが変わったイベントの表を渡し、返り値で購読を止める", async () => {
    const off = vi.fn();
    const listen = vi.fn().mockResolvedValue(off);
    const handler = vi.fn();
    const stop = await createIpcCommandsApi(vi.fn(), listen).onChanged(handler);
    expect(listen.mock.calls[0]?.[0]).toBe(COMMANDS_CHANGED_EVENT);
    listen.mock.calls[0]?.[1]({ payload: [lift] });
    expect(handler).toHaveBeenCalledWith([lift]);
    expect(stop).toBe(off);
  });

  it("アプリがメニューやキーから実行したパネルのコマンドの ID を渡す", async () => {
    const off = vi.fn();
    const listen = vi.fn().mockResolvedValue(off);
    const handler = vi.fn();
    const stop = await createIpcCommandsApi(vi.fn(), listen).onPanelCommand(handler);
    expect(listen.mock.calls[0]?.[0]).toBe(PANEL_COMMAND_EVENT);
    expect(PANEL_COMMAND_EVENT).toBe("mm:panel-command");
    listen.mock.calls[0]?.[1]({ payload: "media.import" });
    expect(handler).toHaveBeenCalledWith("media.import");
    expect(stop).toBe(off);
  });
});
