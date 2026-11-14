import assert from 'node:assert/strict';
import path from 'node:path';
import { test } from 'node:test';
import { tempDir } from './tmp.ts';

// 実際の状態に触れないよう、データの置き場所を一時ディレクトリに向けてから読み込む。
process.env.DURUN_DASHBOARD_DATA_DIR = path.join(tempDir('durun-dashboard-pr-body-'), 'data');
const { readPrBody, writePrBody } = await import('../server/prBody.ts');

/** 本文を 1 つ持つ偽の gh。pr view は本文を返し、pr edit は標準入力の本文に書き換える。 */
const fakeGh = (url: string, initial: string) => {
  const pr = { body: initial };
  const gh = async (args: string[], input?: string): Promise<string> => {
    if (args[1] === 'view') return JSON.stringify({ url, body: pr.body });
    if (args[1] === 'edit') {
      pr.body = input ?? '';
      return '';
    }
    throw new Error(`想定していない gh の呼び出し: ${args.join(' ')}`);
  };
  return { pr, gh };
};

test('読んでいない本文は書き換えない', async () => {
  const { pr, gh } = fakeGh('https://github.com/o/r/pull/1', '元の本文');
  const result = await writePrBody('s1', '1', '新しい本文', undefined, gh);
  assert.equal(result.isError, true);
  assert.equal(pr.body, '元の本文');
});

test('読んだ本文は書き換えられ、続けて書くときは読み直さなくてよい', async () => {
  const { pr, gh } = fakeGh('https://github.com/o/r/pull/2', '元の本文');
  assert.match((await readPrBody('s1', '2', undefined, gh)).text, /元の本文/);
  assert.equal((await writePrBody('s1', '2', '2 版', undefined, gh)).isError, undefined);
  assert.equal((await writePrBody('s1', '2', '3 版', undefined, gh)).isError, undefined);
  assert.equal(pr.body, '3 版');
});

test('読んだ後に GitHub 上で変わった本文は、読み直すまで書き換えない', async () => {
  const { pr, gh } = fakeGh('https://github.com/o/r/pull/3', '元の本文');
  await readPrBody('s1', '3', undefined, gh);
  pr.body = '人間が足した本文';
  assert.equal((await writePrBody('s1', '3', 'Claude の本文', undefined, gh)).isError, true);
  assert.equal(pr.body, '人間が足した本文');
  await readPrBody('s1', '3', undefined, gh);
  assert.equal((await writePrBody('s1', '3', '人間が足した本文\nClaude の追記', undefined, gh)).isError, undefined);
  assert.equal(pr.body, '人間が足した本文\nClaude の追記');
});

test('ほかのセッションが読んでいても、自分が読んでいなければ書き換えない', async () => {
  const { pr, gh } = fakeGh('https://github.com/o/r/pull/4', '元の本文');
  await readPrBody('s1', '4', undefined, gh);
  assert.equal((await writePrBody('s2', '4', '新しい本文', undefined, gh)).isError, true);
  assert.equal(pr.body, '元の本文');
});
