// @vitest-environment jsdom
import { render } from "preact";
import { act } from "preact/test-utils";
import type { UiCommand } from "@mm/sdk";
import { afterEach, describe, expect, it, vi } from "vitest";
import { DEFAULT_GROUPS } from "./groups.ts";
import { TasksPanel } from "./TasksPanel.tsx";

const host = document.createElement("div");
document.body.append(host);

afterEach(() => render(null, host));

const click = (el: Element | null | undefined) => {
  act(() => {
    (el as HTMLElement).click();
  });
};

const command = (id: string, over: Partial<UiCommand> = {}): UiCommand => ({
  id,
  name: id,
  keys: [],
  key_label: null,
  description: null,
  menu: null,
  menu_order: null,
  panel: null,
  enabled: true,
  checked: null,
  ...over,
});

const split = command("clip.split", {
  name: "Split Clip",
  keys: ["KeyB"],
  key_label: "B",
  description: "Split the clip at the playhead",
});

const GROUPS = [
  {
    id: "g",
    title: "G",
    items: [
      { id: "split", label: "Split at playhead", command: "clip.split" },
      { id: "import", label: "Import video and audio", command: "media.import" },
      { id: "silence", label: "Remove silent parts", command: "clip.remove_silence" },
    ],
  },
];

const links = () => [...host.querySelectorAll<HTMLButtonElement>(".mm-tasks-link")];

describe("TasksPanel", () => {
  it("項目を押すとコマンドの ID を onInvoke に渡す", () => {
    const onInvoke = vi.fn();
    render(<TasksPanel onInvoke={onInvoke} />, host);
    click(host.querySelector(".mm-tasks-link"));
    expect(onInvoke).toHaveBeenCalledWith(DEFAULT_GROUPS[0]?.items[0]?.command);
  });

  it("見出しを押すとグループを閉じて、もう一度で開く", () => {
    render(<TasksPanel onInvoke={vi.fn()} />, host);
    const toggle = host.querySelector(".mm-tasks-toggle");
    const total = host.querySelectorAll(".mm-tasks-link").length;
    click(toggle);
    expect(toggle?.getAttribute("aria-expanded")).toBe("false");
    expect(host.querySelectorAll(".mm-tasks-link").length).toBe(total - (DEFAULT_GROUPS[0]?.items.length ?? 0));
    click(toggle);
    expect(host.querySelectorAll(".mm-tasks-link").length).toBe(total);
  });

  it("initiallyClosed のグループは閉じて始まる", () => {
    render(<TasksPanel onInvoke={vi.fn()} initiallyClosed={["import"]} />, host);
    expect(host.querySelector(".mm-tasks-toggle")?.getAttribute("aria-expanded")).toBe("false");
  });
});

describe("TasksPanel のツールチップ", () => {
  it("表が読めるまでは全部の項目を押せる見た目にし、ツールチップを出さない", () => {
    render(<TasksPanel groups={GROUPS} onInvoke={vi.fn()} commands={null} />, host);
    expect(links().map((l) => [l.getAttribute("aria-disabled"), l.title])).toEqual([
      [null, ""],
      [null, ""],
      [null, ""],
    ]);
  });

  it("ツールチップは名前とキー、説明があれば 2 行目に出す", () => {
    const table = [split, command("media.import", { name: "Import Media…", panel: "media" })];
    render(<TasksPanel groups={GROUPS} onInvoke={vi.fn()} commands={table} />, host);
    expect(links()[0]?.title).toBe("Split Clip (B)\nSplit the clip at the playhead");
    expect(links()[1]?.title).toBe("Import Media…");
  });
});

describe("TasksPanel の薄い項目", () => {
  it("表に無い項目は薄くし、ツールチップに Not available yet と出し、押しても呼ばない", () => {
    const onInvoke = vi.fn();
    render(<TasksPanel groups={GROUPS} onInvoke={onInvoke} commands={[split]} />, host);
    const silence = links()[2];
    expect(silence?.getAttribute("aria-disabled")).toBe("true");
    expect(silence?.title).toBe("Not available yet");
    click(silence);
    expect(onInvoke).not.toHaveBeenCalled();
    click(links()[0]);
    expect(onInvoke).toHaveBeenCalledWith("clip.split");
  });

  it("今押せないコマンドの項目は薄くし、ツールチップは名前とキーのまま", () => {
    const onInvoke = vi.fn();
    render(<TasksPanel groups={GROUPS} onInvoke={onInvoke} commands={[{ ...split, enabled: false }]} />, host);
    const link = links()[0];
    expect(link?.getAttribute("aria-disabled")).toBe("true");
    expect(link?.title).toBe("Split Clip (B)\nSplit the clip at the playhead");
    click(link);
    expect(onInvoke).not.toHaveBeenCalled();
  });

  it("既定の項目のうち、表に無いものだけを薄くする", () => {
    const table = ["media.import", "clip.split", "clip.add_transition", "export.start", "export.choose_target"].map((id) => command(id));
    render(<TasksPanel onInvoke={vi.fn()} commands={table} />, host);
    const dimmed = links().filter((l) => l.getAttribute("aria-disabled") === "true");
    expect(dimmed.map((l) => l.textContent)).toEqual([
      "Import pictures",
      "Transcribe speech",
      "Remove silent parts",
      "Add a title",
      "Add captions",
      "Add music",
    ]);
  });
});
