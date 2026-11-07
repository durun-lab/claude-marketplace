import { useCallback, useEffect, useState } from "preact/hooks";
import type { NewFeedback, Target } from "./api.ts";
import { startPicking, targetOf } from "./pick.ts";

interface ComposerProps {
  busy: boolean;
  /** ウィンドウのスクリーンショットを撮り、パスを返す。撮れなければ部品だけ添える。 */
  capture: () => Promise<string>;
  onSubmit: (feedback: NewFeedback) => Promise<boolean>;
}

type Rect = Target["rect"];

/** 指すモードの間も指さずに普通に押せる所（指すのをやめるボタンと案内の帯）。 */
const isPickIgnored = (el: Element) => el.closest("[data-pick-ignore]") !== null;

const rectOf = (el: Element): Rect => {
  const r = el.getBoundingClientRect();
  return { x: r.x, y: r.y, width: r.width, height: r.height };
};

/** 指すモード。部品の上に枠を出し、クリックした部品をスクリーンショットつきで返す。 */
const usePicking = (capture: () => Promise<string>, onTarget: (t: Target) => void) => {
  const [picking, setPicking] = useState(false);
  const [hover, setHover] = useState<Rect | null>(null);
  const end = useCallback(() => {
    setPicking(false);
    setHover(null);
  }, []);
  useEffect(() => {
    if (!picking) return;
    // 効果の片付けは描画の後なので、指し終えたらその場で見張りを外し、片付けまでの間の動きで枠を動かさない。
    let stop: () => void = () => undefined;
    const pick = async (el: Element) => {
      stop();
      setHover(rectOf(el));
      const target = targetOf(el, window.devicePixelRatio || 1);
      const screenshot = await capture().catch(() => undefined);
      onTarget(screenshot ? { ...target, screenshot } : target);
      end();
    };
    const handlers = { onHover: (el: Element | null) => setHover(el ? rectOf(el) : null), onPick: (el: Element) => void pick(el), onCancel: end };
    stop = startPicking(document, handlers, isPickIgnored);
    return () => stop();
  }, [picking, capture, onTarget, end]);
  return { picking, hover, start: () => setPicking(true), cancel: end };
};

const TargetChip = ({ target, onRemove }: { target: Target; onRemove: () => void }) => (
  <li class="mm-feedback-chip" title={target.selector}>
    <span class="mm-feedback-chip-text">
      {target.panel ? `${target.panel}: ` : ""}
      {target.label || target.selector}
      {target.screenshot ? " (screenshot)" : ""}
    </span>
    <button type="button" class="mm-feedback-chip-remove" aria-label={`Remove ${target.label || target.selector}`} onClick={onRemove}>
      ×
    </button>
  </li>
);

/** 指している部品に重ねる枠。WebView の座標で固定して置く。 */
const Highlight = ({ rect }: { rect: Rect }) => (
  <div
    class="mm-feedback-highlight"
    aria-hidden="true"
    style={{ left: `${rect.x}px`, top: `${rect.y}px`, width: `${rect.width}px`, height: `${rect.height}px` }}
  />
);

const Chips = ({ targets, onRemove }: { targets: Target[]; onRemove: (i: number) => void }) => (
  <ul class="mm-feedback-chips" aria-label="Pointed parts">
    {targets.map((t, i) => (
      <TargetChip key={i} target={t} onRemove={() => onRemove(i)} />
    ))}
  </ul>
);

/** 要望の入力。文と、画面で指した部品（クリックで識別子と位置、ウィンドウのスクリーンショット）を送る。 */
export const Composer = ({ busy, capture, onSubmit }: ComposerProps) => {
  const [text, setText] = useState("");
  const [targets, setTargets] = useState<Target[]>([]);
  const addTarget = useCallback((t: Target) => setTargets((ts) => [...ts, t]), []);
  const pick = usePicking(capture, addTarget);
  const send = async (e: Event) => {
    e.preventDefault();
    const viewport = { width: window.innerWidth, height: window.innerHeight };
    if (await onSubmit({ text: text.trim(), targets, context: { viewport } })) {
      setText("");
      setTargets([]);
    }
  };
  return (
    <form class="mm-feedback-composer" onSubmit={send}>
      <textarea
        class="mm-feedback-text"
        aria-label="Feedback"
        placeholder="What should change? Point at the part of the screen if it helps."
        value={text}
        onInput={(e) => setText(e.currentTarget.value)}
      />
      {targets.length > 0 && <Chips targets={targets} onRemove={(i) => setTargets((ts) => ts.filter((_, j) => j !== i))} />}
      <div class="mm-feedback-actions">
        <button type="button" class="mm-feedback-button" data-pick-ignore aria-pressed={pick.picking} onClick={pick.picking ? pick.cancel : pick.start}>
          {pick.picking ? "Cancel pointing" : "Point at UI"}
        </button>
        <button type="submit" class="mm-feedback-button mm-feedback-send" disabled={busy || text.trim() === ""}>
          Send
        </button>
      </div>
      {pick.picking && (
        <p class="mm-feedback-pickbar" data-pick-ignore role="status">
          Click a part of the screen to attach it. Esc cancels.
        </p>
      )}
      {pick.hover && <Highlight rect={pick.hover} />}
    </form>
  );
};
