// エージェントが MCP のツールで登録して進めるタスク。人間がダッシュボードで進み具合を見るためのもの。
// Claude Code のタスクのツールは一覧をコンテキストに載せ続けるので、ここでは登録と更新で一覧を返さず、list_tasks で読んだときだけ返す。
import type { AgentTask } from '../shared/types.ts';

export const TASK_STATUSES = ['pending', 'in_progress', 'completed'] as const;
const TASK_LABEL: Record<AgentTask['status'], string> = { pending: '未着手', in_progress: '作業中', completed: '完了' };

/** agentTaskSeq は最後に振った id。消したタスクの id を使い直さないよう、タスクとは別に覚える。 */
type Owner = { agentTasks?: AgentTask[]; agentTaskSeq?: number };
type Result = { text: string; isError?: boolean; changed?: boolean };
/** create_tasks に渡すタスク。題だけの文字列でも受け付ける。 */
export type NewTask = string | { subject?: unknown; description?: unknown };

const line = (t: AgentTask): string => `- ${t.id} [${TASK_LABEL[t.status]}] ${t.subject}`;
const text = (v: unknown): string | undefined => (typeof v === 'string' && v.trim() ? v.trim() : undefined);

/** 登録したタスクの id を返す。id はセッションの中の通し番号で、消したタスクの番号は使い直さない。 */
export const addTasks = (owner: Owner, given: NewTask[], now = Date.now()): Result => {
  const items = given.flatMap((t) => {
    const subject = text(typeof t === 'string' ? t : t?.subject);
    return subject ? [{ subject, description: typeof t === 'string' ? undefined : text(t?.description) }] : [];
  });
  if (!items.length) return { text: 'tasks に、登録するタスクを 1 つ以上渡してください。', isError: true };
  let next = owner.agentTaskSeq ?? 0;
  const added = items.map((t): AgentTask => ({ id: String(++next), ...t, status: 'pending', createdAt: now }));
  Object.assign(owner, { agentTasks: [...(owner.agentTasks ?? []), ...added], agentTaskSeq: next });
  return { text: `登録しました。\n${added.map(line).join('\n')}`, changed: true };
};

export type TaskChange = { status?: string; subject?: string; description?: string };

/** status に deleted を渡すと消す。着手と完了の時刻は、その状態に変えた時刻を覚え、未着手に戻したら忘れる。 */
export const changeTask = (owner: Owner, id: string | undefined, change: TaskChange, now = Date.now()): Result => {
  const task = owner.agentTasks?.find((t) => t.id === id);
  if (!task) return { text: 'id に、create_tasks か list_tasks が返したタスクの id を渡してください。', isError: true };
  const { status } = change;
  if (status === 'deleted') {
    owner.agentTasks = owner.agentTasks?.filter((t) => t !== task);
    return { text: '消しました。', changed: true };
  }
  const isStatus = (v: string): v is AgentTask['status'] => (TASK_STATUSES as readonly string[]).includes(v);
  const next = status && isStatus(status) ? status : undefined;
  if (status && !next) return { text: `status には ${[...TASK_STATUSES, 'deleted'].join('、')} のどれかを渡してください。`, isError: true };
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
  const tasks = (owner.agentTasks ?? []).filter((t) => includeCompleted || t.status !== 'completed');
  return { text: tasks.length ? tasks.map(line).join('\n') : includeCompleted ? '登録したタスクはありません。' : '完了していないタスクはありません。' };
};
