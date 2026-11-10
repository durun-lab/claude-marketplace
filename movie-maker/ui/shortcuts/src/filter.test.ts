import type { UiCommand } from "@mm/sdk";
import { describe, expect, it } from "vitest";
import { flatten, matches, sections } from "./filter.ts";

const command = (id: string, name: string, over: Partial<UiCommand> = {}): UiCommand => ({
  id,
  name,
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

// Rust の表は ID の名前の順なので、メニューの中の並びとは違う順で渡す。
const TABLE = [
  command("timeline.full_extent_zoom", "Full Extent Zoom", { menu: "view", menu_order: 2, key_label: "Shift+Z" }),
  command("playback.stop", "Stop", { key_label: "K" }),
  command("app.show_about", "About", { menu: "help", menu_order: 1 }),
  command("timeline.zoom_out", "Zoom Out", { menu: "view", menu_order: 1, key_label: "-" }),
  command("edit.undo", "Undo", { menu: "edit", menu_order: 0, key_label: "Ctrl+Z" }),
  command("timeline.zoom_in", "Zoom In", { menu: "view", menu_order: 0, key_label: "=" }),
  command("media.import", "Import Media…", { menu: "file", menu_order: 0, key_label: "Ctrl+I" }),
];

const names = (query: string) => flatten(sections(TABLE, query)).map((c) => c.name);

describe("sections", () => {
  it("メニューの組の順に並べ、組の中はメニューの中の並びにする。メニューに無いコマンドは最後の Other", () => {
    expect(sections(TABLE, "").map((s) => s.title)).toEqual(["File", "Edit", "View", "Help", "Other"]);
    expect(names("")).toEqual([
      "Import Media…",
      "Undo",
      "Zoom In",
      "Zoom Out",
      "Full Extent Zoom",
      "About",
      "Stop",
    ]);
  });

  it("zoom で絞ると Zoom In、Zoom Out、Full Extent Zoom がメニューの順に残り、空になった組は消える", () => {
    expect(names("zoom")).toEqual(["Zoom In", "Zoom Out", "Full Extent Zoom"]);
    expect(sections(TABLE, "zoom").map((s) => s.id)).toEqual(["view"]);
  });
});

describe("matches", () => {
  const zoomIn = TABLE[5] as UiCommand;
  it("名前は大文字小文字を問わず部分一致", () => {
    expect(matches(zoomIn, "ZOOM")).toBe(true);
    expect(matches(zoomIn, "oom i")).toBe(true);
    expect(matches(zoomIn, "pan")).toBe(false);
  });

  it("キーの見た目でも絞れる", () => {
    expect(names("shift+z")).toEqual(["Full Extent Zoom"]);
    expect(names("=")).toEqual(["Zoom In"]);
    expect(names("ctrl")).toEqual(["Import Media…", "Undo"]);
  });

  it("空白で分けた語を全部含むものだけ残す", () => {
    expect(names("zoom shift")).toEqual(["Full Extent Zoom"]);
    expect(names("zoom nothing")).toEqual([]);
  });
});
