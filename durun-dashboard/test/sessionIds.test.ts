import assert from 'node:assert/strict';
import { test } from 'node:test';
import { sessionIdsIn } from '../shared/sessionIds.ts';

test('発言に出たセッションの id を、省略形はフルの id に直し、出た順に重ねずに拾う', () => {
  const a = '1403684c-9348-4c8e-95aa-2c7bb977f334';
  const b = 'abcdef12-0000-4000-8000-000000000001';
  const c = 'abcdef12-0000-4000-8000-000000000002';
  assert.deepEqual(sessionIdsIn([a, b, c], [`子は ${b}`, 'ジョブ 1403684c と abcdef12 と deadbeef', `もう一度 ${a}`]), [b, a]);
});
