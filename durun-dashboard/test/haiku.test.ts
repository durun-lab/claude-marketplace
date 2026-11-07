import assert from 'node:assert/strict';
import { test } from 'node:test';
import { UpstreamError, structuredOf } from '../server/haiku.ts';

test('API の答えの JSON を、structured_output と同じ object として読む', () => {
  assert.deepEqual(structuredOf('{"answer":"はい","links":[]}', 'end_turn'), { answer: 'はい', links: [] });
});

test('断られた答えと途中で切れた答えは、UpstreamError にする', () => {
  assert.throws(() => structuredOf('', 'refusal'), UpstreamError);
  assert.throws(() => structuredOf('{"answer":"は', 'max_tokens'), UpstreamError);
});

test('object でない答えは、答えの文を添えて UpstreamError にする', () => {
  assert.throws(() => structuredOf('分かりません', 'end_turn'), (e) => e instanceof UpstreamError && e.message === '分かりません');
  assert.throws(() => structuredOf('[1]', 'end_turn'), UpstreamError);
});
