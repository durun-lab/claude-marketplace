import type { UiCommand } from "@mm/sdk";
import { describe, expect, it } from "vitest";
import { flatten, matches, sections } from "./filter.ts";

const command = (id: string, name: string, over: Partial<UiCommand> = {}): UiCommand => ({
  id,
  name,
  keys: [],
  key_label: null,
  key_labels: [],
  description: null,
  menu: null,
  menu_order: null,
  panel: null,
  enabled: true,
  checked: null,
  ...over,
});

// Rust の表は ID の名前の順なので、メニューバーの中の並びとは違う順で渡す。menu_order はメニューをまたぐ通し番号。
const TABLE = [
  command("timeline.full_extent_zoom", "Full Extent Zoom", { menu: "View", menu_order: 14, key_labels: ["Shift+Z"] }),
  command("playback.stop", "Stop", { key_labels: ["K"] }),
  command("app.show_about", "About", { menu: "Help", menu_order: 40 }),
  command("timeline.zoom_out", "Zoom Out", { menu: "View", menu_order: 13, key_labels: ["-", "Ctrl+-"] }),
  command("edit.undo", "Undo", { menu: "Edit", menu_order: 3, key_labels: ["Ctrl+Z"] }),
  command("timeline.zoom_in", "Zoom In", { menu: "View", menu_order: 12, key_labels: ["=", "+", "Ctrl+="] }),
  command("media.import", "Import Media…", { menu: "File", menu_order: 0, key_labels: ["Ctrl+I"] }),
  command("export.choose_target", "Choose Render Target…", { panel: "deliver" }),
];

const names = (query: string) => flatten(sections(TABLE, query)).map((c) => c.name);

describe("sections", () => {
  it("メニューバーの中の位置の順に並べ、続く同じメニューを 1 つの区画にする。メニューに無いコマンドは最後の Other に名前の順", () => {
    expect(sections(TABLE, "").map((s) => s.title)).toEqual(["File", "Edit", "View", "Help", "Other"]);
    expect(names("")).toEqual([
      "Import Media…",
      "Undo",
      "Zoom In",
      "Zoom Out",
      "Full Extent Zoom",
      "About",
      "Choose Render Target…",
      "Stop",
    ]);
  });

  it("zoom で絞ると Zoom In、Zoom Out、Full Extent Zoom がメニューの順に残り、空になった区画は消える", () => {
    expect(names("zoom")).toEqual(["Zoom In", "Zoom Out", "Full Extent Zoom"]);
    expect(sections(TABLE, "zoom").map((s) => s.title)).toEqual(["View"]);
  });

  it("渡した表の並びは変えない", () => {
    const before = TABLE.map((c) => c.id);
    sections(TABLE, "");
    expect(TABLE.map((c) => c.id)).toEqual(before);
  });
});

describe("matches", () => {
  const zoomIn = TABLE[5] as UiCommand;
  it("名前は大文字小文字を問わず部分一致", () => {
    expect(matches(zoomIn, "ZOOM")).toBe(true);
    expect(matches(zoomIn, "oom i")).toBe(true);
    expect(matches(zoomIn, "pan")).toBe(false);
  });

  it("先頭のキーだけでなく、割り当てた全部のキーの見た目で絞れる", () => {
    expect(names("shift+z")).toEqual(["Full Extent Zoom"]);
    expect(names("ctrl+=")).toEqual(["Zoom In"]);
    expect(names("ctrl+-")).toEqual(["Zoom Out"]);
    expect(names("ctrl")).toEqual(["Import Media…", "Undo", "Zoom In", "Zoom Out"]);
  });

  it("空白で分けた語を全部含むものだけ残す", () => {
    expect(names("zoom shift")).toEqual(["Full Extent Zoom"]);
    expect(names("zoom nothing")).toEqual([]);
  });
});
