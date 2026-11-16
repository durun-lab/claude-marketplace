import assert from 'node:assert/strict';
import { test } from 'node:test';
import { handOverChildren } from '../server/handover.ts';
import type { AgentTask } from '../shared/types.ts';

test('親セッションを新しくすると、子セッションと任せたタスクの元を新しい親へ付け替え、親のタスクを移す', () => {
  const task: AgentTask = { id: '1', subject: '確かめる', status: 'in_progress', delegatedTo: 'c1' };
  const old = { id: 'old', agentTasks: [task], agentTaskSeq: 1 };
  const next = { id: 'new' };
  const c1 = { id: 'c1', parent: 'old', agentTasks: [{ id: '1', subject: '確かめる', status: 'in_progress' as const, delegatedFrom: { session: 'old', task: '1' } }] };
  const c2 = { id: 'c2', parent: 'old', movedTo: 'c3' };
  const c3 = { id: 'c3', parent: 'old' };
  const other = { id: 'x', parent: 'someone' };
  assert.equal(handOverChildren({ old, new: next, c1, c2, c3, x: other }, old, next), 2);
  assert.deepEqual([c1.parent, c2.parent, c3.parent, other.parent], ['new', 'new', 'new', 'someone']);
  assert.deepEqual(c1.agentTasks[0]?.delegatedFrom, { session: 'new', task: '1' });
  assert.deepEqual([(next as { agentTasks?: AgentTask[] }).agentTasks, old.agentTasks], [[task], undefined]);
});
