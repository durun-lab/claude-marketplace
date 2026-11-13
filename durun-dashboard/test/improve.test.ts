import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { test } from 'node:test';
import { hostOs } from '../server/platform.ts';
import { js, useFakeClaude } from './fakeClaude.ts';
import { tempDir } from './tmp.ts';

// 実際の状態や要望、ワークフローの定義に触れないよう、データの置き場所を一時ディレクトリに向けてから読み込む。
const dataDir = tempDir('durun-dashboard-improve-');
process.env.DURUN_DASHBOARD_DATA_DIR = dataDir;
process.env.CLAUDE_CONFIG_DIR = tempDir('durun-dashboard-improve-claude-');
const { saveFeedback } = await import('../server/improve.ts');
const { BUNDLED_AGENTS_DIR, WORKFLOWS_DIR, endFeedbackWave, improveState, runWorkflow, setWorkflowEnabled, startWorkflows, updateWorkflow, workflowState } = await import('../server/workflow.ts');
const { ensureSession, onChange, state } = await import('../server/store.ts');

const feedbackDir = path.join(dataDir, 'feedback');
const pending = (): string[] => fs.readdirSync(feedbackDir).filter((f) => f.endsWith('.json')).sort();
const done = (): string[] => fs.readdirSync(path.join(feedbackDir, 'done')).sort();

// 偽の claude は、--bg で受け取った引数を書き足してすぐ失敗する。ワークフローは起動に失敗したウェーブを待機させる。
const bin = tempDir('durun-dashboard-bin-');
const callsFile = path.join(bin, 'calls');
useFakeClaude(bin, `fs.appendFileSync(${js(callsFile)}, lines(args) + '\\n---\\n'); console.error('fake failure'); process.exitCode = 1;`);
const calls = (): string[] => (fs.existsSync(callsFile) ? fs.readFileSync(callsFile, 'utf8').split('\n---\n').filter(Boolean) : []);

const writeDef = (enabled: boolean): void => setWorkflowEnabled('improve', enabled);

const launchFailed = (): Promise<void> => {
  const before = calls().length;
  return new Promise((resolve) => onChange(() => calls().length > before && state.workflowRuns?.improve?.error?.includes('fake') && resolve()));
};

test('自動実装の設定を、改善要望のトリガーを持つワークフローの定義へ移す', () => {
  state.improve = { enabled: true, model: 'opus' };
  startWorkflows();
  const dir = path.join(WORKFLOWS_DIR, 'improve');
  const def = JSON.parse(fs.readFileSync(path.join(dir, 'workflow.json'), 'utf8'));
  assert.deepEqual([def.enabled, def.mode, def.compact, def.triggers], [true, 'continue', true, [{ kind: 'feedback' }]]);
  // 指示はフォルダーに書かず、plugin に同梱した agent を使う。設定を保存しても、指示を変えなければ写さない。
  assert.equal(fs.existsSync(path.join(dir, 'improve.md')), false);
  const w = workflowState().workflows.find((x) => x.id === 'improve');
  assert.equal(w?.agentFile, path.join(BUNDLED_AGENTS_DIR, 'improve.md'));
  updateWorkflow('improve', { ...w, account: 'sub' });
  assert.equal(fs.existsSync(path.join(dir, 'improve.md')), false);
  updateWorkflow('improve', { ...w, prompt: '別の指示' });
  assert.match(fs.readFileSync(path.join(dir, 'improve.md'), 'utf8'), /^---\nname: improve\n[\s\S]*\n---\n\n別の指示\n$/);
  fs.rmSync(path.join(dir, 'improve.md'));
  updateWorkflow('improve', w);
  assert.equal(state.improve, undefined);
});

test('要望が届くと、たまった要望の一覧を 1 つのウェーブとして渡し、起動できなければ「実行」まで待つ', async () => {
  let failed = launchFailed();
  saveFeedback('一つ目', [], {});
  await failed;
  const [first = ''] = pending();
  // どの OS で起きたことかを後から読めるよう、サーバーの OS を添える。
  assert.deepEqual(JSON.parse(fs.readFileSync(path.join(feedbackDir, first), 'utf8')).context.os, hostOs());
  const [args = ''] = calls();
  assert.match(args, /^--bg$/m);
  assert.match(args, /^opus$/m);
  // 依頼は要望の一覧だけにし、読み方や進め方は agent のファイルを指す。
  assert.match(args, new RegExp(`1 件たまっています[\\s\\S]*${first}: 一つ目[\\s\\S]*improve\\.md の指示に従って`));
  assert.doesNotMatch(args, /git worktree|targets|対応した要望/);
  assert.equal(state.workflowRuns?.improve?.feedback?.stalled, 'unmarked');

  // 待機中は、新しい要望が届いてもウェーブを始めない。
  await new Promise((r) => setTimeout(r, 5));
  saveFeedback('二つ目', [], {});
  assert.equal(calls().length, 1);
  failed = launchFailed();
  runWorkflow('improve');
  await failed;
  assert.match(calls()[1] ?? '', /2 件たまっています/);
});

test('ウェーブは最後の発言の「対応した要望:」で区切り、済んだ要望だけを移す。印が無ければ、後で印を書くまで待つ', () => {
  writeDef(false);
  const [first = '', second = ''] = pending();
  ensureSession('s1', dataDir).workflow = 'improve';
  state.workflowRuns = { improve: { queue: [], feedback: { items: [first, second], sessionId: 's1' } } };
  assert.deepEqual([improveState().workflow?.running, improveState().pending], [true, 0]);

  endFeedbackWave('s1', 'テストが通ったら、コミットします。');
  assert.equal(state.workflowRuns.improve?.feedback?.stalled, 'unmarked');
  endFeedbackWave('other', `対応した要望:\n${first}`);
  assert.deepEqual(pending(), [first, second]);
  endFeedbackWave('s1', `まとめて直しました。\n${second} は次に回します。\n対応した要望:\n${first.replace(/\.json$/, '')}`);
  assert.deepEqual(pending(), [second]);
  assert.deepEqual(done(), [first]);
  assert.match(JSON.parse(fs.readFileSync(path.join(feedbackDir, 'done', first), 'utf8')).result, /まとめて直しました/);
  assert.equal(state.workflowRuns.improve?.feedback, undefined);
  assert.equal(state.workflowRuns.improve?.error, undefined);
  assert.deepEqual([improveState().workflow?.running, improveState().pending], [false, 1]);
});

test('済んだ要望が無いまま止まったウェーブは、新しい要望が届くと待機を解いて、たまった要望をすべて渡す', async () => {
  ensureSession('s2', dataDir).workflow = 'improve';
  state.workflowRuns = { improve: { queue: [], feedback: { items: pending(), sessionId: 's2' } } };
  endFeedbackWave('s2', '対応した要望:\nなし');
  assert.equal(state.workflowRuns.improve?.feedback?.stalled, 'nothing-done');
  assert.match(state.workflowRuns.improve?.error ?? '', /対応を終えた要望/);

  writeDef(true);
  const failed = launchFailed();
  saveFeedback('三つ目', [], {});
  await failed;
  assert.match(calls().at(-1) ?? '', /2 件たまっています[\s\S]*二つ目[\s\S]*三つ目/);
});

test('ワークフローに移す前の改善のセッションが動いている間は、次のウェーブを始めず、印を書いて止まったら済んだ要望を移して始める', async () => {
  const [second = '', third = ''] = pending();
  state.workflowRuns = { improve: { queue: [] } };
  ensureSession('old', dataDir).improver = true;
  state.improve = { run: { sessionId: 'old', pid: process.pid } };
  const before = calls().length;
  runWorkflow('improve');
  assert.equal(state.workflowRuns.improve?.feedback, undefined);
  assert.equal(improveState().workflow?.running, true);
  // 「今すぐ実行」は、改善要望のウェーブの代わりに、ただ動かしたことを伝える。
  await new Promise<void>((resolve) => onChange(() => calls().length > before && resolve()));
  assert.match(calls().at(-1) ?? '', /今すぐ実行/);

  state.workflowRuns = { improve: { queue: [] } };
  const failed = launchFailed();
  endFeedbackWave('old', `対応した要望:\n${second}`);
  await failed;
  assert.equal(state.improve, undefined);
  assert.deepEqual(pending(), [third]);
  assert.match(calls().at(-1) ?? '', /1 件たまっています/);
});

test('溜まったイベントは、次に渡すときに 1 つの依頼にまとめ、依頼の頭と指示の場所は 1 回だけ書く', async () => {
  const dir = path.join(WORKFLOWS_DIR, 'batch');
  fs.mkdirSync(dir, { recursive: true });
  fs.writeFileSync(path.join(dir, 'workflow.json'), JSON.stringify({ name: 'まとめ', cwd: dataDir, triggers: [], mode: 'new' }));
  fs.writeFileSync(path.join(dir, 'batch.md'), '---\nname: batch\n---\n\n指示\n');
  // 書き込みと一緒に定義を読み直させる。
  setWorkflowEnabled('batch', false);
  state.workflowRuns = { batch: { queue: ['一つ目のイベント', '二つ目のイベント'] } };
  runWorkflow('batch');
  assert.deepEqual(state.workflowRuns.batch?.queue, []);
  // イベントの区切りが calls の区切りと同じ --- なので、記録をそのまま読む。前のテストのワークフローの起動も混ざるので、このワークフローの分だけを数える。
  const raw = (): string => (fs.existsSync(callsFile) ? fs.readFileSync(callsFile, 'utf8') : '');
  await new Promise<void>((resolve) => onChange(() => raw().includes('batch.md') && resolve()));
  assert.equal(raw().match(/「まとめ」/g)?.length, 1);
  assert.match(raw(), /「まとめ」 一つ目のイベント[\s\S]*二つ目のイベント[\s\S]*今すぐ実行[\s\S]*batch\.md の指示に従って/);
  assert.equal(raw().match(/batch\.md の指示に従って/g)?.length, 1);
});
