// エージェントが MCP のツールで登録して進めるタスク。人間がダッシュボードで進み具合を見るためのもの。
// Claude Code のタスクのツールは一覧をコンテキストに載せ続けるので、ここでは登録と更新で一覧を返さず、list_tasks で読んだときだけ返す。
import type { AgentTask } from '../shared/types.ts';

export const TASK_STATUSES = ['pending', 'in_progress', 'completed'] as const;
const TASK_LABEL: Record<AgentTask['status'], string> = { pending: '未着手', in_progress: '作業中', completed: '完了' };

/** agentTaskSeq は最後に振った id。消したタスクの id を使い直さないよう、タスクとは別に覚える。 */
/** rootTask は、親セッションから任されたタスクの id。子セッションで登録するタスクは、親を省くとこの下に置く。 */
type Owner = { agentTasks?: AgentTask[]; agentTaskSeq?: number; rootTask?: string };
type Result = { text: string; isError?: boolean; changed?: boolean };
/**
 * create_tasks に渡すタスク。題だけの文字列でも受け付ける。
 * blocked_by は待つタスクの id、parent は手順として下に置く親のタスクの id。同じ呼び出しで登録するタスクは、tasks の何番目かを 1 から数えて #2 のように書く。
 * ticket は PR、Issue、JIRA の課題の URL か課題のキー。チケットのタスクは subject を省ける。
 */
export type NewTask = string | { subject?: unknown; description?: unknown; blocked_by?: unknown; parent?: unknown; ticket?: unknown };
/** チケットを、バッジと同じ URL と、題を省いたときに出す名前にそろえる。チケットでなければ undefined。 */
export type TicketOf = (text: string) => { url: string; label: string } | undefined;

const ids = (v: unknown): string[] => (Array.isArray(v) ? v.filter((x): x is string => typeof x === 'string' && !!x.trim()).map((x) => x.trim()) : []);

/** 待っているタスクのうち、まだ終わっていないものを添える。 */
const line = (t: AgentTask, tasks: AgentTask[]): string => {
  const waiting = (t.blockedBy ?? []).filter((id) => tasks.find((u) => u.id === id)?.status !== 'completed');
  return `- ${t.id} [${TASK_LABEL[t.status]}] ${t.subject}${t.status !== 'completed' && waiting.length ? `（${waiting.join('・')} を待つ）` : ''}`;
};

/** from から blockedBy を辿って to に着くか。着くなら、to が from を待つと輪になる。 */
const reaches = (tasks: AgentTask[], from: string, to: string, seen = new Set<string>()): boolean => {
  if (from === to) return true;
  if (seen.has(from)) return false;
  seen.add(from);
  return (tasks.find((t) => t.id === from)?.blockedBy ?? []).some((id) => reaches(tasks, id, to, seen));
};

/** task の親を parent にすると、親を辿って task に戻るなら、輪になる。 */
const parentLoops = (tasks: AgentTask[], task: string, parent: string): boolean => {
  const seen = new Set<string>();
  for (let id: string | undefined = parent; id; id = tasks.find((t) => t.id === id)?.parent) {
    if (id === task) return true;
    if (seen.has(id)) return false;
    seen.add(id);
  }
  return false;
};

const parentError = (tasks: AgentTask[], task: string, parent: string): string | undefined =>
  !tasks.some((t) => t.id === parent) ? `親のタスクの id（${parent}）が見つかりません。` : parentLoops(tasks, task, parent) ? `タスク ${task} の親を ${parent} にすると、親子が輪になります。` : undefined;

/** task が waits を待つようにできないなら、その理由を返す。 */
const blockError = (tasks: AgentTask[], task: string, waits: string[]): string | undefined => {
  const unknown = waits.filter((id) => !tasks.some((t) => t.id === id));
  if (unknown.length) return `待つタスクの id（${unknown.join('、')}）が見つかりません。`;
  const loop = waits.find((id) => reaches(tasks, id, task));
  return loop ? `タスク ${task} が ${loop} を待つと、待ち合いが輪になります。` : undefined;
};
const text = (v: unknown): string | undefined => (typeof v === 'string' && v.trim() ? v.trim() : undefined);

/** 登録したタスクの id を返す。id はセッションの中の通し番号で、消したタスクの番号は使い直さない。 */
export const addTasks = (owner: Owner, given: NewTask[], opts: { now?: number; ticketOf?: TicketOf } = {}): Result => {
  const now = opts.now ?? Date.now();
  const items: { subject: string; description?: string; waits: string[]; parent?: string; ticket?: string }[] = [];
  for (const t of given) {
    const o = typeof t === 'string' ? { subject: t } : (t ?? {});
    const typed = text(o.ticket);
    const ticket = typed ? opts.ticketOf?.(typed) : undefined;
    if (typed && !ticket) return { text: `登録しませんでした。ticket（${typed}）は、PR、Issue、JIRA の課題の URL か課題のキーではありません。`, isError: true };
    const subject = text(o.subject) ?? ticket?.label;
    if (subject) items.push({ subject, description: text(o.description), waits: ids(o.blocked_by), parent: text(o.parent) ?? (owner.agentTasks?.some((u) => u.id === owner.rootTask) ? owner.rootTask : undefined), ticket: ticket?.url });
  }
  if (!items.length) return { text: 'tasks に、登録するタスクを 1 つ以上渡してください。', isError: true };
  const before = owner.agentTaskSeq ?? 0;
  // #n は、この呼び出しで n 番目に登録するタスクの id に読み替える。
  const resolve = (id: string): string => (/^#\d+$/.test(id) && Number(id.slice(1)) <= items.length ? String(before + Number(id.slice(1))) : id);
  const added = items.map(({ subject, description, waits, parent, ticket }, i): AgentTask => {
    const blockedBy = [...new Set(waits.map(resolve))];
    return { id: String(before + i + 1), subject, description, status: 'pending', createdAt: now, ...(blockedBy.length ? { blockedBy } : {}), ...(parent ? { parent: resolve(parent) } : {}), ...(ticket ? { ticket } : {}) };
  });
  const tasks = [...(owner.agentTasks ?? []), ...added];
  for (const t of added) {
    const error = (t.blockedBy && blockError(tasks.filter((u) => u !== t), t.id, t.blockedBy)) || (t.parent && parentError(tasks, t.id, t.parent));
    if (error) return { text: `登録しませんでした。${error}`, isError: true };
  }
  Object.assign(owner, { agentTasks: tasks, agentTaskSeq: before + added.length });
  return { text: `登録しました。\n${added.map((t) => line(t, tasks)).join('\n')}`, changed: true };
};

/** parent に空の文字列を渡すと、親から外していちばん外に出す。 */
export type TaskChange = { status?: string; subject?: string; description?: string; addBlockedBy?: unknown; removeBlockedBy?: unknown; parent?: string };

/** status に deleted を渡すと消す。着手と完了の時刻は、その状態に変えた時刻を覚え、未着手に戻したら忘れる。 */
export const changeTask = (owner: Owner, id: string | undefined, change: TaskChange, now = Date.now()): Result => {
  const task = owner.agentTasks?.find((t) => t.id === id);
  if (!task) return { text: 'id に、create_tasks か list_tasks が返したタスクの id を渡してください。', isError: true };
  const { status } = change;
  if (status === 'deleted') {
    // 消したタスクを待っていたタスクは待つのをやめ、その手順だったタスクは親から外す。
    owner.agentTasks = owner.agentTasks
      ?.filter((t) => t !== task)
      .map((t) => ({ ...t, blockedBy: t.blockedBy?.filter((id) => id !== task.id), parent: t.parent === task.id ? undefined : t.parent }));
    return { text: '消しました。', changed: true };
  }
  const isStatus = (v: string): v is AgentTask['status'] => (TASK_STATUSES as readonly string[]).includes(v);
  const next = status && isStatus(status) ? status : undefined;
  if (status && !next) return { text: `status には ${[...TASK_STATUSES, 'deleted'].join('、')} のどれかを渡してください。`, isError: true };
  const adding = ids(change.addBlockedBy);
  const parent = change.parent?.trim();
  const error = (adding.length ? blockError(owner.agentTasks ?? [], task.id, adding) : undefined) || (parent ? parentError(owner.agentTasks ?? [], task.id, parent) : undefined);
  if (error) return { text: `更新しませんでした。${error}`, isError: true };
  if (change.parent !== undefined) task.parent = parent || undefined;
  const removing = new Set(ids(change.removeBlockedBy));
  const blockedBy = [...new Set([...(task.blockedBy ?? []), ...adding])].filter((id) => !removing.has(id));
  task.blockedBy = blockedBy.length ? blockedBy : undefined;
  task.updatedAt = now;
  if (next && next !== task.status) {
    task.status = next;
    if (next === 'pending') Object.assign(task, { startedAt: undefined, completedAt: undefined });
    if (next === 'in_progress') Object.assign(task, { startedAt: task.startedAt ?? now, completedAt: undefined });
    if (next === 'completed') Object.assign(task, { startedAt: task.startedAt ?? now, completedAt: now });
  }
  task.subject = text(change.subject) ?? task.subject;
  if (change.description !== undefined) task.description = text(change.description);
  return { text: '更新しました。', changed: true };
};

/**
 * 親セッションのタスクを子セッションに任せる。タスクに子セッションを結び、子セッションにはそのタスクを写したルートのタスクを作る。
 * 起動するときにも、起動した後にも結べる。子セッションが 1 つ任されたタスクを持つ間は、ほかのタスクを結ばない。
 * ルートのタスクの実体は親のタスクで、子セッションが題、説明、状態を変えると親のタスクが変わる（mcp の update_task）。
 */
export const delegateTask = (parent: Owner & { id: string }, taskId: string, child: Owner & { id: string }, now = Date.now()): Result => {
  const task = parent.agentTasks?.find((t) => t.id === taskId);
  if (!task) return { text: 'task に、create_tasks か list_tasks が返したタスクの id を渡してください。', isError: true };
  const held = child.agentTasks?.find((t) => t.id === child.rootTask);
  if (held) return { text: `その子セッションは、すでに「${held.subject}」を任されています。`, isError: true };
  // 後から結ぶときは、子セッションが登録していたタスクのうち、親の無いものをルートの下に入れる。
  const id = String((child.agentTaskSeq ?? 0) + 1);
  const { subject, description, ticket } = task;
  const root: AgentTask = { id, subject, description, ticket, status: 'in_progress', createdAt: now, startedAt: now, delegatedFrom: { session: parent.id, task: taskId } };
  const tasks = (child.agentTasks ?? []).map((t) => (t.parent ? t : { ...t, parent: id }));
  Object.assign(child, { agentTasks: [root, ...tasks], agentTaskSeq: Number(id), rootTask: id });
  Object.assign(task, { delegatedTo: child.id, updatedAt: now });
  if (task.status === 'pending') Object.assign(task, { status: 'in_progress', startedAt: now });
  return { text: '任せました。', changed: true };
};

type Sessions = Record<string, (Owner & { id: string; movedTo?: string }) | undefined>;

/** 会話を引き継いだ複製へ移っていたら、複製をたどる。 */
const follow = (sessions: Sessions, id: string): (Owner & { id: string }) | undefined => {
  let m = sessions[id];
  for (const seen = new Set<string>(); m?.movedTo && sessions[m.movedTo] && !seen.has(m.id); m = sessions[m.movedTo]) seen.add(m.id);
  return m;
};

/** 子セッションが任されたタスクの実体。親セッションのタスクで、子セッションのルートのタスクは、これを写して見せる。 */
export const delegatedSource = (child: Owner, sessions: Sessions): { owner: Owner; task: AgentTask } | undefined => {
  const from = child.agentTasks?.find((t) => t.id === child.rootTask)?.delegatedFrom;
  const owner = from && follow(sessions, from.session);
  const task = from && owner?.agentTasks?.find((t) => t.id === from.task);
  return owner && task ? { owner, task } : undefined;
};

/** 子セッションのルートのタスクを、親セッションのタスクの今の題、説明、状態、時刻、チケットで見せる。 */
export const withLiveRoot = (child: Owner, sessions: Sessions): AgentTask[] => {
  const source = delegatedSource(child, sessions);
  if (!source) return child.agentTasks ?? [];
  const { subject, description, status, ticket, createdAt, updatedAt, startedAt, completedAt } = source.task;
  return (child.agentTasks ?? []).map((t) => (t.id === child.rootTask ? { ...t, subject, description, status, ticket, createdAt, updatedAt, startedAt, completedAt } : t));
};

/** 完了していない、task の下の手順。子セッションに任せたタスクは、子セッションが登録した手順も数える。 */
const unfinishedSteps = (owner: Owner, task: AgentTask, sessions: Sessions): AgentTask[] => {
  const child = task.delegatedTo ? follow(sessions, task.delegatedTo) : undefined;
  const steps = [...(owner.agentTasks ?? []).filter((t) => t.parent === task.id), ...(child?.agentTasks ?? []).filter((t) => t.id !== child?.rootTask)];
  return steps.filter((t) => t.status !== 'completed');
};

/**
 * タスクを変える。下の手順が終わっていないタスクは完了にしない。子セッションのルートのタスクの題、説明、状態は、実体である親セッションのタスクを変える。親と子で同じタスクがずれないため。
 * 待つタスクと親のタスクは、子セッションの中の並びなので、子セッションのルートのタスクに付ける。
 */
export const updateTask = (owner: Owner, id: string | undefined, sessions: Sessions, change: TaskChange, now = Date.now()): Result => {
  const source = id && id === owner.rootTask ? delegatedSource(owner, sessions) : undefined;
  const target = source ?? { owner, task: owner.agentTasks?.find((t) => t.id === id) };
  const left = change.status === 'completed' && target.task ? unfinishedSteps(target.owner, target.task, sessions) : [];
  if (left.length) return { text: `更新しませんでした。下の手順 ${left.map((t) => `${t.id} ${t.subject}`).join('、')} が終わっていません。先に完了にするか、要らなければ消してください。`, isError: true };
  if (!source) return changeTask(owner, id, change, now);
  const { status, subject, description, ...rest } = change;
  const result = changeTask(source.owner, source.task.id, { status, subject, description }, now);
  return result.isError ? result : changeTask(owner, id, rest, now);
};

/**
 * 画面に出すタスク。ルートのタスクは親のタスクを写し、子セッションに任せたタスクの下には、子セッションが登録した手順を並べる。
 * 子の手順の id は「任せたタスクの id/子の id」にして、親のタスクの id と重ねない。孫セッションに任せた手順も、同じく下へたどる。
 */
export const viewTasks = (owner: Owner & { id: string }, sessions: Sessions, seen = new Set<string>()): AgentTask[] => {
  const own = withLiveRoot(owner, sessions);
  seen.add(owner.id);
  return own.flatMap((t) => {
    const child = t.delegatedTo ? follow(sessions, t.delegatedTo) : undefined;
    if (!child || seen.has(child.id)) return [t];
    const prefix = (id: string): string => `${t.id}/${id}`;
    const steps = viewTasks(child, sessions, seen)
      .filter((c) => c.id !== child.rootTask)
      .map((c) => ({ ...c, id: prefix(c.id), parent: !c.parent || c.parent === child.rootTask ? t.id : prefix(c.parent), blockedBy: c.blockedBy?.map(prefix) }));
    return [t, ...steps];
  });
};

/** 既定では完了していないタスクだけを返す。説明は返さない。 */
export const listTasks = (owner: Owner, includeCompleted: boolean): Result => {
  const all = owner.agentTasks ?? [];
  const tasks = all.filter((t) => includeCompleted || t.status !== 'completed');
  return { text: tasks.length ? tasks.map((t) => line(t, all)).join('\n') : includeCompleted ? '登録したタスクはありません。' : '完了していないタスクはありません。' };
};
