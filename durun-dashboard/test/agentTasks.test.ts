import assert from 'node:assert/strict';
import { test } from 'node:test';
import { addTasks, changeTask, listTasks } from '../server/agentTasks.ts';
import type { AgentTask } from '../shared/types.ts';

test('タスクは通し番号で登録し、状態と内容を変えて消せ、一覧は既定で完了していないものだけを返す', () => {
  const owner: { agentTasks?: AgentTask[]; agentTaskSeq?: number } = {};
  assert.equal(addTasks(owner, ['  ']).isError, true);
  assert.equal(addTasks(owner, ['調べる', '直す', '確かめる']).text, '登録しました。\n- 1 [未着手] 調べる\n- 2 [未着手] 直す\n- 3 [未着手] 確かめる');
  assert.equal(changeTask(owner, '1', 'completed', undefined).text, '更新しました。');
  changeTask(owner, '2', 'in_progress', 'テストを足して直す');
  assert.equal(changeTask(owner, '2', 'done', undefined).isError, true);
  assert.equal(changeTask(owner, '9', 'completed', undefined).isError, true);
  assert.equal(listTasks(owner, false).text, '- 2 [作業中] テストを足して直す\n- 3 [未着手] 確かめる');
  assert.equal(listTasks(owner, true).text.split('\n').length, 3);
  // 消したタスクの番号は使い直さない。
  changeTask(owner, '3', 'deleted', undefined);
  assert.equal(addTasks(owner, ['見直す']).text, '登録しました。\n- 4 [未着手] 見直す');
  changeTask(owner, '2', 'completed', undefined);
  changeTask(owner, '4', 'completed', undefined);
  assert.equal(listTasks(owner, false).text, '完了していないタスクはありません。');
});
