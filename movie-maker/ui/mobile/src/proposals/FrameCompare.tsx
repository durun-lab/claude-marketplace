import { useState } from "preact/hooks";
import { frameUrl, type Proposal } from "./api.ts";
import { frameTime } from "./view.ts";

type View = "before" | "after";

/**
 * 提案の前後のフレーム。2 枚とも先に読み込んで重ね、押すと切り替える（デスクトップの Program の切り替えと同じ）。
 * 提案の区間の短い動画（`clip.mp4`）は PC がまだ配らないので、再生のボタンは置かない。
 */
export const FrameCompare = ({ proposal, ops, comp }: { proposal: Proposal; ops: number[] | null; comp?: string }) => {
  const [view, setView] = useState<View>("after");
  const t = frameTime(proposal);
  const flip = (): void => setView((v) => (v === "after" ? "before" : "after"));
  return (
    <div class="compare">
      <div class="stage">
        <button type="button" class="swap" aria-label={`Showing ${view}. Tap to switch`} onClick={flip}>
          <img src={frameUrl(t, { comp })} alt="Before" class={view === "before" ? "show" : ""} />
          <img src={frameUrl(t, { comp, proposal: proposal.id, ...(ops ? { ops } : {}) })} alt="After" class={view === "after" ? "show" : ""} />
        </button>
      </div>
      <div class="row seg" role="group" aria-label="Compare">
        {(["before", "after"] as const).map((v) => (
          <button key={v} type="button" class={`chipbtn${view === v ? " on" : ""}`} aria-pressed={view === v} onClick={() => setView(v)}>
            {v === "before" ? "Before" : "After"}
          </button>
        ))}
      </div>
    </div>
  );
};
