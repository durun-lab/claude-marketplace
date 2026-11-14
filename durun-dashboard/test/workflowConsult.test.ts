import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { test } from 'node:test';
import { COMPACT_WINDOW_MAX, COMPACT_WINDOW_MIN, EFFORT_LEVELS, MODELS } from '../shared/types.ts';

const SKILL = path.join(import.meta.dirname, '..', '..', 'plugins', 'durun-dashboard', 'skills', 'workflow-consult', 'SKILL.md');

test('ワークフローの相談のスキルは、ダッシュボードが読める model・effort・compactWindow とトリガーをそのまま案内する', () => {
  const text = fs.readFileSync(SKILL, 'utf8');
  assert.ok(text.includes(`model（${MODELS.join(' / ')}）`));
  assert.ok(text.includes(`effort（${EFFORT_LEVELS.join(' / ')}）`));
  assert.ok(text.includes(`${COMPACT_WINDOW_MIN}〜${COMPACT_WINDOW_MAX}`));
  for (const kind of ['schedule', 'github-review', 'slack-mention', 'jira-mention', 'feedback']) assert.ok(text.includes(`"kind": "${kind}"`), kind);
  assert.match(text, /^disable-model-invocation: true$/m);
});
