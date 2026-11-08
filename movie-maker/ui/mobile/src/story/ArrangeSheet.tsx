import { useEffect, useRef, useState } from "preact/hooks";
import type { Topic } from "@mm/sdk";
import { formatClock, toSeconds } from "@mm/story/src/model.ts";
import { RATING_LABELS, moveItem } from "./model.ts";

/** 長押しと認める時間と、その前に指が動いてよい距離（スクロールの意図は長押しにしない）。 */
const HOLD_MS = 450;
const HOLD_SLOP_PX = 8;

export interface ArrangeSheetProps {
  /** 並べる候補。初めの並びは素材の時刻の順。 */
  topics: readonly Topic[];
  /** 並べた順の話題を、開いているシーケンスへ差し込む。 */
  onInsert: (ordered: Topic[]) => void;
  onClose: () => void;
}

interface Drag {
  id: string;
  startY: number;
}

/** 行の中心の位置から、指の高さにある行の番号を返す。 */
const indexAt = (rows: (HTMLElement | null)[], y: number): number => {
  const hit = rows.findIndex((el) => el !== null && y < el.getBoundingClientRect().bottom);
  return hit < 0 ? rows.length - 1 : hit;
};

/**
 * 長押しで持ち上げ、指を動かして並べ、離すと確定する。
 * 持ち上げた後はスクロールを止める（touchmove の既定動作を止める）。
 */
const useLongPressReorder = (
  order: Topic[],
  setOrder: (next: Topic[]) => void,
  commit: (final: Topic[]) => void,
) => {
  const rows = useRef<(HTMLElement | null)[]>([]);
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const press = useRef<Drag | null>(null);
  const latest = useRef(order);
  latest.current = order;
  const [lifted, setLifted] = useState<string | null>(null);

  useEffect(() => {
    if (!lifted) return;
    const stop = (e: Event): void => void e.preventDefault();
    document.addEventListener("touchmove", stop, { passive: false });
    return () => document.removeEventListener("touchmove", stop);
  }, [lifted]);

  const cancelTimer = (): void => {
    if (timer.current) clearTimeout(timer.current);
    timer.current = null;
  };
  const down = (id: string) => (e: PointerEvent) => {
    if ((e.target as HTMLElement).closest("button")) return;
    press.current = { id, startY: e.clientY };
    cancelTimer();
    timer.current = setTimeout(() => setLifted(id), HOLD_MS);
  };
  const move = (e: PointerEvent): void => {
    const drag = press.current;
    if (!drag) return;
    if (!lifted) {
      if (Math.abs(e.clientY - drag.startY) > HOLD_SLOP_PX) cancelTimer();
      return;
    }
    const from = latest.current.findIndex((t) => t.id === drag.id);
    const to = indexAt(rows.current, e.clientY);
    if (from !== to) setOrder(moveItem(latest.current, from, to));
  };
  const up = (): void => {
    cancelTimer();
    const wasLifted = lifted !== null;
    press.current = null;
    setLifted(null);
    if (wasLifted) commit(latest.current);
  };
  return { rows, lifted, down, move, up };
};

/** 話題の並べ替え。長押しで持ち上げて離すと差し込む。同じことをする上下のボタンと、差し込むボタンも置く。 */
export const ArrangeSheet = ({ topics, onInsert, onClose }: ArrangeSheetProps) => {
  const [order, setOrder] = useState<Topic[]>(() => [...topics]);
  const { rows, lifted, down, move, up } = useLongPressReorder(order, setOrder, onInsert);
  return (
    <div class="st-sheet" role="dialog" aria-label="Arrange topics">
      <header class="st-sheet-head">
        <strong>Arrange topics</strong>
        <button type="button" class="st-plain" onClick={onClose}>
          Close
        </button>
      </header>
      <p class="st-meta">Hold a topic, drag, and release to insert them in this order.</p>
      <ol class="st-order" onPointerMove={move} onPointerUp={up} onPointerCancel={up}>
        {order.map((t, i) => (
          <li
            key={t.id}
            ref={(el) => void (rows.current[i] = el)}
            class="st-row"
            data-lifted={lifted === t.id}
            onPointerDown={down(t.id)}
          >
            <span class="st-grip" aria-hidden="true">
              ≡
            </span>
            <span class="st-row-main">
              <span class="st-title">{t.title}</span>
              <span class="st-time">
                {formatClock(toSeconds(t.in))} · {RATING_LABELS[t.rating ?? "unrated"]}
              </span>
            </span>
            <button type="button" class="st-step" aria-label={`Move ${t.title} up`} disabled={i === 0} onClick={() => setOrder(moveItem(order, i, i - 1))}>
              ▲
            </button>
            <button type="button" class="st-step" aria-label={`Move ${t.title} down`} disabled={i === order.length - 1} onClick={() => setOrder(moveItem(order, i, i + 1))}>
              ▼
            </button>
          </li>
        ))}
      </ol>
      <button type="button" class="st-primary" disabled={order.length === 0} onClick={() => onInsert(order)}>
        Insert {order.length} {order.length === 1 ? "topic" : "topics"}
      </button>
    </div>
  );
};
