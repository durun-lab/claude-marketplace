import type { CommentShape } from "@mm/sdk";
import type { Tool } from "./shapes.ts";

/** コメントを書いている途中の状態。 */
export interface Compose {
  text: string;
  /** `Range` の始めと終わり（秒）。終わりは再生して止めた所で決める。 */
  range: { in: number; out: number | null } | null;
  shapes: CommentShape[];
  tool: Tool;
  /** 指で描く面を有効にしている。 */
  drawing: boolean;
}

export const emptyCompose = (): Compose => ({ text: "", range: null, shapes: [], tool: "stroke", drawing: false });

export type ComposeAction =
  | { type: "reset" }
  | { type: "text"; text: string }
  | { type: "range"; time: number }
  | { type: "rangeClear" }
  | { type: "shape"; shape: CommentShape }
  | { type: "undoShape" }
  | { type: "clearShapes" }
  | { type: "tool"; tool: Tool }
  | { type: "drawing"; on: boolean };

/** `Range` は 1 回目で今の時刻を始めに、2 回目で今の時刻を終わりにする。 */
const nextRange = (range: Compose["range"], time: number): Compose["range"] => {
  if (!range || range.out !== null) return { in: time, out: null };
  return { in: range.in, out: time };
};

export const composeReducer = (s: Compose, a: ComposeAction): Compose => {
  switch (a.type) {
    case "reset":
      return emptyCompose();
    case "text":
      return { ...s, text: a.text };
    case "range":
      return { ...s, range: nextRange(s.range, a.time) };
    case "rangeClear":
      return { ...s, range: null };
    case "shape":
      return { ...s, shapes: [...s.shapes, a.shape] };
    case "undoShape":
      return { ...s, shapes: s.shapes.slice(0, -1) };
    case "clearShapes":
      return { ...s, shapes: [] };
    case "tool":
      return { ...s, tool: a.tool };
    case "drawing":
      return { ...s, drawing: a.on };
  }
};
