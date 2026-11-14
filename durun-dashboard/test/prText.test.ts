import assert from 'node:assert/strict';
import path from 'node:path';
import { test } from 'node:test';
import { tempDir } from './tmp.ts';

// 実際の状態に触れないよう、データの置き場所を一時ディレクトリに向けてから読み込む。
process.env.DURUN_DASHBOARD_DATA_DIR = path.join(tempDir('durun-dashboard-pr-text-'), 'data');
const { readPr, writePr } = await import('../server/prText.ts');

/** PR を 1 つ持つ偽の gh。pr view はタイトルと本文を返し、pr edit は --title の値と標準入力の本文に書き換える。 */
const fakeGh = (n: number) => {
  const pr = { url: `https://github.com/o/r/pull/${n}`, title: '元の題名', body: '元の本文' };
  const gh = async (args: string[], input?: string): Promise<string> => {
    if (args[1] === 'view') return JSON.stringify(pr);
    if (args[1] !== 'edit') throw new Error(`想定していない gh の呼び出し: ${args.join(' ')}`);
    const t = args.indexOf('--title');
    if (t >= 0) pr.title = args[t + 1]!;
    if (args.includes('--body-file')) pr.body = input ?? '';
    return '';
  };
  return { pr, gh };
};

test('読んでいない PR は書き換えない', async () => {
  const { pr, gh } = fakeGh(1);
  assert.equal((await writePr('s1', '1', { body: '新しい本文' }, undefined, gh)).isError, true);
  assert.equal(pr.body, '元の本文');
});

test('読んだ PR は書き換えられ、省いたほうは変わらず、続けて書くときは読み直さなくてよい', async () => {
  const { pr, gh } = fakeGh(2);
  assert.match((await readPr('s1', '2', undefined, gh)).text, /タイトル: 元の題名\n\n元の本文/);
  assert.equal((await writePr('s1', '2', { body: '2 版' }, undefined, gh)).isError, undefined);
  assert.deepEqual([pr.title, pr.body], ['元の題名', '2 版']);
  assert.equal((await writePr('s1', '2', { title: '新しい題名' }, undefined, gh)).isError, undefined);
  assert.deepEqual([pr.title, pr.body], ['新しい題名', '2 版']);
  assert.equal((await writePr('s1', '2', { title: '題名 3', body: '3 版' }, undefined, gh)).isError, undefined);
  assert.deepEqual([pr.title, pr.body], ['題名 3', '3 版']);
});

test('読んだ後に GitHub 上で変わった PR は、読み直すまで書き換えない', async () => {
  const { pr, gh } = fakeGh(3);
  await readPr('s1', '3', undefined, gh);
  pr.body = '人間が足した本文';
  assert.equal((await writePr('s1', '3', { body: 'Claude の本文' }, undefined, gh)).isError, true);
  // タイトルだけを書くときも、本文が変わっていれば断る。タイトルと本文は 1 組で読んだものとして扱う。
  assert.equal((await writePr('s1', '3', { title: 'Claude の題名' }, undefined, gh)).isError, true);
  assert.deepEqual([pr.title, pr.body], ['元の題名', '人間が足した本文']);
  await readPr('s1', '3', undefined, gh);
  assert.equal((await writePr('s1', '3', { body: '人間が足した本文\nClaude の追記' }, undefined, gh)).isError, undefined);
  assert.equal(pr.body, '人間が足した本文\nClaude の追記');
});

test('ほかのセッションが読んでいても、自分が読んでいなければ書き換えない', async () => {
  const { pr, gh } = fakeGh(4);
  await readPr('s1', '4', undefined, gh);
  assert.equal((await writePr('s2', '4', { body: '新しい本文' }, undefined, gh)).isError, true);
  assert.equal(pr.body, '元の本文');
});
