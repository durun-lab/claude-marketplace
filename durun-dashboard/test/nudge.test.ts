import assert from 'node:assert/strict';
import { test } from 'node:test';
import { clickedInsteadOf, onMutter } from '../client/nudge.ts';
import { HOME_SHORTCUT, SEARCH_SHORTCUT, shortcutLabel } from '../client/shortcut.ts';

test('ショートカットのある操作をマウスで 10 回するごとに、10 回目の操作のショートカットを呟き、キーボードで押したボタンは数えない', () => {
  const heard: string[] = [];
  const off = onMutter((t) => heard.push(t));
  const click = (detail: number) => ({ detail }) as MouseEvent;
  for (let i = 0; i < 9; i++) clickedInsteadOf(click(1), SEARCH_SHORTCUT);
  clickedInsteadOf(click(0), HOME_SHORTCUT);
  assert.deepEqual(heard, []);
  clickedInsteadOf(click(1), HOME_SHORTCUT);
  assert.equal(heard.length, 1);
  assert.match(heard[0] ?? '', new RegExp(shortcutLabel(HOME_SHORTCUT).replace(/[+]/g, '\\+')));
  off();
});
