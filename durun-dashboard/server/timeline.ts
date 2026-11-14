// タイムラインのウィジェットに描く、セッションの状態の移り変わりと、人間への質問と回答の記録。
// Claude Code は状態の履歴を残さないので、スナップショットを作るたびに状態を見比べ、変わった時刻を覚える。
import type { Attention, Session, TimelineEventKind, TimelineState } from '../shared/types.ts';

export type TimelineRecord = { states: [number, TimelineState][]; events: [number, TimelineEventKind][] };
/** asking は、まだ答えていない質問の id と、そのセッションの id。 */
export type TimelineLog = { sessions: Record<string, TimelineRecord>; asking: Record<string, string> };

// タイムラインに描く長さの上限。これより古い記録は捨てる。
export const TIMELINE_KEEP_MS = 24 * 60 * 60_000;
const ASK_KINDS = new Set<Attention['kind']>(['ask', 'question']);

const recordOf = (log: TimelineLog, id: string): TimelineRecord => (log.sessions[id] ??= { states: [], events: [] });

/** 古い記録を捨てる。区間の始まりの状態が分かるよう、切る時刻より前の最後の状態は残す。 */
const prune = (r: TimelineRecord, cutoff: number): void => {
  const first = r.states.findLastIndex(([at]) => at < cutoff);
  if (first > 0) r.states.splice(0, first);
  if (r.events[0] && r.events[0][0] < cutoff) r.events = r.events.filter(([at]) => at >= cutoff);
};

/** スナップショットのセッションと要対応から、変わった状態と、新しく届いた質問と答えた質問を記録する。 */
export const noteTimeline = (log: TimelineLog, sessions: Session[], attention: Attention[], now: number): void => {
  const listed = new Set<string>();
  for (const s of sessions) {
    listed.add(s.id);
    const r = recordOf(log, s.id);
    const key = s.waitingFor ?? s.status;
    if (r.states.at(-1)?.[1] !== key) r.states.push([now, key]);
  }
  const asking = attention.filter((a) => ASK_KINDS.has(a.kind));
  for (const a of asking) {
    if (a.id in log.asking) continue;
    log.asking[a.id] = a.sessionId;
    recordOf(log, a.sessionId).events.push([Math.min(a.createdAt, now), 'question']);
  }
  const open = new Set(asking.map((a) => a.id));
  for (const [id, sessionId] of Object.entries(log.asking)) {
    if (open.has(id)) continue;
    delete log.asking[id];
    recordOf(log, sessionId).events.push([now, 'answer']);
  }
  const cutoff = now - TIMELINE_KEEP_MS;
  for (const [id, r] of Object.entries(log.sessions)) {
    if (!listed.has(id) && r.states.at(-1)?.[1] !== 'gone') r.states.push([now, 'gone']);
    prune(r, cutoff);
    const last = r.states.at(-1);
    if (!last || (last[1] === 'gone' && last[0] < cutoff && !r.events.length)) delete log.sessions[id];
  }
};
