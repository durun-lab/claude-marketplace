// ウィジェット「タイムライン」。横軸に時刻、縦にセッションを並べ、状態の区間を線で、主な出来事を印で描く。
// 作業中と実行中は青の太線、待機中は灰色の太線、それ以外は状態の色の細線にする。
import { useRef } from 'preact/hooks';
import type { TimelineEventKind, TimelineRow, TimelineState } from '../shared/types.ts';
import type { WidgetSettings } from './layout.ts';
import { api } from './api.ts';
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

const lastPrompt = (r: TimelineRow): number => r.events.findLast(([, kind]) => kind === 'prompt')?.[0] ?? 0;

const lastAt = (r: TimelineRow): number => Math.max(r.states.at(-1)?.[0] ?? 0, r.events.at(-1)?.[0] ?? 0);

/** from より後に、出来事があったか、作業中か実行中の区間が重なる行だけを描く。止まったままのセッションで行を埋めないため。 */
const movedSince = (r: TimelineRow, from: number): boolean =>
  r.events.some(([at]) => at >= from) || r.states.some(([, state], i) => (state === 'busy' || state === 'work') && (r.states[i + 1]?.[0] ?? Infinity) > from);

/** 1 つのセッションの行。名前は HTML で省略して出し、線と印は SVG で描く。 */
const Lane = ({ r, x, ticks, from, now, plot, current }: { r: TimelineRow; x: (at: number) => number; ticks: number[]; from: number; now: number; plot: number; current: boolean }) => {
  const y = ROW_H / 2;
  return (
    <div class={`session-timeline-row${current ? ' is-current' : ''}`}>
      <a class="session-timeline-name" href={href.session(r.id)} title={r.name}>{r.name}</a>
      <svg width={plot + RIGHT} height={ROW_H} aria-hidden="true">
        {ticks.map((t) => <line key={t} x1={x(t)} x2={x(t)} y1={0} y2={ROW_H} class="session-timeline-grid" />)}
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
      </svg>
    </div>
  );
};

/** セッションが多くても時刻が読めるよう、時刻の目盛りは行を送っても上に残す。 */
const Chart = ({ rows, from, now, current }: { rows: TimelineRow[]; from: number; now: number; current?: string }) => {
  const box = useRef<HTMLDivElement>(null);
  const width = useWidth(box);
  const plot = Math.max(width - LABEL_W - RIGHT, 1);
  const x = (at: number) => ((Math.max(at, from) - from) / (now - from)) * plot;
  const step = TICK_MS.find((ms) => (now - from) / ms <= 8) ?? (TICK_MS.at(-1) as number);
  const ticks = Array.from({ length: Math.floor((now - from) / step) + 1 }, (_, i) => Math.ceil(from / step) * step + i * step).filter((t) => t <= now);
  return (
    <div ref={box} class="session-timeline-chart" style={{ '--label-w': `${LABEL_W}px` }}>
      {width > 0 && (
        <>
          <div class="session-timeline-axis">
            <span class="session-timeline-count">{rows.length} セッション</span>
            <svg width={plot + RIGHT} height={AXIS_H} aria-hidden="true">
              {ticks.map((t) => <text key={t} x={x(t)} y={AXIS_H - 5} text-anchor="middle" class="session-timeline-tick">{clock(t)}</text>)}
            </svg>
          </div>
          {rows.map((r) => <Lane key={r.id} r={r} x={x} ticks={ticks} from={from} now={now} plot={plot} current={r.id === current} />)}
        </>
      )}
    </div>
  );
};

type Props = { now: number; current?: string; settings: WidgetSettings; onSettings: (next: WidgetSettings) => void };

/**
 * 描く長さの中で動いた行を、最後にチャットで指示したものから並べ、指示の無い行は最後に動いたものから後ろに続ける。
 * セッションの画面では current にそのセッションを渡し、動いていなくても先頭に置いて背景で見分ける。
 */
export const SessionTimeline = ({ now, current, settings, onSettings }: Props) => {
  const hours = HOURS.includes(Number(settings.hours)) ? Number(settings.hours) : DEFAULT_HOURS;
  const tick = Math.floor(now / REFRESH_MS);
  const { data, error } = useFetch(() => api.timeline(), [tick]);
  const from = now - hours * HOUR_MS;
  const rows = (data ?? []).filter((r) => r.id === current || movedSince(r, from)).sort((a, b) => Number(b.id === current) - Number(a.id === current) || lastPrompt(b) - lastPrompt(a) || lastAt(b) - lastAt(a));
  return (
    <>
      <h2 class="section-title">{TIMELINE_TITLE}</h2>
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
        {!data && !error ? <Loading /> : rows.length ? <Chart rows={rows} from={from} now={now} current={current} /> : !error && <Empty>この {hours} 時間に動いたセッションはありません。</Empty>}
      </div>
    </>
  );
};
