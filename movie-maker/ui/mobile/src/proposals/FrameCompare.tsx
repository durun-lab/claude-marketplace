import { type RefObject } from "preact";
import { useEffect, useRef, useState } from "preact/hooks";
import { clipUrl, frameUrl, type Proposal } from "./api.ts";
import { frameTime, hasClip } from "./view.ts";

type View = "before" | "after";
/** 短い動画の状態。PC が作っている間は `loading`、映像が出たら `playing`。 */
type Clip = "off" | "loading" | "playing" | "failed";

interface ClipPlayer {
  clip: Clip;
  video: RefObject<HTMLVideoElement>;
  start: (url: string) => void;
  stop: () => void;
  fail: () => void;
  playing: () => void;
}

/** 短い動画の再生の状態と操作。止めた後に届いたイベント（`playing`、`error`）は無視する。 */
const useClip = (): ClipPlayer => {
  const [clip, setClip] = useState<Clip>("off");
  const video = useRef<HTMLVideoElement>(null);
  const fail = (): void => setClip((c) => (c === "off" ? c : "failed"));
  // iOS の Safari は押した操作の中で呼んだ play() だけを許すので、<video> は先に置いておき、src を入れてすぐ呼ぶ。
  const start = (url: string): void => {
    const v = video.current;
    if (!v) return;
    v.src = url;
    setClip("loading");
    void v.play().catch((e: unknown) => {
      if (!(e instanceof DOMException && e.name === "AbortError")) fail();
    });
  };
  const stop = (): void => {
    const v = video.current;
    if (v) {
      v.pause();
      v.removeAttribute("src");
      v.load();
    }
    setClip("off");
  };
  const playing = (): void => setClip((c) => (c === "off" ? c : "playing"));
  return { clip, video, start, stop, fail, playing };
};

/** 前後のフレームの上に重ねる短い動画。映像が出るまでは透明にして、作っている間の表示（`Rendering clip…`）を出す。 */
const ClipLayer = ({ player }: { player: ClipPlayer }) => (
  <>
    <video
      ref={player.video}
      class={`clip${player.clip === "playing" ? " on" : ""}`}
      playsInline
      preload="none"
      controls={player.clip === "playing"}
      onPlaying={player.playing}
      onError={player.fail}
    />
    {player.clip === "loading" && (
      <p class="clip-note" role="status">
        Rendering clip…
      </p>
    )}
    {player.clip === "failed" && (
      <p class="clip-note" role="alert">
        Could not play the clip.
      </p>
    )}
  </>
);

/**
 * 提案の前後のフレーム。2 枚とも先に読み込んで重ね、押すと切り替える（デスクトップの Program の切り替えと同じ）。
 * `Play` は範囲の前後 2 秒の短い動画に替える。
 */
export const FrameCompare = ({ proposal, ops, comp }: { proposal: Proposal; ops: number[] | null; comp?: string }) => {
  const [view, setView] = useState<View>("after");
  const player = useClip();
  const t = frameTime(proposal);
  const on = player.clip !== "off";
  const show = (v: View): void => {
    player.stop();
    setView(v);
  };
  const url = clipUrl(proposal.id, { comp, ops });
  const can = hasClip(proposal);
  // 選ぶ操作が変わるか提案が決まったら、前の操作の短い動画を流し続けない。
  useEffect(() => player.stop, [url, can]);
  const play = (): void => (on ? player.stop() : player.start(url));
  return (
    <div class="compare">
      <div class="stage">
        <button type="button" class="swap" aria-label={`Showing ${view}. Tap to switch`} onClick={() => show(view === "after" ? "before" : "after")}>
          <img src={frameUrl(t, { comp })} alt="Before" class={view === "before" ? "show" : ""} />
          <img src={frameUrl(t, { comp, proposal: proposal.id, ...(ops ? { ops } : {}) })} alt="After" class={view === "after" ? "show" : ""} />
        </button>
        {can && <ClipLayer player={player} />}
      </div>
      <div class="row seg" role="group" aria-label="Compare">
        {(["before", "after"] as const).map((v) => (
          <button key={v} type="button" class={`chipbtn${!on && view === v ? " on" : ""}`} aria-pressed={!on && view === v} onClick={() => show(v)}>
            {v === "before" ? "Before" : "After"}
          </button>
        ))}
        {can && (
          <button type="button" class={`chipbtn${on ? " on" : ""}`} aria-pressed={on} onClick={play}>
            Play
          </button>
        )}
      </div>
    </div>
  );
};
