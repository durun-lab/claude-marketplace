// ウィジェット「タイムライン」。横軸に時刻、縦にセッションを並べ、状態の区間を線で、主な出来事を印で描く。
// 作業中と実行中は青の太線、待機中は灰色の太線、それ以外は状態の色の細線にする。
import { useRef } from 'preact/hooks';
import type { Snapshot, TimelineEventKind, TimelineRow, TimelineState } from '../shared/types.ts';
import { api } from './api.ts';
import type { WidgetProps } from './Dashboard.tsx';
import { clock, dateTime } from './format.ts';
import { useFetch } from './hooks.ts';
import { href } from './router.ts';
import { useWidth } from './Timeline.tsx';
import { Empty, Loading } from './ui.tsx';

export const TIMELINE_TITLE = 'タイムライン';
const HOURS = [1, 3, 6, 12, 24];
const DEFAULT_HOURS = 6;
const ROW_H = 22;
const LABEL_W = 150;
const AXIS_H = 18;
const RIGHT = 8;
const THICK = 7;
const THIN = 2;
// 画面の更新ごとに読み直すと重いので、この間隔で読み直す。
const REFRESH_MS = 15_000;
const HOUR_MS = 60 * 60_000;
// 目盛りの間隔の候補。描く長さに 4〜8 本ほど並ぶものを選ぶ。
const TICK_MS = [10, 15, 30, 60, 120, 180, 240].map((m) => m * 60_000);

const LINE: Record<Exclude<TimelineState, 'gone'>, { color: string; width: number; label: string }> = {
  busy: { color: 'var(--busy)', width: THICK, label: '作業中' },
  work: { color: 'var(--busy)', width: THICK, label: '実行中' },
  event: { color: 'var(--muted)', width: THICK, label: '待機中' },
  human: { color: 'var(--idle)', width: THIN, label: '回答待ち' },
  idle: { color: 'var(--idle)', width: THIN, label: '入力待ち' },
  done: { color: 'var(--done)', width: THIN, label: '完了' },
  interrupted: { color: 'var(--interrupted)', width: THIN, label: '中断' },
  ended: { color: 'var(--ended)', width: THIN, label: '終了' },
};

const MARK: Record<TimelineEventKind, { glyph: string; label: string }> = {
  prompt: { glyph: '▼', label: 'チャットの指示' },
  question: { glyph: '?', label: '質問' },
  answer: { glyph: '✓', label: '回答' },
  compact: { glyph: '◆', label: 'compact' },
};

const lastAt = (r: TimelineRow): number => Math.max(r.states.at(-1)?.[0] ?? 0, r.events.at(-1)?.[0] ?? 0);

/** from より後に、出来事があったか、作業中か実行中の区間が重なる行だけを描く。止まったままのセッションで行を埋めないため。 */
const movedSince = (r: TimelineRow, from: number): boolean =>
  r.events.some(([at]) => at >= from) || r.states.some(([, state], i) => (state === 'busy' || state === 'work') && (r.states[i + 1]?.[0] ?? Infinity) > from);

const Chart = ({ rows, from, now }: { rows: TimelineRow[]; from: number; now: number }) => {
  const box = useRef<HTMLDivElement>(null);
  const width = useWidth(box);
  const plot = Math.max(width - LABEL_W - RIGHT, 1);
  const x = (at: number) => LABEL_W + ((Math.max(at, from) - from) / (now - from)) * plot;
  const step = TICK_MS.find((ms) => (now - from) / ms <= 8) ?? (TICK_MS.at(-1) as number);
  const ticks = Array.from({ length: Math.floor((now - from) / step) + 1 }, (_, i) => Math.ceil(from / step) * step + i * step).filter((t) => t <= now);
  const height = AXIS_H + rows.length * ROW_H;
  return (
    <div ref={box} class="session-timeline-chart">
      {width > 0 && (
        <svg width={width} height={height} role="img" aria-label="セッションのタイムライン">
          {ticks.map((t) => (
            <g key={t}>
              <line x1={x(t)} x2={x(t)} y1={AXIS_H - 4} y2={height} class="session-timeline-grid" />
              <text x={x(t)} y={AXIS_H - 6} text-anchor="middle" class="session-timeline-tick">{clock(t)}</text>
            </g>
          ))}
          {rows.map((r, i) => {
            const y = AXIS_H + i * ROW_H + ROW_H / 2;
            return (
              <g key={r.id}>
                <a href={href.session(r.id)}>
                  <text x={4} y={y + 4} class="session-timeline-name"><title>{r.name}</title>{r.name.length > 12 ? `${r.name.slice(0, 12)}…` : r.name}</text>
                </a>
                {r.states.map(([at, state], j) => {
                  const end = r.states[j + 1]?.[0] ?? now;
                  if (state === 'gone' || end <= from) return null;
                  const line = LINE[state];
                  return (
                    <line key={at} x1={x(at)} x2={x(end)} y1={y} y2={y} stroke={line.color} stroke-width={line.width}>
                      <title>{`${line.label} ${clock(Math.max(at, from))}〜${clock(end)}`}</title>
                    </line>
                  );
                })}
                {r.events.filter(([at]) => at >= from).map(([at, kind], j) => (
                  <text key={`${at}-${j}`} x={x(at)} y={y - THICK / 2 - 1} text-anchor="middle" class={`session-timeline-mark is-${kind}`}>
                    <title>{`${MARK[kind].label} ${dateTime(at)}`}</title>
                    {MARK[kind].glyph}
                  </text>
                ))}
              </g>
            );
          })}
        </svg>
      )}
    </div>
  );
};

/** 描く長さの中で動いた行を、最後に動いたものから並べる。 */
export const SessionTimeline = ({ snapshot, settings, onSettings }: WidgetProps<{ snapshot: Snapshot }>) => {
  const hours = HOURS.includes(Number(settings.hours)) ? Number(settings.hours) : DEFAULT_HOURS;
  const tick = Math.floor(snapshot.now / REFRESH_MS);
  const { data, error } = useFetch(() => api.timeline(), [tick]);
  const now = snapshot.now;
  const from = now - hours * HOUR_MS;
  const rows = (data ?? []).filter((r) => movedSince(r, from)).sort((a, b) => lastAt(b) - lastAt(a));
  return (
    <div class="widget-body session-timeline">
      <div class="session-timeline-bar">
        <select class="input input-sm" aria-label="描く長さ" value={hours} onChange={(e) => onSettings({ ...settings, hours: e.currentTarget.value })}>
          {HOURS.map((h) => <option key={h} value={h}>{h} 時間</option>)}
        </select>
        <span class="session-timeline-legend">
          {(['busy', 'event', 'idle'] as const).map((k) => (
            <span key={k}><i style={{ background: LINE[k].color, height: `${LINE[k].width}px` }} />{k === 'idle' ? 'ほか（状態の色）' : LINE[k].label}</span>
          ))}
          {Object.entries(MARK).map(([k, m]) => <span key={k}><b class={`session-timeline-mark is-${k}`}>{m.glyph}</b>{m.label}</span>)}
        </span>
      </div>
      {error && <Empty>{error}</Empty>}
      {!data && !error ? <Loading /> : rows.length ? <Chart rows={rows} from={from} now={now} /> : !error && <Empty>この {hours} 時間に動いたセッションはありません。</Empty>}
    </div>
  );
};
