import type { Compose, ComposeAction } from "./compose.ts";
import { mmss } from "./time.ts";

const rangeLabel = (range: Compose["range"]): string => {
  if (!range) return "Range";
  if (range.out === null) return `End here (from ${mmss(range.in)})`;
  return `${mmss(range.in)}–${mmss(range.out)}`;
};

const DrawTools = ({ state, dispatch }: { state: Compose; dispatch: (a: ComposeAction) => void }) => (
  <div class="row tools">
    {(["stroke", "rect"] as const).map((tool) => (
      <button key={tool} type="button" class={`chipbtn${state.tool === tool ? " on" : ""}`} onClick={() => dispatch({ type: "tool", tool })}>
        {tool === "stroke" ? "Line" : "Box"}
      </button>
    ))}
    <button type="button" class="chipbtn" disabled={state.shapes.length === 0} onClick={() => dispatch({ type: "undoShape" })}>
      Undo
    </button>
    <button type="button" class="chipbtn" disabled={state.shapes.length === 0} onClick={() => dispatch({ type: "clearShapes" })}>
      Clear
    </button>
  </div>
);

export interface ComposeSheetProps {
  state: Compose;
  /** 止めている絵の時刻（秒）。 */
  time: number;
  canSend: boolean;
  dispatch: (a: ComposeAction) => void;
  onCancel: () => void;
  onSend: () => void;
}

/** 止めた絵に対するコメントの入力。文字は OS の音声入力でも入る。下の 3 分の 1 に送信を置く。 */
export const ComposeSheet = ({ state, time, canSend, dispatch, onCancel, onSend }: ComposeSheetProps) => (
  <form
    class="sheet compose"
    onSubmit={(e) => {
      e.preventDefault();
      onSend();
    }}
  >
    <p class="sheet-title">Comment at {mmss(time)}</p>
    <textarea
      value={state.text}
      rows={3}
      placeholder="Write a comment"
      aria-label="Comment text"
      onInput={(e) => dispatch({ type: "text", text: e.currentTarget.value })}
    />
    <div class="row">
      <button type="button" class={`chipbtn${state.range ? " on" : ""}`} onClick={() => dispatch({ type: "range", time })}>
        {rangeLabel(state.range)}
      </button>
      {state.range && (
        <button type="button" class="chipbtn" aria-label="Clear range" onClick={() => dispatch({ type: "rangeClear" })}>
          Clear range
        </button>
      )}
      <button
        type="button"
        class={`chipbtn${state.drawing ? " on" : ""}`}
        aria-pressed={state.drawing}
        onClick={() => dispatch({ type: "drawing", on: !state.drawing })}
      >
        Draw
      </button>
    </div>
    {state.drawing && <DrawTools state={state} dispatch={dispatch} />}
    <div class="row actions">
      <button type="button" class="secondary" onClick={onCancel}>
        Cancel
      </button>
      <button type="submit" class="primary" disabled={!canSend}>
        Send
      </button>
    </div>
  </form>
);
