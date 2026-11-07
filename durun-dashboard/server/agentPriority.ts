// エージェントのプロセスの優先度を、人間が選んだ値に揃える。ゲームやほかの作業をする間、CPU をそちらへ先に回す。
// 動いているセッションと、バックグラウンドのセッションを起動する supervisor に掛ける。どちらかが後から起動したツールやセッションは、同じ優先度を受け継ぐ。
import path from 'node:path';
import type { AgentPriority } from '../shared/types.ts';
import { CLAUDE_DIR, readJson, readLiveSessions } from './claude.ts';
import { platform } from './platform.ts';
import { state, touch } from './store.ts';

const ROSTER = path.join(CLAUDE_DIR, 'daemon', 'roster.json');

/** 優先度を掛け終えたプロセスと、その値。セッションが増減するたびにすべてへ掛け直さないよう覚える。 */
const applied = new Map<number, AgentPriority>();

/** エージェントのプロセスの PID。 */
const agentPids = (): number[] => {
  const pids = [...readLiveSessions().values()].map((s) => s.pid);
  const supervisor = readJson<{ supervisorPid: number }>(ROSTER)?.supervisorPid;
  return supervisor ? [...pids, supervisor] : pids;
};

/** まだ選んだ優先度になっていないエージェントのプロセスに掛ける。通常のまま一度も変えていなければ何もしない。 */
export const applyAgentPriority = (): void => {
  const priority = state.agentPriority ?? 'normal';
  if (priority === 'normal' && applied.size === 0) return;
  const pids = agentPids();
  for (const pid of applied.keys()) if (!pids.includes(pid)) applied.delete(pid);
  for (const pid of pids) {
    if (applied.get(pid) === priority) continue;
    if (platform.setProcessPriority(pid, priority)) applied.set(pid, priority);
  }
};

/** エージェントの優先度を選び直し、動いているプロセスにすぐ掛ける。 */
export const setAgentPriority = (priority: AgentPriority): void => {
  state.agentPriority = priority;
  touch();
  applyAgentPriority();
};
