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
  key_labels: [],
  description: null,
  menu: "View",
  menu_order: 12,
  panel: null,
  enabled: true,
  checked: null,
  ...over,
});

const TABLE = [
  command("timeline.zoom_in", "Zoom In", { key_labels: ["=", "+", "Ctrl+="], menu_order: 12 }),
  command("timeline.zoom_out", "Zoom Out", { key_labels: ["-", "Ctrl+-"], menu_order: 13 }),
  command("clip.lift", "Lift", { menu: "Edit", key_labels: ["Delete"], menu_order: 5, enabled: false }),
  command("edit.undo", "Undo", { menu: "Edit", key_labels: ["Ctrl+Z"], menu_order: 3 }),
];

/** `push` で、押せるかが変わったときにアプリが送る新しい表を届ける。 */
const setup = async (table: UiCommand[] = TABLE, run: CommandsApi["run"] = async () => null) => {
  let changed: (t: UiCommand[]) => void = () => {};
  const api: CommandsApi = {
    table: async () => table,
    run: vi.fn(run),
    onChanged: async (handler) => {
      changed = handler;
      return () => {};
    },
    onPanelCommand: async () => () => {},
  };
  const close = vi.fn();
  await act(async () => {
    render(<Shortcuts commands={api} close={close} />, host);
  });
  const push = async (t: UiCommand[]) => act(() => changed(t));
  return { api, close, push };
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
const keysOf = (row: Element | undefined) => [...(row?.querySelectorAll("kbd") ?? [])].map((k) => k.textContent);
const selectedName = () => host.querySelector('[aria-selected="true"] .mm-shortcuts-name')?.textContent;

describe("Shortcuts", () => {
  it("メニューの区画ごとに見出しと、名前と割り当てた全部のキーを出す", async () => {
    await setup();
    expect([...host.querySelectorAll(".mm-shortcuts-heading")].map((h) => h.textContent)).toEqual(["Edit", "View"]);
    expect(rows().map((r) => r.querySelector(".mm-shortcuts-name")?.textContent)).toEqual(["Undo", "Lift", "Zoom In", "Zoom Out"]);
    expect(keysOf(rows()[2])).toEqual(["=", "+", "Ctrl+="]);
    expect(keysOf(rows()[3])).toEqual(["-", "Ctrl+-"]);
  });

  it("絞り込みの入力で行が減る", async () => {
    await setup();
    await type("zoom");
    expect(rows().map((r) => r.querySelector(".mm-shortcuts-name")?.textContent)).toEqual(["Zoom In", "Zoom Out"]);
    await type("ctrl+-");
    expect(rows().map((r) => r.querySelector(".mm-shortcuts-name")?.textContent)).toEqual(["Zoom Out"]);
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

describe("Shortcuts の表示", () => {
  it("入/切のあるコマンドは入のときにチェックを出し、アプリが送り直した表に追従する", async () => {
    const snapping = command("timeline.toggle_snapping", "Snapping", { key_labels: ["N"], menu_order: 15, checked: false });
    const { push } = await setup([snapping]);
    expect(rows()[0]?.getAttribute("aria-checked")).toBe("false");
    expect(rows()[0]?.querySelector(".mm-shortcuts-check")?.textContent).toBe("");
    await push([{ ...snapping, checked: true }]);
    expect(rows()[0]?.getAttribute("aria-checked")).toBe("true");
    expect(rows()[0]?.querySelector(".mm-shortcuts-check")?.textContent).toBe("✓");
  });

  it("下端にキーの案内を出す", async () => {
    await setup();
    expect([...host.querySelectorAll(".mm-shortcuts-hint")].map((h) => h.textContent)).toEqual([
      "↑↓ Select",
      "Enter Run",
      "Esc Close",
    ]);
  });
});

describe("Shortcuts の実行", () => {
  it("押せないコマンドは薄い印を付け、Enter ではアプリが拒否した理由を出して閉じない", async () => {
    const { api, close, push } = await setup(TABLE, async (id) => (id === "clip.lift" ? "Lift isn't available now" : null));
    await type("lift");
    expect(rows()[0]?.getAttribute("aria-disabled")).toBe("true");
    expect(selectedName()).toBe("Lift");
    await press("Enter");
    expect(api.run).toHaveBeenCalledWith("clip.lift");
    expect(close).not.toHaveBeenCalled();
    expect(host.querySelector('[role="alert"]')?.textContent).toBe("Lift isn't available now");
    await push(TABLE.map((c) => (c.id === "clip.lift" ? { ...c, enabled: true } : c)));
    expect(rows()[0]?.getAttribute("aria-disabled")).toBe("false");
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
