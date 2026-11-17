// 子セッションを抱えた親セッションだけを新しくするときに、子セッションとタスクを新しい親へ付け替える。
import type { AgentTask } from '../shared/types.ts';

type Meta = { id: string; parent?: string; movedTo?: string; agentTasks?: AgentTask[]; agentTaskSeq?: number };

/**
 * from の子セッションを to の子にし、子に任せたタスクの元を to に向ける。from が登録したタスクは to へ移す。
 * 移した子セッションの数を返す。会話を引き継いだ複製へ移った子は、複製の方を数える。
 */
export const handOverChildren = (sessions: Record<string, Meta>, from: Meta, to: Meta): number => {
  let moved = 0;
  for (const m of Object.values(sessions)) {
    if (m.parent === from.id) {
      m.parent = to.id;
      if (!m.movedTo) moved += 1;
    }
    for (const t of m.agentTasks ?? []) if (t.delegatedFrom?.session === from.id) t.delegatedFrom = { ...t.delegatedFrom, session: to.id };
  }
  if (from.agentTasks?.length && !to.agentTasks?.length) {
    Object.assign(to, { agentTasks: from.agentTasks, agentTaskSeq: from.agentTaskSeq });
    Object.assign(from, { agentTasks: undefined, agentTaskSeq: undefined });
  }
  return moved;
};

/**
 * 人間がセッション ID を貼って、child を parent の子にする。parent を省くと親から外す。
 * 自分自身と、親をたどって child に戻る組み合わせは、親子が輪になるので断る。断る理由を返し、結べたら undefined を返す。
 */
export const linkParent = (sessions: Record<string, Meta>, child: Meta, parent?: Meta): string | undefined => {
  if (!parent) {
    child.parent = undefined;
    return undefined;
  }
  if (parent.id === child.id) return '自分自身を親にはできません';
  const seen = new Set<string>();
  for (let m: Meta | undefined = parent; m; m = m.parent ? sessions[m.parent] : undefined) {
    if (m.id === child.id) return '親子が輪になるので結べません';
    if (seen.has(m.id)) break;
    seen.add(m.id);
  }
  child.parent = parent.id;
  return undefined;
};
