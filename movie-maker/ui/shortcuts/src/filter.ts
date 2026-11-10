import type { UiCommand } from "@mm/sdk";

/** メニューに入っていないコマンド（Stop など、キーだけで呼ぶもの）の区画の見出し。Shotcut の Actions and Shortcuts も、
 * メニューから読んだ操作にはメニューの名前を、残りの操作には Other を付ける。 */
export const OTHER = "Other";

/** メニューの名前か Other を見出しにした、コマンドの行のまとまり。 */
export interface Section {
  title: string;
  commands: UiCommand[];
}

/** メニューバーの中の位置の順。メニューに無いコマンドは最後に名前の順で並べる。 */
const byPlace = (a: UiCommand, b: UiCommand): number =>
  (a.menu_order ?? Number.MAX_SAFE_INTEGER) - (b.menu_order ?? Number.MAX_SAFE_INTEGER) || a.name.localeCompare(b.name);

/** 検索語を空白で分け、全部が名前か、どれかのキーの見た目に（大文字小文字を問わず）含まれるコマンドだけ通す。 */
export const matches = (command: UiCommand, query: string): boolean => {
  const haystack = [command.name, ...command.key_labels].join(" ").toLowerCase();
  return query
    .toLowerCase()
    .split(/\s+/)
    .filter(Boolean)
    .every((token) => haystack.includes(token));
};

/** 絞り込んだ表をメニューバーの中の位置の順に並べ、続く同じメニューを 1 つの区画にする。位置はメニューをまたぐ通し番号なので、
 * 区画もメニューバーの順に並び、メニューの並びを持たずに済む。 */
export const sections = (table: readonly UiCommand[], query: string): Section[] => {
  const out: Section[] = [];
  for (const command of table.filter((c) => matches(c, query)).sort(byPlace)) {
    const title = command.menu ?? OTHER;
    const last = out[out.length - 1];
    if (last?.title === title) last.commands.push(command);
    else out.push({ title, commands: [command] });
  }
  return out;
};

/** 画面に出る順に並べたコマンド。上下の矢印の移動の並び。 */
export const flatten = (list: readonly Section[]): UiCommand[] => list.flatMap((s) => s.commands);
