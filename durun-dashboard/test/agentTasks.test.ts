import assert from 'node:assert/strict';
import { test } from 'node:test';
import { addTasks, changeTask, delegateTask, listTasks, updateTask, viewTasks, withLiveRoot } from '../server/agentTasks.ts';
import type { AgentTask } from '../shared/types.ts';

test('タスクは通し番号で登録し、状態と内容を変えて消せ、一覧は既定で完了していないものだけを返す', () => {
  const owner: { agentTasks?: AgentTask[]; agentTaskSeq?: number } = {};
  assert.equal(addTasks(owner, ['  ', { subject: '' }]).isError, true);
  assert.equal(addTasks(owner, ['調べる', { subject: '直す' }, { subject: '確かめる' }]).text, '登録しました。\n- 1 [未着手] 調べる\n- 2 [未着手] 直す\n- 3 [未着手] 確かめる');
  assert.equal(changeTask(owner, '1', { status: 'completed' }).text, '更新しました。');
  changeTask(owner, '2', { status: 'in_progress', subject: 'テストを足して直す' });
  assert.equal(changeTask(owner, '2', { status: 'done' }).isError, true);
  assert.equal(changeTask(owner, '9', { status: 'completed' }).isError, true);
  assert.equal(listTasks(owner, false).text, '- 2 [作業中] テストを足して直す\n- 3 [未着手] 確かめる');
  assert.equal(listTasks(owner, true).text.split('\n').length, 3);
  // 消したタスクの番号は使い直さない。
  changeTask(owner, '3', { status: 'deleted' });
  assert.equal(addTasks(owner, ['見直す']).text, '登録しました。\n- 4 [未着手] 見直す');
  changeTask(owner, '2', { status: 'completed' });
  changeTask(owner, '4', { status: 'completed' });
  assert.equal(listTasks(owner, false).text, '完了していないタスクはありません。');
});

test('下の手順が終わっていないタスクは完了にしない', () => {
  const owner: { agentTasks?: AgentTask[]; agentTaskSeq?: number } = {};
  addTasks(owner, ['直す', { subject: '手順', parent: '#1' }]);
  assert.match(updateTask(owner, '1', {}, { status: 'completed' }).text, /2 手順 が終わっていません/);
  updateTask(owner, '2', {}, { status: 'completed' });
  assert.equal(updateTask(owner, '1', {}, { status: 'completed' }).text, '更新しました。');
});

test('タスクの詳細は、説明と、登録・着手・完了の時刻を覚え、未着手に戻すと着手と完了の時刻を忘れる', () => {
  const owner: { agentTasks?: AgentTask[]; agentTaskSeq?: number } = {};
  addTasks(owner, [{ subject: '直す', description: 'テストが通るまで' }], { now: 100 });
  const task = (): AgentTask | undefined => owner.agentTasks?.[0];
  assert.deepEqual(task(), { id: '1', subject: '直す', description: 'テストが通るまで', status: 'pending', createdAt: 100 });
  // 一覧には説明を載せない。
  assert.equal(listTasks(owner, false).text, '- 1 [未着手] 直す');
  changeTask(owner, '1', { status: 'in_progress' }, 200);
  changeTask(owner, '1', { status: 'in_progress', description: '型も直す' }, 250);
  changeTask(owner, '1', { status: 'completed' }, 300);
  assert.deepEqual([task()?.startedAt, task()?.completedAt, task()?.description], [200, 300, '型も直す']);
  changeTask(owner, '1', { status: 'pending', description: '' }, 400);
  assert.deepEqual([task()?.startedAt, task()?.completedAt, task()?.description], [undefined, undefined, undefined]);
});

test('待つタスクは、同じ呼び出しの #n と既存の id で登録し、輪になる待ち合いと無い id を断り、消したタスクは待つのをやめる', () => {
  const owner: { agentTasks?: AgentTask[]; agentTaskSeq?: number } = {};
  addTasks(owner, ['設計する']);
  assert.equal(
    addTasks(owner, [{ subject: '実装する', blocked_by: ['1'] }, { subject: 'テストする', blocked_by: ['#1'] }, { subject: '見直す', blocked_by: ['#1', '#2'] }]).text,
    '登録しました。\n- 2 [未着手] 実装する（1 を待つ）\n- 3 [未着手] テストする（2 を待つ）\n- 4 [未着手] 見直す（2・3 を待つ）',
  );
  assert.equal(addTasks(owner, [{ subject: 'x', blocked_by: ['9'] }]).isError, true);
  assert.equal(owner.agentTasks?.length, 4);
  // 1 が 4 を待つと、4 → 3 → 2 → 1 → 4 の輪になる。
  assert.match(changeTask(owner, '1', { addBlockedBy: ['4'] }).text, /輪になります/);
  changeTask(owner, '1', { status: 'completed' });
  assert.equal(listTasks(owner, false).text, '- 2 [未着手] 実装する\n- 3 [未着手] テストする（2 を待つ）\n- 4 [未着手] 見直す（2・3 を待つ）');
  changeTask(owner, '4', { removeBlockedBy: ['2'] });
  changeTask(owner, '3', { status: 'deleted' });
  assert.deepEqual(owner.agentTasks?.map((t) => [t.id, t.blockedBy]), [['1', undefined], ['2', ['1']], ['4', []]]);
});

test('チケットのタスクは題を省くとチケットの名前にし、手順は parent で親の下に置き、輪になる親子と無い親を断る', () => {
  const owner: { agentTasks?: AgentTask[]; agentTaskSeq?: number } = {};
  const ticketOf = (t: string) => (/^CATS-\d+$/.test(t) ? { url: `https://jira.example.com/browse/${t}`, label: t } : undefined);
  assert.match(addTasks(owner, [{ ticket: 'not a ticket' }], { ticketOf }).text, /ticket（not a ticket）は/);
  assert.equal(
    addTasks(owner, [{ ticket: 'CATS-1' }, { subject: '調べる', parent: '#1' }, { subject: '直す', parent: '#1', blocked_by: ['#2'] }], { ticketOf }).text,
    '登録しました。\n- 1 [未着手] CATS-1\n- 2 [未着手] 調べる\n- 3 [未着手] 直す（2 を待つ）',
  );
  assert.deepEqual(owner.agentTasks?.map((t) => [t.id, t.ticket, t.parent]), [['1', 'https://jira.example.com/browse/CATS-1', undefined], ['2', undefined, '1'], ['3', undefined, '1']]);
  assert.match(changeTask(owner, '1', { parent: '3' }).text, /輪になります/);
  assert.match(changeTask(owner, '2', { parent: '9' }).text, /見つかりません/);
  changeTask(owner, '3', { parent: '' });
  changeTask(owner, '1', { status: 'deleted' });
  assert.deepEqual(owner.agentTasks?.map((t) => [t.id, t.parent]), [['2', undefined], ['3', undefined]]);
});

test('タスクを子セッションに任せると、子セッションのルートのタスクになり、子の登録はその下に入り、ルートの実体は親のタスクになる', () => {
  const parent: { id: string; agentTasks?: AgentTask[]; agentTaskSeq?: number } = { id: 'p' };
  const child: { id: string; agentTasks?: AgentTask[]; agentTaskSeq?: number; rootTask?: string } = { id: 'c' };
  addTasks(parent, [{ subject: '移行する', description: '手順書どおりに' }], { now: 100 });
  assert.equal(delegateTask(parent, '9', child).isError, true);
  delegateTask(parent, '1', child, 200);
  assert.deepEqual(parent.agentTasks?.[0], { id: '1', subject: '移行する', description: '手順書どおりに', status: 'in_progress', createdAt: 100, updatedAt: 200, startedAt: 200, delegatedTo: 'c' });
  assert.deepEqual(child.agentTasks?.[0]?.delegatedFrom, { session: 'p', task: '1' });
  addTasks(child, ['準備する', { subject: '外に置く', parent: '' }]);
  assert.deepEqual(child.agentTasks?.map((t) => t.parent), [undefined, '1', '1']);
  const sessions = { p: parent, c: child };
  // 子の手順が残っている間は、子も親もルートを完了にできない。
  assert.match(updateTask(child, '1', sessions, { status: 'completed' }, 250).text, /2 準備する、3 外に置く が終わっていません/);
  assert.equal(updateTask(parent, '1', sessions, { status: 'completed' }, 250).isError, true);
  updateTask(child, '2', sessions, { status: 'completed' }, 260);
  updateTask(child, '3', sessions, { status: 'completed' }, 260);
  // 子がルートを完了にすると、親のタスクが完了になる。
  updateTask(child, '1', sessions, { status: 'completed' }, 300);
  assert.deepEqual([parent.agentTasks?.[0]?.status, parent.agentTasks?.[0]?.completedAt, parent.agentTasks?.[0]?.updatedAt], ['completed', 300, 300]);
  // 親がタスクを戻して書き直すと、子のルートにもそのまま見える。
  changeTask(parent, '1', { status: 'in_progress', subject: '移行して確かめる' }, 400);
  assert.deepEqual([withLiveRoot(child, sessions)[0]?.status, withLiveRoot(child, sessions)[0]?.subject], ['in_progress', '移行して確かめる']);
  assert.equal(listTasks({ ...child, agentTasks: withLiveRoot(child, sessions) }, false).text.split('\n')[0], '- 1 [作業中] 移行して確かめる');
  // 親の画面では、子の手順が任せたタスクの下に並ぶ。
  assert.deepEqual(viewTasks(parent, sessions).map((t) => [t.id, t.parent, t.subject]), [['1', undefined, '移行して確かめる'], ['1/2', '1', '準備する'], ['1/3', '1', '外に置く']]);
  // ルートを消した後の登録は、いちばん外に置く。
  changeTask(child, '1', { status: 'deleted' });
  addTasks(child, ['後片付け']);
  assert.equal(child.agentTasks?.at(-1)?.parent, undefined);
});

test('起動済みの子セッションにも後からタスクを任せられ、子が登録していた親の無いタスクはルートの下に入り、任されたタスクを持つ子にはほかを結ばない', () => {
  const parent: { id: string; agentTasks?: AgentTask[]; agentTaskSeq?: number } = { id: 'p' };
  const child: { id: string; agentTasks?: AgentTask[]; agentTaskSeq?: number; rootTask?: string } = { id: 'c' };
  addTasks(parent, ['調べる', '直す']);
  addTasks(child, ['読む', { subject: '細かく読む', parent: '#1' }]);
  assert.equal(delegateTask(parent, '1', child).text, '任せました。');
  assert.deepEqual(child.agentTasks?.map((t) => [t.id, t.parent]), [['3', undefined], ['1', '3'], ['2', '1']]);
  assert.equal(child.rootTask, '3');
  assert.equal(parent.agentTasks?.[0]?.delegatedTo, 'c');
  assert.match(delegateTask(parent, '2', child).text, /すでに「調べる」を任されています/);
  addTasks(child, ['まとめる']);
  assert.equal(child.agentTasks?.at(-1)?.parent, '3');
});
