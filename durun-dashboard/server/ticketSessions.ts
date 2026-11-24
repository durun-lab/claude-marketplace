// エージェントが実装を始める前に、同じチケットを見ているほかのセッションと衝突しないかを確かめるための、チケットの関連セッション。
// 画面の「関連セッション」と同じく、その PR か JIRA の課題をバッジにしているセッションを並べる。
import type { Session } from '../shared/types.ts';

const STATUS_LABEL: Record<Session['status'], string> = { busy: '作業中', idle: '入力待ち', done: '停止', interrupted: '停止', ended: '停止' };
const TASK_LABEL = { pending: '未着手', in_progress: '作業中', completed: '完了' } as const;

/** ticket は PR の URL か番号（#123）、JIRA の課題のキーか URL。合うバッジの URL を返す。 */
const badgeOf = (s: Session, ticket: string): string | undefined => {
  const t = ticket.trim().replace(/\/+$/, '');
  const key = (t.match(/\/browse\/([A-Z][A-Z0-9]*-\d+)$/i)?.[1] ?? t).toUpperCase();
  const num = t.match(/^#?(\d+)$/)?.[1];
  return (
    s.prs.find((p) => p.url === t || (num !== undefined && String(p.number) === num))?.url ?? s.jira.find((j) => j.key.toUpperCase() === key)?.url
  );
};

/** self を除いた、ticket をバッジにしているセッション。チケットをタスクにしたセッションを先に、アーカイブ済みを後に並べる。 */
export const ticketSessionsText = (sessions: Session[], ticket: string, self: string): string => {
  const rows = sessions
    .filter((s) => s.id !== self)
    .flatMap((s) => {
      const url = badgeOf(s, ticket);
      return url ? [{ s, task: s.agentTasks?.find((t) => t.ticket === url)?.status }] : [];
    })
    .sort((a, b) => Number(a.s.archived) - Number(b.s.archived) || Number(b.task !== undefined) - Number(a.task !== undefined));
  if (!rows.length) return `${ticket} をバッジにしているほかのセッションはありません。`;
  return rows
    .map(({ s, task }) => `- session_id: ${s.id}、名前: ${s.name}、状態: ${s.archived ? 'アーカイブ済み' : STATUS_LABEL[s.status]}、リポジトリ: ${s.repo}${s.branch ? `、ブランチ: ${s.branch}` : ''}${task ? `、このチケットのタスク: ${TASK_LABEL[task]}` : ''}`)
    .join('\n');
};
