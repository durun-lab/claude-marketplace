// ワークフローアプリ。Claude Code のルーティンのように、トリガーでセッションを動かす。ルーティンと違い、手元のダッシュボードで動く。
// 定義は workflows.json に置き、人間とエージェントが書く。ダッシュボードは書き換えを読み直してすぐ使い、ON と OFF の切り替えだけを書き込む。
// 動きは 1 分ごとに確かめる。時刻のトリガーはその間に過ぎた時刻で、レビュー依頼のトリガーは GitHub の Inbox に新しく現れた PR で、メンションのトリガーは Slack の検索に新しく現れた発言で動く。
// 改善要望のトリガーは、要望が届いたときと前のウェーブを終えたときにもすぐ確かめる。
import fs from 'node:fs';
import path from 'node:path';
import { EFFORT_LEVELS, type ImproveState, MODELS, type WorkflowDef, type WorkflowInfo, type WorkflowState, type WorkflowTrigger } from '../shared/types.ts';
import { isAlive, readLiveSessions } from './claude.ts';
import { COMPACT_COMMAND, compactSession, sendMessage, startTrackedSession } from './deliver.ts';
import { DONE_MARK, feedbackEvent, finishItems, improveWorkflow, onFeedback, pendingItems } from './improve.ts';
import { githubInbox } from './integrations.ts';
import { slackMentions } from './slack.ts';
import { DATA_DIR, type FeedbackWave, newId, persist, state, touch, type WorkflowRun } from './store.ts';

export const WORKFLOWS_FILE = path.join(DATA_DIR, 'workflows.json');
const TICK_MS = 60_000;
// スリープ明けなどで長く確かめていなかったときも、丸 1 日より前の時刻では動かさない。
const LOOKBACK_MS = 24 * 60 * 60_000;
const DAY_MS = 24 * 60 * 60_000;
const SEEN_KEPT = 500;
const MARK = '[durun-dashboard ワークフロー]';
const DIRECT_SECTION = 'Needs your review';
const MENTION_ERROR = 'Slack のメンションを読めません';
const ID_PATTERN = /^[\w-]+$/;
const TIME_PATTERN = /^([01]\d|2[0-3]):[0-5]\d$/;
// 溜まったイベントを 1 つの依頼にまとめるときの区切り。
const SEPARATOR = '\n\n---\n\n';

type Parsed = { workflows: WorkflowDef[]; error?: string };

const isObject = (v: unknown): v is Record<string, unknown> => typeof v === 'object' && v !== null && !Array.isArray(v);

const triggerOf = (raw: unknown): WorkflowTrigger => {
  if (isObject(raw) && raw.kind === 'github-review') return { kind: 'github-review' };
  if (isObject(raw) && raw.kind === 'slack-mention') return { kind: 'slack-mention' };
  if (isObject(raw) && raw.kind === 'feedback') return { kind: 'feedback' };
  if (isObject(raw) && raw.kind === 'schedule' && typeof raw.at === 'string' && TIME_PATTERN.test(raw.at)) {
    const days = Array.isArray(raw.days) ? raw.days.filter((d): d is number => Number.isInteger(d) && d >= 0 && d <= 6) : undefined;
    return { kind: 'schedule', at: raw.at, ...(days?.length ? { days } : {}) };
  }
  throw new Error(`トリガー ${JSON.stringify(raw)} を読めません`);
};

const defOf = (raw: unknown, i: number): WorkflowDef => {
  if (!isObject(raw)) throw new Error(`${i + 1} 番目がオブジェクトではありません`);
  const { id, name, enabled, cwd, prompt, triggers, mode, maxSessions, compact, model, effort } = raw;
  if (typeof id !== 'string' || !ID_PATTERN.test(id)) throw new Error(`${i + 1} 番目の id は英数字とハイフンで書いてください`);
  if (typeof cwd !== 'string' || !path.isAbsolute(cwd)) throw new Error(`${id} の cwd は絶対パスで書いてください`);
  if (typeof prompt !== 'string' || !prompt.trim()) throw new Error(`${id} の prompt が空です`);
  return {
    id,
    name: typeof name === 'string' && name.trim() ? name.trim() : id,
    enabled: enabled === true,
    cwd,
    prompt,
    triggers: Array.isArray(triggers) ? triggers.map(triggerOf) : [],
    mode: mode === 'continue' ? 'continue' : 'new',
    ...(mode === 'continue' && compact === true ? { compact: true } : {}),
    ...(Number.isInteger(maxSessions) && (maxSessions as number) > 0 ? { maxSessions: maxSessions as number } : {}),
    ...(typeof model === 'string' && (MODELS as readonly string[]).includes(model) ? { model } : {}),
    ...(typeof effort === 'string' && (EFFORT_LEVELS as readonly string[]).includes(effort) ? { effort } : {}),
  };
};

/** workflows.json の本文を読む。1 つでも読めない定義があれば、どれも動かさずに理由を返す。書きかけの定義で動かさないため。 */
export const parseWorkflows = (text: string): Parsed => {
  try {
    const raw: unknown = JSON.parse(text);
    const list = isObject(raw) ? raw.workflows : undefined;
    if (!Array.isArray(list)) throw new Error('workflows の配列がありません');
    const workflows = list.map(defOf);
    const dup = workflows.find((w, i) => workflows.findIndex((o) => o.id === w.id) !== i);
    if (dup) throw new Error(`id ${dup.id} が重なっています`);
    return { workflows };
  } catch (e) {
    return { workflows: [], error: `${path.basename(WORKFLOWS_FILE)} を読めません: ${(e as Error).message}` };
  }
};

let cache: { mtime: number; parsed: Parsed } | undefined;

/** 定義を読む。書き換えたときだけ読み直す。ファイルが無ければ、定義は無い。 */
const readWorkflows = (): Parsed => {
  let mtime: number;
  try {
    mtime = fs.statSync(WORKFLOWS_FILE).mtimeMs;
  } catch {
    return { workflows: [] };
  }
  if (cache?.mtime !== mtime) cache = { mtime, parsed: parseWorkflows(fs.readFileSync(WORKFLOWS_FILE, 'utf8')) };
  return cache.parsed;
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

const promptFor = (w: WorkflowDef, event: string): string => `${MARK}「${w.name}」 ${event}\n\n${w.prompt}`;

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
  startTrackedSession(w.cwd, text, w.model, w.effort)
    .then((meta) => {
      Object.assign(meta, { workflow: w.id, title: w.name });
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
 * 引き継いでいるセッションへイベントを送る。動いているセッションにはチャットのキューに積み、入力待ちになったら送らせる。
 * 止まっているセッションには、まとめて送って再開させる。
 * compact なら、前回までの会話を compact してから送る。止まっているセッションは compact で再開させ、イベントはその後ろのキューに積む。
 */
const deliverTo = (w: WorkflowDef, sessionId: string, texts: string[]): void => {
  const live = readLiveSessions().has(sessionId);
  if (live) return queueTo(sessionId, w.compact ? [COMPACT_COMMAND, ...texts] : texts);
  if (!w.compact) return void sendMessage(sessionId, texts.join(SEPARATOR));
  compactSession(sessionId, state.sessions[sessionId]?.cwd ?? w.cwd)
    .then(() => queueTo(sessionId, texts))
    // compact できなくても、イベントは落とさずに送る。
    .catch((e: Error) => {
      runOf(w.id).error = `セッションを compact できません: ${e.message}`;
      sendMessage(sessionId, texts.join(SEPARATOR));
    })
    .finally(touch);
};

/** 待たせているイベントを、動かせるだけセッションに渡す。 */
const dispatch = (w: WorkflowDef): void => {
  const run = runOf(w.id);
  if (!run.queue.length) return;
  if (w.mode === 'new') {
    const max = w.maxSessions ?? 1;
    for (let n = runningCount(w.id); n < max && run.queue.length; n++) launch(w, run, run.queue.shift() as string);
    return touch();
  }
  // 起動を待っている間は、次のイベントを同じセッションへ送るため、id が分かるまで待つ。
  if (launching.get(w.id)) return;
  const texts = run.queue.splice(0);
  const sessionId = continuedSession(run);
  if (sessionId) {
    run.lastRunAt = Date.now();
    const wave = unassignedWave(run);
    if (wave) wave.sessionId = sessionId;
    deliverTo(w, sessionId, texts);
  } else launch(w, run, texts.join(SEPARATOR));
  touch();
};

const enqueue = (w: WorkflowDef, event: string): void => {
  runOf(w.id).queue.push(promptFor(w, event));
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
const checkReviews = async (list: WorkflowDef[]): Promise<void> => {
  if (!list.length) return;
  const prs = (await githubInbox()).find((s) => s.title === DIRECT_SECTION)?.prs ?? [];
  for (const w of list) {
    const run = runOf(w.id);
    const fresh = run.seenPrs ? prs.filter((pr) => !run.seenPrs?.includes(pr.url)) : [];
    for (const pr of fresh) enqueue(w, `GitHub で自分へのレビュー依頼が届きました: ${pr.repo}#${pr.number} ${pr.title}\n${pr.url}`);
    run.seenPrs = [...new Set([...prs.map((pr) => pr.url), ...(run.seenPrs ?? [])])].slice(0, SEEN_KEPT);
  }
};

/** メンションのトリガーを持つワークフローに、新しく届いたメンションを積む。初めて読んだときに届いていたメンションでは動かない。 */
const checkMentions = async (list: WorkflowDef[]): Promise<void> => {
  if (!list.length) return;
  const mentions = await slackMentions();
  for (const w of list) {
    const run = runOf(w.id);
    const fresh = run.seenMentions ? mentions.filter((m) => !run.seenMentions?.includes(m.url)) : [];
    // 検索の結果は新しい順なので、古いものから積む。
    for (const m of fresh.reverse()) enqueue(w, `Slack で自分へのメンションが届きました（${m.channel}）:
${m.text}
${m.url}`);
    run.seenMentions = [...new Set([...mentions.map((m) => m.url), ...(run.seenMentions ?? [])])].slice(0, SEEN_KEPT);
    if (run.error?.startsWith(MENTION_ERROR)) run.error = undefined;
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
  await checkEvents(active, 'github-review', 'レビュー依頼を読めません', checkReviews);
  await checkEvents(active, 'slack-mention', MENTION_ERROR, checkMentions);
  checkFeedback(active.filter(hasFeedback));
  for (const w of active) dispatch(w);
  persist();
};

/** 定義を書き足す。ファイルが無ければ作る。 */
const appendWorkflow = (def: object): void => {
  const raw = (fs.existsSync(WORKFLOWS_FILE) ? JSON.parse(fs.readFileSync(WORKFLOWS_FILE, 'utf8')) : { workflows: [] }) as { workflows: object[] };
  raw.workflows.push(def);
  fs.mkdirSync(DATA_DIR, { recursive: true });
  fs.writeFileSync(WORKFLOWS_FILE, `${JSON.stringify(raw, null, 2)}\n`);
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
  if (!workflows.some(hasFeedback)) appendWorkflow(improveWorkflow(workflows.some((w) => w.id === 'improve') ? 'improve-feedback' : 'improve', legacy));
  state.improve = legacy.run && isAlive(legacy.run.pid) ? { run: { sessionId: legacy.run.sessionId, pid: legacy.run.pid } } : undefined;
  persist();
};

/** サーバーを起動したときに呼ぶ。止まっていた間に過ぎた時刻のトリガーも、丸 1 日前の分まで拾う。 */
export const startWorkflows = (): void => {
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

/** ON と OFF を切り替える。ほかの項目と書き方は、人間とエージェントが書いたまま残す。 */
export const setWorkflowEnabled = (id: string, enabled: boolean): void => {
  findDef(id);
  const raw = JSON.parse(fs.readFileSync(WORKFLOWS_FILE, 'utf8')) as { workflows: Record<string, unknown>[] };
  for (const w of raw.workflows) if (w.id === id) w.enabled = enabled;
  fs.writeFileSync(WORKFLOWS_FILE, `${JSON.stringify(raw, null, 2)}\n`);
  touch();
};

const consultPrompt = (target: WorkflowDef | undefined): string => `${MARK} 人間と相談して、${target ? `ワークフロー「${target.name}」（id: ${target.id}）を変えて` : '新しいワークフローを作って'}ください。
ワークフローは、トリガーで Claude Code のセッションを動かす、手元で動くルーティンです。定義は ${WORKFLOWS_FILE} に JSON で書きます。ダッシュボードは書き換えを読み直して、すぐに使います。

## 形
{ "workflows": [ { ... }, ... ] }。ファイルが無ければ作ってください。ほかのワークフローは変えないでください。
- id: 英数字とハイフン。ほかと重ならないもの
- name: 画面とセッションのバッジに出す名前
- enabled: true で動く。新しく作るときは、人間が良いと言うまで false にする
- cwd: セッションを動かす作業ディレクトリ（絶対パス）
- prompt: セッションへの依頼。イベントの内容は、ダッシュボードが依頼の前に書き足す
- triggers: 次の配列
  - { "kind": "schedule", "at": "09:00", "days": [1, 2, 3, 4, 5] }: 時刻で動く。days は 0 が日曜で、省くと毎日
  - { "kind": "github-review" }: GitHub で自分へのレビュー依頼が新しく届いたときに動く
  - { "kind": "slack-mention" }: Slack で自分へのメンションが新しく届いたときに動く。Claude Code の /mcp で Slack に接続している必要がある
  - { "kind": "feedback" }: ダッシュボードへの改善要望がたまっているときに動く。前のウェーブを終えてから、たまった要望を 1 つのウェーブとして渡す。要望の一覧と、最後に「${DONE_MARK}」の行に対応を終えた要望を書く約束は、ダッシュボードが依頼の前に書き足す
- mode: "new" はイベントごとに新しいセッションを起動する。"continue" は 1 つのセッションを引き継いでイベントをチャットに送り、セッションが無いかアーカイブされていたら新しく起動する
- compact: mode が "continue" のとき、true なら、イベントを送る前に毎回前回までの会話を compact する
- maxSessions: mode が "new" のとき、同時に動かすセッションの上限（既定 1）。上限を超えたイベントは、空くまで待たせる
- model（${MODELS.join(' / ')}）、effort（${EFFORT_LEVELS.join(' / ')}）: 省くと Claude Code の設定に従う

## 進め方
まず、何をいつ動かしたいかを人間に聞いてください。決まったらファイルを書き、書いた内容を短く伝えて止まってください。`;

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
  return { file: WORKFLOWS_FILE, workflows: workflows.map((w) => infoOf(w, live)), error };
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
