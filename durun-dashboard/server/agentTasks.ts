// エージェントが MCP のツールで登録して進めるタスク。人間がダッシュボードで進み具合を見るためのもの。
// Claude Code のタスクのツールは一覧をコンテキストに載せ続けるので、ここでは登録と更新で一覧を返さず、list_tasks で読んだときだけ返す。
import type { AgentTask } from '../shared/types.ts';

export const TASK_STATUSES = ['pending', 'in_progress', 'completed'] as const;
const TASK_LABEL: Record<AgentTask['status'], string> = { pending: '未着手', in_progress: '作業中', completed: '完了' };

/** agentTaskSeq は最後に振った id。消したタスクの id を使い直さないよう、タスクとは別に覚える。 */
type Owner = { agentTasks?: AgentTask[]; agentTaskSeq?: number };
type Result = { text: string; isError?: boolean; changed?: boolean };

const line = (t: AgentTask): string => `- ${t.id} [${TASK_LABEL[t.status]}] ${t.subject}`;

/** 登録したタスクの id を返す。id はセッションの中の通し番号で、消したタスクの番号は使い直さない。 */
export const addTasks = (owner: Owner, subjects: string[]): Result => {
  const given = subjects.map((s) => s.trim()).filter(Boolean);
  if (!given.length) return { text: 'tasks に、登録するタスクを 1 つ以上渡してください。', isError: true };
  const tasks = owner.agentTasks ?? [];
  let next = owner.agentTaskSeq ?? 0;
  const added = given.map((subject): AgentTask => ({ id: String(++next), subject, status: 'pending' }));
  Object.assign(owner, { agentTasks: [...tasks, ...added], agentTaskSeq: next });
  return { text: `登録しました。\n${added.map(line).join('\n')}`, changed: true };
};

/** status に deleted を渡すと消す。 */
export const changeTask = (owner: Owner, id: string | undefined, status: string | undefined, subject: string | undefined): Result => {
  const task = owner.agentTasks?.find((t) => t.id === id);
  if (!task) return { text: 'id に、create_tasks か list_tasks が返したタスクの id を渡してください。', isError: true };
  if (status === 'deleted') {
    owner.agentTasks = owner.agentTasks?.filter((t) => t !== task);
    return { text: '消しました。', changed: true };
  }
  const isStatus = (v: string): v is AgentTask['status'] => (TASK_STATUSES as readonly string[]).includes(v);
  if (status && !isStatus(status)) return { text: `status には ${[...TASK_STATUSES, 'deleted'].join('、')} のどれかを渡してください。`, isError: true };
  Object.assign(task, { status: status ?? task.status, subject: subject?.trim() || task.subject });
  return { text: '更新しました。', changed: true };
};

/** 既定では完了していないタスクだけを返す。 */
export const listTasks = (owner: Owner, includeCompleted: boolean): Result => {
  const tasks = (owner.agentTasks ?? []).filter((t) => includeCompleted || t.status !== 'completed');
  return { text: tasks.length ? tasks.map(line).join('\n') : includeCompleted ? '登録したタスクはありません。' : '完了していないタスクはありません。' };
};
