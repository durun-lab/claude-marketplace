import assert from 'node:assert/strict';
import { test } from 'node:test';
import { claudeSettingFields } from '../shared/claudeSettings.ts';

test('スキーマの型から入力欄の種類を決め、管理者向けと使わなくなった項目を並べない', () => {
  const fields = claudeSettingFields({
    properties: {
      $schema: { type: 'string' },
      verbose: { type: 'boolean', description: 'Show full output' },
      effortLevel: { type: 'string', enum: ['low', 'high'] },
      cleanupPeriodDays: { type: 'integer' },
      language: { type: 'string' },
      permissions: { type: 'object' },
      theme: {},
      includeCoAuthoredBy: { type: 'boolean', description: "DEPRECATED. Use 'attribution' instead." },
      claudeMd: { type: 'string', description: '(Managed settings only) instructions' },
    },
  });
  assert.deepEqual(
    fields.map((f) => [f.key, f.kind, f.options]),
    [
      ['cleanupPeriodDays', 'number', undefined],
      ['effortLevel', 'enum', ['low', 'high']],
      ['language', 'string', undefined],
      ['permissions', 'json', undefined],
      ['theme', 'json', undefined],
      ['verbose', 'boolean', undefined],
    ],
  );
});
