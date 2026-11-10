import type { UiCommand } from "@mm/sdk";

/** メニューバーのメニューの並び（`menu.rs` の `BAR`）。 */
const MENUS = [
  ["file", "File"],
  ["edit", "Edit"],
  ["view", "View"],
  ["playback", "Playback"],
  ["timeline", "Timeline"],
  ["help", "Help"],
] as const;

/** メニューに入っていないコマンド（Stop など、キーだけで呼ぶもの）の組。 */
const OTHER = ["other", "Other"] as const;

export interface Section {
  id: string;
  title: string;
  commands: UiCommand[];
}

const byMenuOrder = (a: UiCommand, b: UiCommand): number =>
  (a.menu_order ?? Number.MAX_SAFE_INTEGER) - (b.menu_order ?? Number.MAX_SAFE_INTEGER) || a.name.localeCompare(b.name);

/** 検索語を空白で分け、全部が名前かキーの見た目に（大文字小文字を問わず）含まれるコマンドだけ通す。 */
export const matches = (command: UiCommand, query: string): boolean => {
  const haystack = `${command.name} ${command.key_label ?? ""}`.toLowerCase();
  return query
    .toLowerCase()
    .split(/\s+/)
    .filter(Boolean)
    .every((token) => haystack.includes(token));
};

/** 表をメニューの組ごとに、メニューの中の並びで並べる。絞り込みで空になった組は出さない。 */
export const sections = (table: readonly UiCommand[], query: string): Section[] => {
  const found = table.filter((c) => matches(c, query));
  return [...MENUS, OTHER]
    .map(([id, title]) => ({
      id,
      title,
      commands: found.filter((c) => (c.menu ?? OTHER[0]) === id).sort(byMenuOrder),
    }))
    .filter((s) => s.commands.length > 0);
};

/** 画面に出る順に並べたコマンド。上下の矢印の移動の並び。 */
export const flatten = (list: readonly Section[]): UiCommand[] => list.flatMap((s) => s.commands);
