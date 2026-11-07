import type { CommentShape } from "@mm/sdk";
import { useReducer, useState } from "preact/hooks";
import type { Controller } from "../controller.ts";
import { buildDraft } from "./commentsView.ts";
import { composeReducer, emptyCompose } from "./compose.ts";
import type { Editing } from "./DrawLayer.tsx";

interface Source {
  comp: string;
  /** 止めている絵の時刻（秒）。 */
  time: number;
}

/** コメントを書く流れ。開く、描く、送る（送信待ちの列へ）、閉じる。 */
export const useComposer = (controller: Controller, { comp, time }: Source) => {
  const [state, dispatch] = useReducer(composeReducer, undefined, emptyCompose);
  const [composing, setComposing] = useState(false);
  const draft = buildDraft({ comp, time, text: state.text, range: state.range, shapes: state.shapes });
  const close = (): void => {
    setComposing(false);
    dispatch({ type: "reset" });
  };
  const editing: Editing | undefined =
    composing && state.drawing
      ? { tool: state.tool, onShape: (shape: CommentShape) => dispatch({ type: "shape", shape }) }
      : undefined;
  return {
    state,
    dispatch,
    composing,
    draft,
    editing,
    open: () => setComposing(true),
    close,
    send: () => {
      if (!draft) return;
      void controller.comment(draft);
      close();
    },
  };
};
