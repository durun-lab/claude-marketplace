import assert from 'node:assert/strict';
import { test } from 'node:test';
import type { Attention, Session } from '../shared/types.ts';
import { TIMELINE_KEEP_MS, type TimelineLog, noteTimeline } from '../server/timeline.ts';

const session = (id: string, status: Session['status'], waitingFor?: Session['waitingFor']) => ({ id, status, waitingFor }) as Session;
const ask = (id: string, sessionId: string, createdAt: number) => ({ id, kind: 'ask', sessionId, title: '', createdAt }) as Attention;

test('状態が変わった時刻と、質問が届いた時刻と答えた時刻を記録し、一覧から消えたら線を止める', () => {
  const log: TimelineLog = { sessions: {}, asking: {} };
  noteTimeline(log, [session('a', 'busy')], [], 10);
  noteTimeline(log, [session('a', 'busy')], [], 20);
  noteTimeline(log, [session('a', 'idle', 'human')], [ask('q', 'a', 25)], 30);
  noteTimeline(log, [session('a', 'busy')], [], 40);
  noteTimeline(log, [], [], 50);
  assert.deepEqual(log.sessions.a, {
    states: [[10, 'busy'], [30, 'human'], [40, 'busy'], [50, 'gone']],
    events: [[25, 'question'], [40, 'answer']],
  });
  assert.deepEqual(log.asking, {});
});

test('古い記録を捨て、区間の始まりの状態は残す。消えてから時間がたったセッションは忘れる', () => {
  const log: TimelineLog = { sessions: {}, asking: {} };
  noteTimeline(log, [session('a', 'busy'), session('b', 'busy')], [], 0);
  noteTimeline(log, [session('a', 'idle')], [], 10);
  noteTimeline(log, [session('a', 'busy')], [], 20);
  noteTimeline(log, [session('a', 'busy')], [], 15 + TIMELINE_KEEP_MS);
  assert.deepEqual(log.sessions.a?.states, [[10, 'idle'], [20, 'busy']]);
  noteTimeline(log, [session('a', 'busy')], [], 20 + TIMELINE_KEEP_MS);
  assert.equal(log.sessions.b, undefined);
});
