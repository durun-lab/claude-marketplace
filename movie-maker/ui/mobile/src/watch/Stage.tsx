import type { CommentShape } from "@mm/sdk";
import type { RefObject } from "preact";
import { DrawLayer, type Editing } from "./DrawLayer.tsx";

export interface StageProps {
  videoRef: RefObject<HTMLVideoElement>;
  playing: boolean;
  failed: boolean;
  shapes: CommentShape[];
  editing?: Editing;
  onToggle: () => void;
}

/** 16:9 の映像と、その上の描き込み。画面の上に固定し、押すと再生と停止を切り替える。 */
export const Stage = ({ videoRef, playing, failed, shapes, editing, onToggle }: StageProps) => (
  <div class="stage">
    <video ref={videoRef} playsInline preload="metadata" onClick={editing ? undefined : onToggle} />
    <DrawLayer shapes={shapes} editing={editing} />
    {failed && <p class="overlay">Video is not available yet.</p>}
    {!playing && !failed && !editing && (
      <button type="button" class="play" aria-label="Play" onClick={onToggle}>
        <svg viewBox="0 0 24 24" aria-hidden="true">
          <path d="M8 5v14l11-7z" />
        </svg>
      </button>
    )}
  </div>
);
