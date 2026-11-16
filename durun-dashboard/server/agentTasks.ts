// エージェントが MCP のツールで登録して進めるタスク。人間がダッシュボードで進み具合を見るためのもの。
// Claude Code のタスクのツールは一覧をコンテキストに載せ続けるので、ここでは登録と更新で一覧を返さず、list_tasks で読んだときだけ返す。
import type { AgentTask } from '../shared/types.ts';

export const TASK_STATUSES = ['pending', 'in_progress', 'completed'] as const;
const TASK_LABEL: Record<AgentTask['status'], string> = { pending: '未着手', in_progress: '作業中', completed: '完了' };

/** agentTaskSeq は最後に振った id。消したタスクの id を使い直さないよう、タスクとは別に覚える。 */
type Owner = { agentTasks?: AgentTask[]; agentTaskSeq?: number };
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
    if (subject) items.push({ subject, description: text(o.description), waits: ids(o.blocked_by), parent: text(o.parent), ticket: ticket?.url });
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

/** 既定では完了していないタスクだけを返す。説明は返さない。 */
export const listTasks = (owner: Owner, includeCompleted: boolean): Result => {
  const all = owner.agentTasks ?? [];
  const tasks = all.filter((t) => includeCompleted || t.status !== 'completed');
  return { text: tasks.length ? tasks.map((t) => line(t, all)).join('\n') : includeCompleted ? '登録したタスクはありません。' : '完了していないタスクはありません。' };
};
