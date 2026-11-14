import assert from 'node:assert/strict';
import { test } from 'node:test';
import fs from 'node:fs';
import path from 'node:path';
import { tempDir } from './tmp.ts';

// 実際の定義と状態に触れないよう、データとワークフローの置き場所を一時ディレクトリに向けてから読み込む。
process.env.DURUN_DASHBOARD_DATA_DIR = tempDir('durun-dashboard-workflow-');
process.env.CLAUDE_CONFIG_DIR = tempDir('durun-dashboard-workflow-claude-');
const { dayMatches, flushBursts, scanWorkflows, scheduleDue, setWorkflowEnabled, startWorkflows, updateWorkflow, WORKFLOWS_DIR } = await import('../server/workflow.ts');

const cwd = path.resolve('work');
type Def = { id?: string; prompt?: string; model?: string; effort?: string; [key: string]: unknown };
const def = (extra: Def = {}): Def => ({ id: 'daily', cwd, prompt: '朝の確認', triggers: [{ kind: 'schedule', at: '09:00' }], ...extra });

/** root の下に、定義を 1 つのフォルダーとして書く。指示と model と effort は agent に、ほかは workflow.json に書く。 */
const writeDir = (root: string, { id = 'daily', prompt = '', model, effort, ...settings }: Def): string => {
  const dir = path.join(root, id);
  fs.mkdirSync(dir, { recursive: true });
  const head = [`name: ${id}`, 'description: テスト', ...(model ? [`model: ${model}`] : []), ...(effort ? [`effort: ${effort}`] : [])];
  fs.writeFileSync(path.join(dir, `${id}.md`), `---\n${head.join('\n')}\n---\n\n${prompt}\n`);
  fs.writeFileSync(path.join(dir, 'workflow.json'), JSON.stringify(settings));
  return dir;
};

const parse = (...defs: Def[]) => {
  const root = tempDir('durun-dashboard-workflow-scan-');
  for (const d of defs) writeDir(root, d);
  return scanWorkflows(root);
};

test('定義を読み、省いた項目は既定にする', () => {
  const { workflows, error } = parse(def({ enabled: true, model: 'nope', effort: 'high' }));
  assert.equal(error, undefined);
  const dir = workflows[0]?.dir ?? '';
  assert.deepEqual(workflows, [{ id: 'daily', dir, agentFile: path.join(dir, 'daily.md'), name: 'daily', enabled: true, cwd, prompt: '朝の確認', triggers: [{ kind: 'schedule', at: '09:00' }], mode: 'new', effort: 'high' }]);
  assert.equal(path.basename(dir), 'daily');
});

test('引き継ぐモードと同時数と、レビュー依頼とメンションと改善要望のトリガーを読む', () => {
  const { workflows } = parse(def({ mode: 'continue', compact: true, maxSessions: 3, triggers: [{ kind: 'github-review' }, { kind: 'slack-mention' }, { kind: 'feedback' }, { kind: 'schedule', at: '18:30', days: [1, 5, 9] }] }));
  assert.equal(workflows[0]?.mode, 'continue');
  assert.equal(workflows[0]?.maxSessions, 3);
  assert.equal(workflows[0]?.compact, true);
  assert.deepEqual(workflows[0]?.triggers, [{ kind: 'github-review' }, { kind: 'slack-mention' }, { kind: 'feedback' }, { kind: 'schedule', at: '18:30', days: [1, 5] }]);
});

test('バースト時間は、ワークフローの正の整数だけを読む', () => {
  const burstOf = (extra: Def) => parse(def(extra)).workflows[0]?.burstMinutes;
  assert.equal(burstOf({ burstMinutes: 5 }), 5);
  assert.equal(burstOf({ burstMinutes: 0 }), undefined);
});

test('メンションのトリガーに書いたバースト時間は、ワークフローのバースト時間として読む', () => {
  const triggers = [{ kind: 'jira-mention', burstMinutes: 3 }, { kind: 'slack-mention' }];
  const w = parse(def({ triggers })).workflows[0];
  assert.equal(w?.burstMinutes, 3);
  assert.deepEqual(w?.triggers, [{ kind: 'jira-mention' }, { kind: 'slack-mention' }]);
});

test('バーストは、最後のイベントからバースト時間が過ぎたら、トリガーの種類ごとに 1 つの依頼にまとめて渡す', () => {
  const w = parse(def({ burstMinutes: 5, triggers: [{ kind: 'slack-mention' }, { kind: 'github-review' }] })).workflows[0];
  assert.ok(w);
  const run = { queue: [], bursts: { 'slack-mention': { texts: ['a', 'b'], lastAt: 0 }, 'github-review': { texts: ['x#1 t\nu'], lastAt: 0 } } };
  assert.deepEqual(flushBursts(w, run, 4 * 60_000), []);
  assert.deepEqual(flushBursts(w, run, 5 * 60_000), ['Slack で自分へのメンションが届きました:\na\nb', 'GitHub で自分へのレビュー依頼が届きました:\nx#1 t\nu']);
  assert.deepEqual(run.bursts, {});
});

test('compact は引き継ぐモードでだけ読む', () => {
  assert.equal(parse(def({ compact: true })).workflows[0]?.compact, undefined);
});

test('読めないフォルダーは動かさずに理由を返し、ほかのワークフローは動かす。workflow.json の無いフォルダーは読まない', () => {
  for (const bad of [def({ id: 'bad', triggers: [{ kind: 'schedule', at: '25:00' }] }), def({ id: 'bad', cwd: 'relative' }), def({ id: 'a b' }), def({ id: 'bad', prompt: ' ' })]) {
    const parsed = parse(def({ id: 'ok' }), bad);
    assert.deepEqual(parsed.workflows.map((w) => w.id), ['ok']);
    assert.ok(parsed.error);
  }
  const root = tempDir('durun-dashboard-workflow-scan-');
  fs.mkdirSync(path.join(root, 'plain-agent'));
  fs.writeFileSync(path.join(root, 'plain-agent', 'plain-agent.md'), '---\nname: plain-agent\n---\n\n指示');
  fs.mkdirSync(path.join(root, 'broken'));
  fs.writeFileSync(path.join(root, 'broken', 'workflow.json'), '{');
  const parsed = scanWorkflows(root);
  assert.deepEqual(parsed.workflows, []);
  assert.match(parsed.error ?? '', /broken/);
});

const at = (y: number, mo: number, d: number, h: number, mi: number) => new Date(y, mo - 1, d, h, mi).getTime();

test('時刻のトリガーは、前に確かめてから過ぎた時刻で動く', () => {
  const t = { kind: 'schedule' as const, at: '09:00' };
  assert.equal(scheduleDue(t, at(2026, 11, 9, 8, 59), at(2026, 11, 9, 9, 0)), true);
  assert.equal(scheduleDue(t, at(2026, 11, 9, 9, 0), at(2026, 11, 9, 9, 1)), false);
  // スリープ明けに、前の日に過ぎた時刻も拾う。
  assert.equal(scheduleDue(t, at(2026, 11, 8, 6, 0), at(2026, 11, 9, 7, 0)), true);
  // 丸 1 日より前の時刻は拾わない。2026-11-07 は土曜。
  assert.equal(scheduleDue({ ...t, days: [6] }, at(2026, 11, 1, 10, 0), at(2026, 11, 9, 8, 0)), false);
});

test('曜日を指定したトリガーは、その曜日だけ動く', () => {
  // 2026-11-09 は月曜、11-08 は日曜。
  const weekdays = { kind: 'schedule' as const, at: '09:00', days: [1, 2, 3, 4, 5] };
  assert.equal(scheduleDue(weekdays, at(2026, 11, 9, 8, 0), at(2026, 11, 9, 10, 0)), true);
  assert.equal(scheduleDue(weekdays, at(2026, 11, 8, 8, 0), at(2026, 11, 8, 10, 0)), false);
});

const dirOf = (id: string) => path.join(WORKFLOWS_DIR, id);
const settingsOf = (id: string) => JSON.parse(fs.readFileSync(path.join(dirOf(id), 'workflow.json'), 'utf8')) as Record<string, unknown>;
const agentOf = (id: string) => fs.readFileSync(path.join(dirOf(id), `${id}.md`), 'utf8');

test('設定を書き込み、送らなかった項目は消す。指示と model と effort は agent に書き、ほかのフロントマターと知らない項目は残す', () => {
  writeDir(WORKFLOWS_DIR, def({ enabled: true, maxSessions: 2, note: 'メモ', effort: 'high' }));
  writeDir(WORKFLOWS_DIR, def({ id: 'other' }));
  const other = [settingsOf('other'), agentOf('other')];
  updateWorkflow('daily', { name: '朝', cwd, prompt: '直した依頼', triggers: [{ kind: 'feedback' }], mode: 'continue', compact: true, model: 'opus' });
  assert.deepEqual(settingsOf('daily'), { cwd, triggers: [{ kind: 'feedback' }], enabled: true, note: 'メモ', name: '朝', mode: 'continue', compact: true });
  assert.equal(agentOf('daily'), '---\nname: daily\ndescription: テスト\nmodel: opus\n---\n\n直した依頼\n');
  assert.deepEqual([settingsOf('other'), agentOf('other')], other);
  setWorkflowEnabled('daily', false);
  assert.equal(settingsOf('daily').enabled, false);
});

test('読めない設定は書き込まずに理由を返す', () => {
  writeDir(WORKFLOWS_DIR, def({ id: 'strict' }));
  const before = [settingsOf('strict'), agentOf('strict')];
  assert.throws(() => updateWorkflow('strict', { cwd: 'relative', prompt: 'x', triggers: [], mode: 'new' }), /絶対パス/);
  assert.throws(() => updateWorkflow('strict', { cwd, prompt: 'x', triggers: [{ kind: 'schedule', at: '25:00' }], mode: 'new' }), /トリガー/);
  assert.throws(() => updateWorkflow('strict', { cwd, prompt: ' ', triggers: [], mode: 'new' }), /指示/);
  assert.throws(() => updateWorkflow('missing', { cwd, prompt: 'x', triggers: [], mode: 'new' }), /ありません/);
  assert.deepEqual([settingsOf('strict'), agentOf('strict')], before);
});

test('1 つのファイルに並べていた定義を、ワークフローごとのフォルダーへ移し、元のファイルの名前を変える', () => {
  const legacy = path.join(process.env.DURUN_DASHBOARD_DATA_DIR ?? '', 'workflows.json');
  fs.writeFileSync(legacy, JSON.stringify({ workflows: [{ id: 'moved', name: '移す', enabled: true, cwd, prompt: '移した依頼', triggers: [], mode: 'new', model: 'sonnet', effort: 'low' }] }));
  startWorkflows();
  assert.deepEqual(settingsOf('moved'), { name: '移す', enabled: true, cwd, triggers: [], mode: 'new' });
  assert.equal(agentOf('moved'), '---\nname: moved\ndescription: ダッシュボードのワークフロー「移す」の指示\nmodel: sonnet\neffort: low\n---\n\n移した依頼\n');
  assert.deepEqual([fs.existsSync(legacy), fs.existsSync(`${legacy}.migrated`)], [false, true]);
});

test('compactWindow は引き継ぐモードで、Claude Code が受け付ける範囲だけ読む', () => {
  const window = (extra: Def) => parse(def(extra)).workflows[0]?.compactWindow;
  assert.equal(window({ mode: 'continue', compactWindow: 200_000 }), 200_000);
  assert.equal(window({ mode: 'new', compactWindow: 200_000 }), undefined);
  assert.equal(window({ mode: 'continue', compactWindow: 50_000 }), undefined);
});

test('日付と営業日を指定したトリガーは、月の前と後ろから数えた日で動き、営業日は土日と祝日を除いて数える', () => {
  const day = (y: number, mo: number, d: number) => new Date(y, mo - 1, d, 9, 0);
  const t = { kind: 'schedule' as const, at: '09:00' };
  // 2026-11-01 は日曜、11-02 は月曜、11-03 は文化の日、11-30 は月曜。
  assert.equal(dayMatches({ ...t, dates: [1] }, day(2026, 11, 1)), true);
  assert.equal(dayMatches({ ...t, dates: [-1] }, day(2026, 11, 30)), true);
  assert.equal(dayMatches({ ...t, dates: [-1] }, day(2026, 11, 29)), false);
  assert.equal(dayMatches({ ...t, businessDays: [1] }, day(2026, 11, 1)), false);
  assert.equal(dayMatches({ ...t, businessDays: [1] }, day(2026, 11, 2)), true);
  assert.equal(dayMatches({ ...t, businessDays: [2] }, day(2026, 11, 4)), true);
  assert.equal(dayMatches({ ...t, businessDays: [-1] }, day(2026, 11, 30)), true);
  // 2027-01-01 は元日、01-02 と 01-03 は土日、01-04 は月曜。
  assert.equal(dayMatches({ ...t, businessDays: [1] }, day(2027, 1, 4)), true);
  // 曜日と合わせると、どちらにも合う日だけ。
  assert.equal(dayMatches({ ...t, days: [1], businessDays: [1] }, day(2026, 11, 2)), true);
  assert.equal(dayMatches({ ...t, days: [2], businessDays: [1] }, day(2026, 11, 2)), false);
});
