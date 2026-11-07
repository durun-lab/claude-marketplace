import { useState } from "preact/hooks";
import { clipUrl, frameUrl, type Proposal } from "./api.ts";
import { frameTime } from "./view.ts";

type View = "before" | "after";

/**
 * 提案の前後のフレーム。2 枚とも先に読み込んで重ね、押すと切り替える（デスクトップの Program の切り替えと同じ）。
 * `Play` は範囲の前後 2 秒の短い動画に替える。
 */
export const FrameCompare = ({ proposal, ops, comp }: { proposal: Proposal; ops: number[] | null; comp?: string }) => {
  const [view, setView] = useState<View>("after");
  const [playing, setPlaying] = useState(false);
  const t = frameTime(proposal);
  const flip = (): void => setView((v) => (v === "after" ? "before" : "after"));
  return (
    <div class="compare">
      <div class="stage">
        {playing ? (
          <video src={clipUrl(proposal.id)} controls autoPlay playsInline />
        ) : (
          <button type="button" class="swap" aria-label={`Showing ${view}. Tap to switch`} onClick={flip}>
            <img src={frameUrl(t, { comp })} alt="Before" class={view === "before" ? "show" : ""} />
            <img src={frameUrl(t, { comp, proposal: proposal.id, ...(ops ? { ops } : {}) })} alt="After" class={view === "after" ? "show" : ""} />
          </button>
        )}
      </div>
      <div class="row seg" role="group" aria-label="Compare">
        {(["before", "after"] as const).map((v) => (
          <button key={v} type="button" class={`chipbtn${!playing && view === v ? " on" : ""}`} aria-pressed={!playing && view === v} onClick={() => {
            setPlaying(false);
            setView(v);
          }}>
            {v === "before" ? "Before" : "After"}
          </button>
        ))}
        <button type="button" class={`chipbtn${playing ? " on" : ""}`} aria-pressed={playing} onClick={() => setPlaying((p) => !p)}>
          Play
        </button>
      </div>
    </div>
  );
};
