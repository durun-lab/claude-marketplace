// ワークフローアプリ。Claude Code のルーティンのように、トリガーでセッションを動かす。ルーティンと違い、手元のダッシュボードで動く。
// 1 つのワークフローは ~/.claude/agents/<id>/ の 1 つのフォルダーにまとめ、フォルダーを写せばほかの人の手元でも動く。
// 指示と model と effort は、同じフォルダーの <id>.md に Claude Code の agent として書く。普段のセッションからもサブエージェントとして呼べる。
// トリガーや動かし方は workflow.json に書く。補助のスクリプトなども同じフォルダーに置ける。
// 定義は人間とエージェントが書く。ダッシュボードは書き換えを読み直してすぐ使い、画面で切り替えた ON と OFF と、「設定」で直した項目を書き込む。
// 動きは 1 分ごとに確かめる。時刻のトリガーはその間に過ぎた時刻で、レビュー依頼のトリガーは GitHub の Inbox に新しく現れた PR で、メンションのトリガーは Slack の検索と JIRA のコメントに新しく現れたメンションで動く。
// 改善要望のトリガーは、要望が届いたときと前のウェーブを終えたときにもすぐ確かめる。
import fs from 'node:fs';
import path from 'node:path';
import { COMPACT_WINDOW_MAX, COMPACT_WINDOW_MIN, EFFORT_LEVELS, type BurstKind, type ImproveState, MODELS, type WorkflowDef, type WorkflowInfo, type WorkflowState, type WorkflowTrigger } from '../shared/types.ts';
import { CLAUDE_DIR, isAlive, readLiveSessions } from './claude.ts';
import { COMPACT_COMMAND, compactSession, sendMessage, startTrackedSession } from './deliver.ts';
import { DONE_MARK, feedbackEvent, finishItems, improveWorkflow, onFeedback, pendingItems } from './improve.ts';
import { githubInbox, jiraMentions } from './integrations.ts';
import { slackMentions } from './slack.ts';
import { DATA_DIR, type FeedbackWave, newId, persist, state, touch, type WorkflowRun } from './store.ts';

export const WORKFLOWS_DIR = path.join(CLAUDE_DIR, 'agents');
// 1 つのファイルに定義を並べていたころの置き場所。起動したときに、フォルダーへ移す。
const LEGACY_FILE = path.join(DATA_DIR, 'workflows.json');
const SETTINGS_FILE = 'workflow.json';
// 画面はスナップショットのたびに定義を読むので、フォルダーを読み直すのは 1 秒に 1 回までにする。
const RESCAN_MS = 1000;
// agent のフロントマターのうち、ワークフローが読み書きする項目。
const AGENT_KEYS = ['model', 'effort'] as const;
const TICK_MS = 60_000;
// スリープ明けなどで長く確かめていなかったときも、丸 1 日より前の時刻では動かさない。
const LOOKBACK_MS = 24 * 60 * 60_000;
const DAY_MS = 24 * 60 * 60_000;
const SEEN_KEPT = 500;
const MARK = '[durun-dashboard ワークフロー]';
const DIRECT_SECTION = 'Needs your review';
const MENTION_ERROR = 'Slack のメンションを読めません';
const JIRA_MENTION_ERROR = 'JIRA のメンションを読めません';
const ID_PATTERN = /^[\w-]+$/;
const TIME_PATTERN = /^([01]\d|2[0-3]):[0-5]\d$/;
// 溜まったイベントを 1 つの依頼にまとめるときの区切り。
const SEPARATOR = '\n\n---\n\n';

type Parsed = { workflows: WorkflowDef[]; error?: string };

const isObject = (v: unknown): v is Record<string, unknown> => typeof v === 'object' && v !== null && !Array.isArray(v);

const positiveInt = (v: unknown): number | undefined => (Number.isInteger(v) && (v as number) > 0 ? (v as number) : undefined);

const triggerOf = (raw: unknown): WorkflowTrigger => {
  if (isObject(raw) && (raw.kind === 'github-review' || raw.kind === 'slack-mention' || raw.kind === 'jira-mention')) return { kind: raw.kind };
  if (isObject(raw) && raw.kind === 'feedback') return { kind: 'feedback' };
  if (isObject(raw) && raw.kind === 'schedule' && typeof raw.at === 'string' && TIME_PATTERN.test(raw.at)) {
    const days = Array.isArray(raw.days) ? raw.days.filter((d): d is number => Number.isInteger(d) && d >= 0 && d <= 6) : undefined;
    return { kind: 'schedule', at: raw.at, ...(days?.length ? { days } : {}) };
  }
  throw new Error(`トリガー ${JSON.stringify(raw)} を読めません`);
};

/** agent のファイル。フロントマターの行と、本文。 */
type Agent = { lines: string[]; body: string };

const FRONTMATTER = /^---\r?\n([\s\S]*?)\r?\n---\r?\n?([\s\S]*)$/;

export const parseAgent = (text: string): Agent => {
  const m = FRONTMATTER.exec(text);
  return m ? { lines: (m[1] ?? '').split(/\r?\n/), body: (m[2] ?? '').trim() } : { lines: [], body: text.trim() };
};

const agentField = (agent: Agent, key: string): string | undefined =>
  agent.lines.map((l) => new RegExp(`^${key}:\\s*(.*)$`).exec(l)?.[1]?.trim().replace(/^["']|["']$/g, '')).find((v) => v !== undefined);

/** フロントマターの model と effort と本文を差し替える。name や description など、ほかの行は書いたまま残す。 */
export const formatAgent = (agent: Agent, fields: { model?: string; effort?: string; prompt: string }): string => {
  const kept = agent.lines.filter((l) => !AGENT_KEYS.some((k) => l.startsWith(`${k}:`)));
  const added = AGENT_KEYS.flatMap((k) => (fields[k] ? [`${k}: ${fields[k]}`] : []));
  return `---\n${[...kept, ...added].join('\n')}\n---\n\n${fields.prompt.trim()}\n`;
};

const isCompactWindow = (v: unknown): v is number => Number.isInteger(v) && (v as number) >= COMPACT_WINDOW_MIN && (v as number) <= COMPACT_WINDOW_MAX;

const agentPath = (dir: string, id: string): string => path.join(dir, `${id}.md`);

/** workflow.json の設定と agent の指示を合わせて、1 つの定義にする。 */
export const defOf = (id: string, dir: string, raw: unknown, agent: Agent): WorkflowDef => {
  if (!isObject(raw)) throw new Error(`${id} の ${SETTINGS_FILE} がオブジェクトではありません`);
  const { name, enabled, cwd, theme, triggers, mode, burstMinutes, maxSessions, compact, compactWindow, account } = raw;
  const [prompt, model, effort] = [agent.body, agentField(agent, 'model'), agentField(agent, 'effort')];
  if (!ID_PATTERN.test(id)) throw new Error(`フォルダー名 ${id} は英数字とハイフンで書いてください`);
  if (typeof cwd !== 'string' || !path.isAbsolute(cwd)) throw new Error(`${id} の cwd は絶対パスで書いてください`);
  if (!prompt) throw new Error(`${id} の ${id}.md に指示がありません`);
  // メンションのトリガーごとに burstMinutes を書いた定義も、ワークフローのバースト時間として読む。
  const burst = positiveInt(burstMinutes) ?? (Array.isArray(triggers) ? triggers.map((t) => (isObject(t) ? positiveInt(t.burstMinutes) : undefined)).find(Boolean) : undefined);
  return {
    id,
    dir,
    name: typeof name === 'string' && name.trim() ? name.trim() : id,
    enabled: enabled === true,
    cwd,
    ...(typeof theme === 'string' && theme.trim() ? { theme: theme.trim() } : {}),
    prompt,
    triggers: Array.isArray(triggers) ? triggers.map(triggerOf) : [],
    mode: mode === 'continue' ? 'continue' : 'new',
    ...(burst ? { burstMinutes: burst } : {}),
    ...(mode === 'continue' && compact === true ? { compact: true } : {}),
    ...(mode === 'continue' && isCompactWindow(compactWindow) ? { compactWindow } : {}),
    ...(Number.isInteger(maxSessions) && (maxSessions as number) > 0 ? { maxSessions: maxSessions as number } : {}),
    ...(typeof model === 'string' && (MODELS as readonly string[]).includes(model) ? { model } : {}),
    ...(typeof effort === 'string' && (EFFORT_LEVELS as readonly string[]).includes(effort) ? { effort } : {}),
    ...(typeof account === 'string' && account ? { account } : {}),
  };
};

/** 1 つのフォルダーを読む。workflow.json が無いフォルダーは、ワークフローではない。 */
const readDir = (dir: string): WorkflowDef | undefined => {
  const id = path.basename(dir);
  let settings: string;
  try {
    settings = fs.readFileSync(path.join(dir, SETTINGS_FILE), 'utf8');
  } catch {
    return undefined;
  }
  try {
    const agent = fs.existsSync(agentPath(dir, id)) ? parseAgent(fs.readFileSync(agentPath(dir, id), 'utf8')) : { lines: [], body: '' };
    return defOf(id, dir, JSON.parse(settings), agent);
  } catch (e) {
    throw new Error(`${id} を読めません: ${(e as Error).message}`);
  }
};

/**
 * root の下のフォルダーからワークフローを読む。読めないフォルダーは動かさずに理由を返し、ほかのワークフローは動かす。
 * 書きかけの定義で動かさないため。
 */
export const scanWorkflows = (root: string): Parsed => {
  const workflows: WorkflowDef[] = [];
  const errors: string[] = [];
  for (const d of fs.readdirSync(root, { withFileTypes: true })) {
    if (!d.isDirectory()) continue;
    try {
      const w = readDir(path.join(root, d.name));
      if (w) workflows.push(w);
    } catch (e) {
      errors.push((e as Error).message);
    }
  }
  return { workflows, ...(errors.length ? { error: errors.join('\n') } : {}) };
};

let cache: { at: number; parsed: Parsed } | undefined;

/** 定義を読む。フォルダーが無ければ、定義は無い。 */
const readWorkflows = (): Parsed => {
  const now = Date.now();
  if (!cache || now - cache.at >= RESCAN_MS) {
    try {
      cache = { at: now, parsed: scanWorkflows(WORKFLOWS_DIR) };
    } catch {
      cache = { at: now, parsed: { workflows: [] } };
    }
  }
  return cache.parsed;
};

/** 読み直しを待たずに、書き換えた定義を使う。 */
const forgetWorkflows = (): void => {
  cache = undefined;
};

/**
 * 定義を 1 つのフォルダーに書く。既にあれば書かない。id と prompt と model と effort は agent に、ほかは workflow.json に書く。
 * agent の description は、普段のセッションがサブエージェントを選ぶときに読む。
 */
const writeWorkflowDir = (def: Record<string, unknown>): void => {
  const { id, prompt, model, effort, ...settings } = def as { id: string; prompt: string; model?: string; effort?: string; name?: unknown };
  const dir = path.join(WORKFLOWS_DIR, id);
  if (fs.existsSync(dir)) return;
  fs.mkdirSync(dir, { recursive: true });
  const name = typeof settings.name === 'string' ? settings.name : id;
  const head = { lines: [`name: ${id}`, `description: ダッシュボードのワークフロー「${name}」の指示`], body: '' };
  fs.writeFileSync(agentPath(dir, id), formatAgent(head, { model, effort, prompt }));
  fs.writeFileSync(path.join(dir, SETTINGS_FILE), `${JSON.stringify(settings, null, 2)}\n`);
  forgetWorkflows();
};

/** 1 つのファイルに並べていた定義を、ワークフローごとのフォルダーへ移す。移したファイルは名前を変えて残す。 */
const migrateLegacyFile = (): void => {
  if (!fs.existsSync(LEGACY_FILE)) return;
  try {
    const raw = JSON.parse(fs.readFileSync(LEGACY_FILE, 'utf8')) as { workflows?: Record<string, unknown>[] };
    for (const def of raw.workflows ?? []) if (typeof def.id === 'string' && ID_PATTERN.test(def.id)) writeWorkflowDir(def);
    fs.renameSync(LEGACY_FILE, `${LEGACY_FILE}.migrated`);
  } catch (e) {
    console.error(`${LEGACY_FILE} をフォルダーへ移せません: ${(e as Error).message}`);
  }
};

/** 時刻のトリガーの時刻のうち、from より後で to までに過ぎたものがあるか。時刻と曜日は、このマシンの時間帯で数える。 */
export const scheduleDue = (t: Extract<WorkflowTrigger, { kind: 'schedule' }>, from: number, to: number): boolean => {
  const [h = 0, m = 0] = t.at.split(':').map(Number);
  const since = Math.max(from, to - LOOKBACK_MS);
  const day = new Date(since);
  day.setHours(0, 0, 0, 0);
  for (let at = day.getTime(); at <= to; at += DAY_MS) {
    const d = new Date(at);
    d.setHours(h, m, 0, 0);
    if (d.getTime() > since && d.getTime() <= to && (!t.days || t.days.includes(d.getDay()))) return true;
  }
  return false;
};

const runOf = (id: string): WorkflowRun => {
  state.workflowRuns ??= {};
  return (state.workflowRuns[id] ??= { queue: [] });
};

/**
 * イベントの内容が指示に埋もれないよう、指示は本文を写さずに agent のファイルを指す。
 * 補助のスクリプトなどは、同じフォルダーにあるので、ファイルの場所から辿れる。
 */
const promptFor = (w: WorkflowDef, event: string): string => `${MARK}「${w.name}」 ${event}\n\n進め方は ${agentPath(w.dir, w.id)} の指示に従ってください。`;

/** 起動を頼み、id をまだ受け取っていないセッションの数。ワークフローごとに数える。 */
const launching = new Map<string, number>();

const sessionsOf = (id: string) => Object.values(state.sessions).filter((m) => m.workflow === id && !m.archived && !m.movedTo);

/** 動いているセッションの数。入力待ちのものは数えない。 */
const runningCount = (id: string, live = readLiveSessions()): number =>
  (launching.get(id) ?? 0) + sessionsOf(id).filter((m) => (live.get(m.id)?.status ?? 'idle') !== 'idle').length;

/** セッションをまだ受け取っていない改善要望のウェーブ。改善要望はキューが空のときだけ積むので、次に渡す本文に入っている。 */
const unassignedWave = (run: WorkflowRun): FeedbackWave | undefined => (run.feedback && !run.feedback.sessionId ? run.feedback : undefined);

const launch = (w: WorkflowDef, run: WorkflowRun, text: string): void => {
  launching.set(w.id, (launching.get(w.id) ?? 0) + 1);
  run.lastRunAt = Date.now();
  const wave = unassignedWave(run);
  startTrackedSession(w.cwd, text, w.model, w.effort, undefined, w.compactWindow ? { autoCompactWindow: w.compactWindow } : undefined, w.account)
    .then((meta) => {
      Object.assign(meta, { workflow: w.id, title: w.name, ...(w.theme ? { theme: w.theme } : {}) });
      if (w.mode === 'continue') run.sessionId = meta.id;
      if (wave) wave.sessionId = meta.id;
      run.error = undefined;
    })
    .catch((e: Error) => {
      run.error = `セッションを起動できません: ${e.message}`;
      // 起動できないまま毎分やり直さないよう、人間が「実行」を押すまで待つ。
      if (wave) wave.stalled = 'unmarked';
    })
    .finally(() => {
      launching.set(w.id, (launching.get(w.id) ?? 1) - 1);
      touch();
    });
};

/** 引き継いでいるセッション。会話を引き継いだ複製へ移っていたら、複製をたどる。アーカイブしていたら無い。 */
const continuedSession = (run: WorkflowRun): string | undefined => {
  let id = run.sessionId;
  for (let meta = id ? state.sessions[id] : undefined; meta?.movedTo; meta = state.sessions[meta.movedTo]) id = meta.movedTo;
  const meta = id ? state.sessions[id] : undefined;
  return meta && !meta.archived ? id : undefined;
};

const queueTo = (sessionId: string, texts: string[]): void => {
  for (const text of texts) state.queue.push({ id: newId(), sessionId, text, createdAt: Date.now() });
};

/**
 * 引き継いでいるセッションへ依頼を送る。動いているセッションにはチャットのキューに積み、送らせる。止まっているセッションには、送って再開させる。
 * compact なら、前回までの会話を compact してから送る。止まっているセッションは compact で再開させ、依頼はその後ろのキューに積む。
 */
const deliverTo = (w: WorkflowDef, sessionId: string, text: string): void => {
  const live = readLiveSessions().has(sessionId);
  if (live) return queueTo(sessionId, w.compact ? [COMPACT_COMMAND, text] : [text]);
  if (!w.compact) return void sendMessage(sessionId, text);
  compactSession(sessionId, state.sessions[sessionId]?.cwd ?? w.cwd)
    .then(() => queueTo(sessionId, [text]))
    // compact できなくても、イベントは落とさずに送る。
    .catch((e: Error) => {
      runOf(w.id).error = `セッションを compact できません: ${e.message}`;
      sendMessage(sessionId, text);
    })
    .finally(touch);
};

/** 溜まっているイベントをすべて取り出し、1 つの依頼にまとめる。 */
const takeAll = (w: WorkflowDef, run: WorkflowRun): string => promptFor(w, run.queue.splice(0).join(SEPARATOR));

/**
 * 待たせているイベントを、渡せるようになったら 1 つの依頼にまとめて渡す。
 * 渡せるのは、新しいセッションなら同時に動かす数に空きがあるとき、引き継ぐセッションなら作業中でないとき。それまではワークフローのキューに溜める。
 */
const dispatch = (w: WorkflowDef): void => {
  const run = runOf(w.id);
  if (!run.queue.length) return;
  if (w.mode === 'new') {
    if (runningCount(w.id) >= (w.maxSessions ?? 1)) return;
    launch(w, run, takeAll(w, run));
    return touch();
  }
  // 起動を待っている間は、次のイベントを同じセッションへ送るため、id が分かるまで待つ。
  if (launching.get(w.id)) return;
  const sessionId = continuedSession(run);
  if (sessionId && readLiveSessions().get(sessionId)?.status === 'busy') return;
  if (sessionId) {
    run.lastRunAt = Date.now();
    const wave = unassignedWave(run);
    if (wave) wave.sessionId = sessionId;
    deliverTo(w, sessionId, takeAll(w, run));
  } else launch(w, run, takeAll(w, run));
  touch();
};

/** イベントの内容だけを溜め、依頼の形には渡すときにまとめる。 */
const enqueue = (w: WorkflowDef, event: string): void => {
  runOf(w.id).queue.push(event);
};

const hasFeedback = (w: WorkflowDef): boolean => w.triggers.some((t) => t.kind === 'feedback');

const UNMARKED_ERROR = `セッションが「${DONE_MARK}」を書かずに止まりました。セッションで続けるか、「実行」で次のウェーブを始めます。`;
const NOTHING_DONE_ERROR = `対応を終えた要望が「${DONE_MARK}」にありませんでした。新しい要望が届くか、「実行」で次のウェーブを始めます。`;

/** ワークフローに移したときに動いていた改善のセッション。プロセスが消えたら忘れる。 */
const legacyWaveAlive = (): boolean => {
  const run = state.improve?.run;
  if (run && isAlive(run.pid)) return true;
  if (run) state.improve = undefined;
  return false;
};

/**
 * 改善要望のトリガーを持つワークフローに、たまった要望を 1 つのウェーブとして積む。
 * 前のウェーブが終わるまでと、ウェーブが済んだ要望の無いまま止まって待機している間は積まない。アーカイブしたウェーブは止まったとみなす。
 */
const checkFeedback = (list: WorkflowDef[]): void => {
  const items = pendingItems();
  if (!list.length || !items.length || legacyWaveAlive()) return;
  for (const w of list) {
    const run = runOf(w.id);
    const wave = run.feedback;
    if (wave && !wave.stalled && wave.sessionId && state.sessions[wave.sessionId]?.archived) {
      wave.stalled = 'unmarked';
      run.error = UNMARKED_ERROR;
    }
    if (wave || run.queue.length || runningCount(w.id) > 0) continue;
    run.feedback = { items };
    enqueue(w, feedbackEvent(items));
  }
};

const activeFeedback = (): WorkflowDef[] => readWorkflows().workflows.filter((w) => w.enabled && hasFeedback(w));

/** 改善要望のトリガーをすぐ確かめ、積んだウェーブを渡す。 */
const kickFeedback = (): void => {
  const list = activeFeedback();
  checkFeedback(list);
  for (const w of list) dispatch(w);
  persist();
};

/** 要望が届いたら、済んだ要望が無いまま止まって待機していたウェーブを解き、次のウェーブを始める。 */
const feedbackArrived = (): void => {
  for (const w of activeFeedback()) {
    const run = runOf(w.id);
    if (run.feedback?.stalled !== 'nothing-done') continue;
    run.feedback = undefined;
    run.error = undefined;
  }
  kickFeedback();
};

/**
 * セッションが応答を終えたとき（Stop）に呼ぶ。改善要望のウェーブなら、最後の発言の「対応した要望:」で済んだ要望を移し、次のウェーブを始める。
 * 印が無ければ、バックグラウンドのコマンドを待っているか人間に任せているので、次のウェーブを始めずに待つ。後で印を書いて止まれば、そこで区切る。
 * ワークフローに移す前の改善のセッションも、同じく済んだ要望を移す。
 */
export const endFeedbackWave = (sessionId: string, message: string | undefined): void => {
  const meta = state.sessions[sessionId];
  if (meta?.improver) {
    if (finishItems(sessionId, message) !== undefined && state.improve?.run?.sessionId === sessionId) state.improve = undefined;
    return kickFeedback();
  }
  const run = meta?.workflow ? state.workflowRuns?.[meta.workflow] : undefined;
  const wave = run?.feedback;
  if (!run || wave?.sessionId !== sessionId) return;
  const done = finishItems(sessionId, message);
  if (done) {
    run.feedback = undefined;
    run.error = undefined;
  } else {
    wave.stalled = done === undefined ? 'unmarked' : 'nothing-done';
    run.error = done === undefined ? UNMARKED_ERROR : NOTHING_DONE_ERROR;
  }
  touch();
  kickFeedback();
};

/** レビュー依頼のトリガーを持つワークフローに、新しく届いた依頼を積む。初めて読んだときに届いていた依頼では動かない。 */
const checkReviews = async (list: WorkflowDef[], now: number): Promise<void> => {
  if (!list.length) return;
  const prs = (await githubInbox()).find((s) => s.title === DIRECT_SECTION)?.prs ?? [];
  for (const w of list) {
    const run = runOf(w.id);
    const fresh = run.seenPrs ? prs.filter((pr) => !run.seenPrs?.includes(pr.url)) : [];
    enqueueEvents(w, 'github-review', fresh.map((pr) => `${pr.repo}#${pr.number} ${pr.title}\n${pr.url}`), now);
    run.seenPrs = [...new Set([...prs.map((pr) => pr.url), ...(run.seenPrs ?? [])])].slice(0, SEEN_KEPT);
  }
};

// メンションの本文は依頼に写さず、Claude が URL から読む。
const EVENT_HEADER: Record<BurstKind, string> = {
  'github-review': 'GitHub で自分へのレビュー依頼が届きました:',
  'slack-mention': 'Slack で自分へのメンションが届きました:',
  'jira-mention': 'JIRA で自分へのメンションが届きました:',
};

const eventRequest = (kind: BurstKind, texts: string[]): string => `${EVENT_HEADER[kind]}\n${texts.join('\n')}`;

/** イベントを積む。ワークフローに burstMinutes があれば、まとめて渡すまで待たせる。 */
const enqueueEvents = (w: WorkflowDef, kind: BurstKind, texts: string[], now: number): void => {
  if (!texts.length) return;
  if (!w.burstMinutes) return texts.forEach((t) => enqueue(w, eventRequest(kind, [t])));
  const run = runOf(w.id);
  run.bursts ??= {};
  const burst = (run.bursts[kind] ??= { texts: [], lastAt: now });
  burst.texts.push(...texts);
  burst.lastAt = now;
};

/** 最後のイベントから burstMinutes が過ぎたバーストを、トリガーの種類ごとに 1 つの依頼にまとめて積む。 */
export const flushBursts = (w: WorkflowDef, run: WorkflowRun, now: number): string[] => {
  const flushed: string[] = [];
  for (const kind of Object.keys(run.bursts ?? {}) as BurstKind[]) {
    const burst = run.bursts?.[kind];
    if (!burst || now - burst.lastAt < (w.burstMinutes ?? 0) * 60_000) continue;
    delete run.bursts?.[kind];
    flushed.push(eventRequest(kind, burst.texts));
  }
  return flushed;
};

/** メンションのトリガーを持つワークフローに、新しく届いた Slack のメンションを積む。初めて読んだときに届いていたメンションでは動かない。 */
const checkMentions = async (list: WorkflowDef[], now: number): Promise<void> => {
  if (!list.length) return;
  const mentions = await slackMentions();
  for (const w of list) {
    const run = runOf(w.id);
    const fresh = run.seenMentions ? mentions.filter((m) => !run.seenMentions?.includes(m.url)) : [];
    // 検索の結果は新しい順なので、古いものから積む。
    enqueueEvents(w, 'slack-mention', fresh.reverse().map((m) => m.url), now);
    run.seenMentions = [...new Set([...mentions.map((m) => m.url), ...(run.seenMentions ?? [])])].slice(0, SEEN_KEPT);
    if (run.error?.startsWith(MENTION_ERROR)) run.error = undefined;
  }
};

/** JIRA のメンションのトリガーを持つワークフローに、新しく届いたコメントでのメンションを積む。初めて読んだときに届いていたメンションでは動かない。 */
const checkJiraMentions = async (list: WorkflowDef[], now: number): Promise<void> => {
  if (!list.length) return;
  const mentions = await jiraMentions();
  for (const w of list) {
    const run = runOf(w.id);
    const fresh = run.seenJira ? mentions.filter((m) => !run.seenJira?.includes(m.url)) : [];
    enqueueEvents(w, 'jira-mention', fresh.reverse().map((m) => m.url), now);
    run.seenJira = [...new Set([...mentions.map((m) => m.url), ...(run.seenJira ?? [])])].slice(0, SEEN_KEPT);
    if (run.error?.startsWith(JIRA_MENTION_ERROR)) run.error = undefined;
  }
};

/** kind のトリガーを持つワークフローについて check を動かし、読めなければ理由をそれぞれに残す。 */
const checkEvents = async (active: WorkflowDef[], kind: WorkflowTrigger['kind'], label: string, check: (list: WorkflowDef[]) => Promise<void>): Promise<void> => {
  const list = active.filter((w) => w.triggers.some((t) => t.kind === kind));
  try {
    await check(list);
  } catch (e) {
    for (const w of list) runOf(w.id).error = `${label}: ${(e as Error).message}`;
  }
};

const tick = async (): Promise<void> => {
  const now = Date.now();
  const from = state.workflowCheckedAt ?? now;
  state.workflowCheckedAt = now;
  const active = readWorkflows().workflows.filter((w) => w.enabled);
  for (const w of active) {
    const due = w.triggers.find((t): t is Extract<WorkflowTrigger, { kind: 'schedule' }> => t.kind === 'schedule' && scheduleDue(t, from, now));
    if (due) enqueue(w, `時刻のトリガー（${due.at}）で動きました。`);
  }
  await checkEvents(active, 'github-review', 'レビュー依頼を読めません', (list) => checkReviews(list, now));
  await checkEvents(active, 'slack-mention', MENTION_ERROR, (list) => checkMentions(list, now));
  await checkEvents(active, 'jira-mention', JIRA_MENTION_ERROR, (list) => checkJiraMentions(list, now));
  checkFeedback(active.filter(hasFeedback));
  for (const w of active) {
    const run = runOf(w.id);
    for (const text of flushBursts(w, run, now)) enqueue(w, text);
    dispatch(w);
  }
  persist();
};

/**
 * 改善要望の自動実装の設定を、改善要望のトリガーを持つワークフローの定義へ移す。既にそのワークフローがあれば、設定は捨てる。
 * 定義を読めないときは、人間が直すまで移さない。動いていたウェーブは、終わるまで覚えておく。
 */
const migrateImprove = (): void => {
  const legacy = state.improve;
  if (!legacy || (legacy.enabled === undefined && !legacy.model && !legacy.effort)) return;
  const { workflows, error } = readWorkflows();
  if (error) return;
  if (!workflows.some(hasFeedback)) writeWorkflowDir(improveWorkflow(workflows.some((w) => w.id === 'improve') ? 'improve-feedback' : 'improve', legacy));
  state.improve = legacy.run && isAlive(legacy.run.pid) ? { run: { sessionId: legacy.run.sessionId, pid: legacy.run.pid } } : undefined;
  persist();
};

/** サーバーを起動したときに呼ぶ。止まっていた間に過ぎた時刻のトリガーも、丸 1 日前の分まで拾う。 */
export const startWorkflows = (): void => {
  migrateLegacyFile();
  migrateImprove();
  onFeedback(feedbackArrived);
  void tick();
  setInterval(() => void tick(), TICK_MS).unref();
};

const findDef = (id: string): WorkflowDef => {
  const w = readWorkflows().workflows.find((d) => d.id === id);
  if (!w) throw new Error(`ワークフロー ${id} がありません`);
  return w;
};

/** 画面の「今すぐ実行」。OFF のワークフローも動かす。改善要望のトリガーを持つなら、待機を解いて、たまった要望でウェーブを始める。 */
export const runWorkflow = (id: string): void => {
  const w = findDef(id);
  const run = runOf(w.id);
  let started = false;
  if (hasFeedback(w) && pendingItems().length) {
    run.feedback = undefined;
    run.error = undefined;
    checkFeedback([w]);
    started = !!run.feedback;
  }
  if (!started) enqueue(w, '人間が「今すぐ実行」で動かしました。');
  dispatch(w);
  persist();
};

/** 書き換える前の workflow.json と agent。 */
const filesOf = (id: string) => {
  const dir = path.join(WORKFLOWS_DIR, id);
  if (!ID_PATTERN.test(id) || !fs.existsSync(path.join(dir, SETTINGS_FILE))) throw new Error(`ワークフロー ${id} がありません`);
  const settingsFile = path.join(dir, SETTINGS_FILE);
  const agentFile = agentPath(dir, id);
  const agent = fs.existsSync(agentFile) ? parseAgent(fs.readFileSync(agentFile, 'utf8')) : { lines: [], body: '' };
  return { dir, settingsFile, agentFile, agent, settings: JSON.parse(fs.readFileSync(settingsFile, 'utf8')) as Record<string, unknown> };
};

const writeSettings = (file: string, settings: Record<string, unknown>): void => {
  fs.writeFileSync(file, `${JSON.stringify(settings, null, 2)}\n`);
  forgetWorkflows();
  touch();
};

/** ON と OFF を切り替える。 */
export const setWorkflowEnabled = (id: string, enabled: boolean): void => {
  const { settingsFile, settings } = filesOf(id);
  writeSettings(settingsFile, { ...settings, enabled });
};

// 画面の「設定」で直す項目のうち、workflow.json に書くもの。id と ON と OFF は直さない。指示と model と effort は agent に書く。
const EDITABLE = ['name', 'cwd', 'theme', 'triggers', 'burstMinutes', 'mode', 'compact', 'compactWindow', 'maxSessions', 'account'] as const;

/**
 * 画面の「設定」で直した項目を書き込む。送らなかった項目は消して既定に戻す。読めない値なら書かずに理由を返す。
 * workflow.json と agent の、画面が触らない項目と書き方は、人間とエージェントが書いたまま残す。
 */
export const updateWorkflow = (id: string, body: unknown): void => {
  if (!isObject(body)) throw new Error('設定がオブジェクトではありません');
  const { dir, settingsFile, agentFile, agent, settings } = filesOf(id);
  const next = { ...settings };
  for (const key of EDITABLE) {
    if (body[key] === undefined || body[key] === null) delete next[key];
    else next[key] = body[key];
  }
  const str = (v: unknown): string | undefined => (typeof v === 'string' && v ? v : undefined);
  const agentText = formatAgent(agent, { model: str(body.model), effort: str(body.effort), prompt: typeof body.prompt === 'string' ? body.prompt : '' });
  defOf(id, dir, next, parseAgent(agentText));
  fs.writeFileSync(agentFile, agentText);
  writeSettings(settingsFile, next);
};

const consultPrompt = (target: WorkflowDef | undefined): string => `${MARK} 人間と相談して、${target ? `ワークフロー「${target.name}」（id: ${target.id}）を変えて` : '新しいワークフローを作って'}ください。
ワークフローは、トリガーで Claude Code のセッションを動かす、手元で動くルーティンです。1 つのワークフローは ${WORKFLOWS_DIR}/<id>/ の 1 つのフォルダーにまとめます。ダッシュボードは書き換えを読み直して、すぐに使います。

## 形
フォルダー名が id です。英数字とハイフンで、ほかのフォルダーと重ならないものにしてください。ほかのワークフローは変えないでください。
- <id>.md: Claude Code の agent として書く、セッションへの指示。普段のセッションからもサブエージェントとして呼べる
  - フロントマター: name（id と同じ）、description（どんなときに使う agent か）、model（${MODELS.join(' / ')}）、effort（${EFFORT_LEVELS.join(' / ')}）。model と effort は省くと Claude Code の設定に従う
  - 本文: セッションへの依頼。ダッシュボードはイベントの内容だけを送り、本文はこのファイルの場所で指す
- 補助のスクリプトなど: 同じフォルダーに置ける。セッションは <id>.md の場所から辿る
- workflow.json: 次の項目を持つ JSON
- name: 画面とセッションのバッジに出す名前
- enabled: true で動く。新しく作るときは、人間が良いと言うまで false にする
- cwd: セッションを動かす作業ディレクトリ（絶対パス）
- theme: 起動したセッションを入れるテーマ。省くと作業ディレクトリのリポジトリ名になる
- triggers: 次の配列
  - { "kind": "schedule", "at": "09:00", "days": [1, 2, 3, 4, 5] }: 時刻で動く。days は 0 が日曜で、省くと毎日
  - { "kind": "github-review" }: GitHub で自分へのレビュー依頼が新しく届いたときに動く
  - { "kind": "slack-mention" }: Slack で自分へのメンションが新しく届いたときに動く。Claude Code の /mcp で Slack に接続している必要がある
  - { "kind": "jira-mention" }: JIRA のコメントで自分へのメンションが新しく届いたときに動く
  - { "kind": "feedback" }: ダッシュボードへの改善要望がたまっているときに動く。前のウェーブを終えてから、たまった要望を 1 つのウェーブとして渡す。要望の一覧と、最後に「${DONE_MARK}」の行に対応を終えた要望を書く約束は、ダッシュボードが依頼の前に書き足す
- burstMinutes: レビュー依頼とメンションが続けて届いたとき、最後のものからこの分数だけ次が届かなくなるまで待って、トリガーの種類ごとに 1 つの依頼にまとめる。省くと 1 件ずつ動く
- mode: "new" はイベントごとに新しいセッションを起動する。"continue" は 1 つのセッションを引き継いでイベントをチャットに送り、セッションが無いかアーカイブされていたら新しく起動する
- compact: mode が "continue" のとき、true なら、イベントを送る前に毎回前回までの会話を compact する
- account: セッションを動かす追加アカウントの名前（${DATA_DIR}/accounts/<名前>.json）。省くとプライマリアカウントで動く。追加アカウントには claude.ai のコネクタが無いので、コネクタを使うワークフローでは省く
- compactWindow: mode が "continue" のとき、会話がこのトークン数（${COMPACT_WINDOW_MIN}〜${COMPACT_WINDOW_MAX}）に達したら自動で compact する。省くと Claude Code の設定に従う。次にセッションを起動したときから効く
- maxSessions: mode が "new" のとき、同時に動かすセッションの上限（既定 1）。上限を超えたイベントは、空くまで待たせる

## 進め方
まず、何をいつ動かしたいかを人間に聞いてください。決まったらフォルダーにファイルを書き、書いた内容を短く伝えて止まってください。`;

/** エージェントと相談してワークフローを作るか変えるセッションを起動し、その id を返す。id を渡すと、そのワークフローを変える相談にする。 */
export const consultWorkflow = async (id?: string): Promise<{ sessionId: string }> => {
  const target = id ? findDef(id) : undefined;
  fs.mkdirSync(DATA_DIR, { recursive: true });
  const meta = await startTrackedSession(DATA_DIR, consultPrompt(target));
  meta.title = target ? `ワークフロー「${target.name}」の相談` : 'ワークフローの相談';
  touch();
  return { sessionId: meta.id };
};

const infoOf = (w: WorkflowDef, live: ReturnType<typeof readLiveSessions>): WorkflowInfo => {
  const run = state.workflowRuns?.[w.id];
  return { ...w, running: runningCount(w.id, live), queued: run?.queue.length ?? 0, sessionId: run && w.mode === 'continue' ? continuedSession(run) : undefined, lastRunAt: run?.lastRunAt, error: run?.error };
};

export const workflowState = (): WorkflowState => {
  const { workflows, error } = readWorkflows();
  const live = readLiveSessions();
  return { dir: WORKFLOWS_DIR, workflows: workflows.map((w) => infoOf(w, live)), error };
};

/** 改善要望への対応の様子。改善要望のトリガーを持つワークフローのうち、ON のものを優先して 1 つ見せる。 */
export const improveState = (): ImproveState => {
  const items = pendingItems();
  const w = readWorkflows().workflows.filter(hasFeedback).sort((a, b) => Number(b.enabled) - Number(a.enabled))[0];
  const legacy = legacyWaveAlive() ? state.improve?.run : undefined;
  const run = w ? state.workflowRuns?.[w.id] : undefined;
  const wave = legacy ? undefined : run?.feedback;
  const running = !!legacy || (!!wave && !wave.stalled);
  const inWave = running ? (wave?.items ?? items) : [];
  return {
    pending: items.filter((f) => !inWave.includes(f)).length,
    workflow: w && {
      id: w.id,
      name: w.name,
      enabled: w.enabled,
      running,
      sessionId: legacy?.sessionId ?? wave?.sessionId,
      current: running ? `${inWave.length} 件の要望のウェーブ` : undefined,
      error: run?.error,
    },
  };
};

/** セッションのバッジに出す、起動したワークフローの名前。定義を消していたら id を出す。 */
export const workflowNameOf = (id: string | undefined): string | undefined => (id ? (readWorkflows().workflows.find((w) => w.id === id)?.name ?? id) : undefined);
