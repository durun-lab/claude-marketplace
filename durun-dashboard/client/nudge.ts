// ショートカットのある操作をマウスでしたら数え、EVERY 回ごとにイルカに呟かせる。悪気なくショートカットを勧める口ぶりにする。
import { EDIT_SHORTCUT, FULLSCREEN_SHORTCUT, HOME_SHORTCUT, OPEN_SHORTCUT, SAVE_SHORTCUT, SEARCH_SHORTCUT, type Shortcut, shortcutLabel } from './shortcut.ts';

const EVERY = 10;

const LINES = new Map<Shortcut, (key: string) => string>([
  [HOME_SHORTCUT, (key) => `${key} で切り替えられるのに…。いえ、マウスで行くのも味がありますよね`],
  [SEARCH_SHORTCUT, (key) => `検索って ${key} で一瞬なんですけど、メニューから開くの、丁寧で素敵です`],
  [EDIT_SHORTCUT, (key) => `編集は ${key} でも始められますよ。あ、ご存じでしたか`],
  [FULLSCREEN_SHORTCUT, (key) => `${key} なら手を離さずに広げられるのに…。運動になっていいですね`],
  [OPEN_SHORTCUT, (key) => `${key} で開けるんですけど、メニューをたどるのもお好きなんですね`],
  [SAVE_SHORTCUT, (key) => `保存は ${key} で済むのに…。几帳面に押すの、ちょっと尊敬します`],
]);

// ページを読み込み直すと数え直す。
let count = 0;
const listeners = new Set<(text: string) => void>();

/** イルカが呟きを受け取る。戻り値の関数を呼ぶと受けるのをやめる。 */
export const onMutter = (fn: (text: string) => void): (() => void) => {
  listeners.add(fn);
  return () => void listeners.delete(fn);
};

/** shortcut でもできる操作を押したら呼ぶ。キーボードで押したボタンは detail が 0 なので数えない。 */
export const clickedInsteadOf = (e: MouseEvent, shortcut: Shortcut): void => {
  if (e.detail === 0 || ++count % EVERY) return;
  const text = LINES.get(shortcut)?.(shortcutLabel(shortcut));
  if (text) listeners.forEach((fn) => fn(text));
};
