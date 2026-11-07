// エージェントのプロセスの優先度を、人間が選んだ値に揃える。ゲームやほかの作業をする間、CPU をそちらへ先に回す。
// 動いているセッションと、バックグラウンドのセッションを起動する supervisor に掛ける。どちらかが後から起動したツールやセッションは、同じ優先度を受け継ぐ。
// Windows では、前から動いていた子孫のプロセスにも優先度を揃え、メモリ優先度を下げて効率モードを掛ける。
import path from 'node:path';
import type { AgentPriority } from '../shared/types.ts';
import { CLAUDE_DIR, readJson, readLiveSessions } from './claude.ts';
import { platform } from './platform.ts';
import { DATA_DIR, state, touch } from './store.ts';

const ROSTER = path.join(CLAUDE_DIR, 'daemon', 'roster.json');

// 子孫まで抑えるには PowerShell を起動してプロセスの一覧を読むので、セッションの状態が変わるたびには走らせず、この間隔を空ける。
const THROTTLE_GAP_MS = 60_000;

/** 子孫まで抑える処理の状態。走っている間に頼まれたら、終わってから間隔を空けて一度だけ走らせ直す。 */
const throttle = { running: false, again: false, lastAt: 0, touched: false };

const throttleAll = async (): Promise<void> => {
  if (throttle.running) {
    throttle.again = true;
    return;
  }
  const wait = throttle.lastAt + THROTTLE_GAP_MS - Date.now();
  if (wait > 0) {
    throttle.running = true;
    setTimeout(() => {
      throttle.running = false;
      void throttleAll();
    }, wait);
    return;
  }
  throttle.running = true;
  throttle.again = false;
  const priority = state.agentPriority ?? 'normal';
  try {
    await platform.throttleProcesses(agentPids(), priority, DATA_DIR);
    throttle.touched = priority !== 'normal';
  } finally {
    throttle.lastAt = Date.now();
    throttle.running = false;
  }
  if (throttle.again) void throttleAll();
};

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
  if (priority === 'normal' && applied.size === 0 && !throttle.touched) return;
  const pids = agentPids();
  for (const pid of applied.keys()) if (!pids.includes(pid)) applied.delete(pid);
  for (const pid of pids) {
    if (applied.get(pid) === priority) continue;
    if (platform.setProcessPriority(pid, priority)) applied.set(pid, priority);
  }
  // 通常に戻したら、掛けたメモリ優先度と効率モードを一度だけ外す。
  if (priority !== 'normal' || throttle.touched) void throttleAll();
};

/** エージェントの優先度を選び直し、動いているプロセスにすぐ掛ける。 */
export const setAgentPriority = (priority: AgentPriority): void => {
  state.agentPriority = priority;
  touch();
  // 人間が選び直した値は、間隔を待たずに掛ける。
  throttle.lastAt = 0;
  applyAgentPriority();
};
