import assert from 'node:assert/strict';
import { test } from 'node:test';
import path from 'node:path';
import { tempDir } from './tmp.ts';

// 実際の定義と状態に触れないよう、データの置き場所を一時ディレクトリに向けてから読み込む。
process.env.DURUN_DASHBOARD_DATA_DIR = tempDir('durun-dashboard-workflow-');
const { parseWorkflows, scheduleDue } = await import('../server/workflow.ts');

const cwd = path.resolve('work');
const def = (extra: object = {}) => ({ id: 'daily', cwd, prompt: '朝の確認', triggers: [{ kind: 'schedule', at: '09:00' }], ...extra });

test('定義を読み、省いた項目は既定にする', () => {
  const { workflows, error } = parseWorkflows(JSON.stringify({ workflows: [def({ enabled: true, model: 'nope' })] }));
  assert.equal(error, undefined);
  assert.deepEqual(workflows, [{ id: 'daily', name: 'daily', enabled: true, cwd, prompt: '朝の確認', triggers: [{ kind: 'schedule', at: '09:00' }], mode: 'new' }]);
});

test('引き継ぐモードと同時数と、レビュー依頼とメンションと改善要望のトリガーを読む', () => {
  const { workflows } = parseWorkflows(JSON.stringify({ workflows: [def({ mode: 'continue', compact: true, maxSessions: 3, triggers: [{ kind: 'github-review' }, { kind: 'slack-mention' }, { kind: 'feedback' }, { kind: 'schedule', at: '18:30', days: [1, 5, 9] }] })] }));
  assert.equal(workflows[0]?.mode, 'continue');
  assert.equal(workflows[0]?.maxSessions, 3);
  assert.equal(workflows[0]?.compact, true);
  assert.deepEqual(workflows[0]?.triggers, [{ kind: 'github-review' }, { kind: 'slack-mention' }, { kind: 'feedback' }, { kind: 'schedule', at: '18:30', days: [1, 5] }]);
});

test('compact は引き継ぐモードでだけ読む', () => {
  assert.equal(parseWorkflows(JSON.stringify({ workflows: [def({ compact: true })] })).workflows[0]?.compact, undefined);
});

test('1 つでも読めない定義があれば、どれも動かさない', () => {
  for (const bad of [def({ triggers: [{ kind: 'schedule', at: '25:00' }] }), def({ cwd: 'relative' }), def({ id: 'a b' }), def({ prompt: ' ' })]) {
    const parsed = parseWorkflows(JSON.stringify({ workflows: [def({ id: 'ok' }), bad] }));
    assert.deepEqual(parsed.workflows, []);
    assert.ok(parsed.error);
  }
  assert.ok(parseWorkflows(JSON.stringify({ workflows: [def(), def()] })).error?.includes('重なって'));
  assert.ok(parseWorkflows('{').error);
});

const at = (y: number, mo: number, d: number, h: number, mi: number) => new Date(y, mo - 1, d, h, mi).getTime();

test('時刻のトリガーは、前に確かめてから過ぎた時刻で動く', () => {
  const t = { kind: 'schedule' as const, at: '09:00' };
  assert.equal(scheduleDue(t, at(2026, 11, 9, 8, 59), at(2026, 11, 9, 9, 0)), true);
  assert.equal(scheduleDue(t, at(2026, 11, 9, 9, 0), at(2026, 11, 9, 9, 1)), false);
  // スリープ明けに、前の日に過ぎた時刻も拾う。
  assert.equal(scheduleDue(t, at(2026, 11, 8, 6, 0), at(2026, 11, 9, 7, 0)), true);
  // 丸 1 日より前の時刻は拾わない。2026-11-07 は土曜。
  assert.equal(scheduleDue({ ...t, days: [6] }, at(2026, 11, 1, 10, 0), at(2026, 11, 9, 8, 0)), false);
});

test('曜日を指定したトリガーは、その曜日だけ動く', () => {
  // 2026-11-09 は月曜、11-08 は日曜。
  const weekdays = { kind: 'schedule' as const, at: '09:00', days: [1, 2, 3, 4, 5] };
  assert.equal(scheduleDue(weekdays, at(2026, 11, 9, 8, 0), at(2026, 11, 9, 10, 0)), true);
  assert.equal(scheduleDue(weekdays, at(2026, 11, 8, 8, 0), at(2026, 11, 8, 10, 0)), false);
});
