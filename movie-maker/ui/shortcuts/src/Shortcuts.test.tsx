// @vitest-environment jsdom
import { render } from "preact";
import { act } from "preact/test-utils";
import type { CommandsApi, UiCommand } from "@mm/sdk";
import { afterEach, describe, expect, it, vi } from "vitest";
import { Shortcuts } from "./Shortcuts.tsx";

const host = document.createElement("div");
document.body.append(host);

afterEach(() => render(null, host));

const command = (id: string, name: string, over: Partial<UiCommand> = {}): UiCommand => ({
  id,
  name,
  keys: [],
  key_label: null,
  description: null,
  menu: "view",
  menu_order: 0,
  panel: null,
  enabled: true,
  checked: null,
  ...over,
});

const TABLE = [
  command("timeline.zoom_in", "Zoom In", { key_label: "=", menu_order: 0 }),
  command("timeline.zoom_out", "Zoom Out", { key_label: "-", menu_order: 1 }),
  command("clip.lift", "Lift", { menu: "edit", key_label: "Delete", menu_order: 1, enabled: false }),
  command("edit.undo", "Undo", { menu: "edit", key_label: "Ctrl+Z", menu_order: 0 }),
];

const setup = async (table: UiCommand[] = TABLE, run: CommandsApi["run"] = async () => null) => {
  const api: CommandsApi = {
    table: async () => table,
    run: vi.fn(run),
    onChanged: async () => () => {},
    onPanelCommand: async () => () => {},
  };
  const close = vi.fn();
  await act(async () => {
    render(<Shortcuts commands={api} close={close} />, host);
  });
  return { api, close };
};

const press = async (key: string) => {
  await act(async () => {
    host.querySelector(".mm-shortcuts-search")?.dispatchEvent(new KeyboardEvent("keydown", { key, bubbles: true }));
  });
};

const type = async (text: string) => {
  const input = host.querySelector<HTMLInputElement>(".mm-shortcuts-search");
  await act(async () => {
    if (input) input.value = text;
    input?.dispatchEvent(new Event("input", { bubbles: true }));
  });
};

const rows = () => [...host.querySelectorAll(".mm-shortcuts-row")];
const selectedName = () => host.querySelector('[aria-selected="true"] .mm-shortcuts-name')?.textContent;

describe("Shortcuts", () => {
  it("メニューの組ごとに見出しと、名前とキーを出す", async () => {
    await setup();
    expect([...host.querySelectorAll(".mm-shortcuts-heading")].map((h) => h.textContent)).toEqual(["Edit", "View"]);
    expect(rows().map((r) => r.textContent)).toEqual(["UndoCtrl+Z", "LiftDelete", "Zoom In=", "Zoom Out-"]);
  });

  it("絞り込みの入力で行が減る", async () => {
    await setup();
    await type("zoom");
    expect(rows().map((r) => r.querySelector(".mm-shortcuts-name")?.textContent)).toEqual(["Zoom In", "Zoom Out"]);
    await type("nothing");
    expect(host.textContent).toContain("No matching commands");
  });

  it("上下の矢印で選び、Enter で実行して閉じる", async () => {
    const { api, close } = await setup();
    await type("zoom");
    expect(selectedName()).toBe("Zoom In");
    await press("ArrowDown");
    expect(selectedName()).toBe("Zoom Out");
    await press("ArrowDown");
    expect(selectedName()).toBe("Zoom Out");
    await press("ArrowUp");
    await press("ArrowUp");
    expect(selectedName()).toBe("Zoom In");
    await press("ArrowDown");
    await press("Enter");
    expect(api.run).toHaveBeenCalledWith("timeline.zoom_out");
    expect(close).toHaveBeenCalledTimes(1);
  });

});

describe("Shortcuts の実行", () => {
  it("押せないコマンドは薄い印を付け、Enter でも実行せず閉じない", async () => {
    const { api, close } = await setup();
    await type("lift");
    expect(rows()[0]?.getAttribute("aria-disabled")).toBe("true");
    expect(selectedName()).toBe("Lift");
    await press("Enter");
    expect(api.run).not.toHaveBeenCalled();
    expect(close).not.toHaveBeenCalled();
  });

  it("実行に失敗したら閉じずに理由を出す", async () => {
    const { close } = await setup(TABLE, async () => "Zoom In isn't available now");
    await type("zoom in");
    await press("Enter");
    expect(close).not.toHaveBeenCalled();
    expect(host.querySelector('[role="alert"]')?.textContent).toBe("Zoom In isn't available now");
  });

  it("Esc で閉じ、何も実行しない", async () => {
    const { api, close } = await setup();
    await press("Escape");
    expect(close).toHaveBeenCalledTimes(1);
    expect(api.run).not.toHaveBeenCalled();
  });

  it("行をクリックしても実行して閉じる", async () => {
    const { api, close } = await setup();
    await act(async () => {
      (rows().find((r) => r.textContent?.startsWith("Undo")) as HTMLElement).click();
    });
    expect(api.run).toHaveBeenCalledWith("edit.undo");
    expect(close).toHaveBeenCalledTimes(1);
  });
});
