import { useEffect, useRef, useState } from "preact/hooks";

export interface Player {
  /** `<audio>` に渡す ref。 */
  ref: { current: HTMLAudioElement | null };
  playing: boolean;
  /** `src` の素材の音を `start` 秒から `end` 秒まで鳴らす。 */
  play: (src: string, start: number, end: number) => void;
  stop: () => void;
}

/** 素材の音だけを鳴らす。範囲の終わりで止める。 */
export const usePlayer = (): Player => {
  const ref = useRef<HTMLAudioElement | null>(null);
  const end = useRef(Infinity);
  const [playing, setPlaying] = useState(false);
  const stop = (): void => {
    ref.current?.pause();
    setPlaying(false);
  };
  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    const tick = (): void => {
      if (el.currentTime >= end.current) stop();
    };
    el.addEventListener("timeupdate", tick);
    el.addEventListener("ended", stop);
    return () => {
      el.removeEventListener("timeupdate", tick);
      el.removeEventListener("ended", stop);
    };
  }, []);
  const play = (src: string, start: number, stopAt: number): void => {
    const el = ref.current;
    if (!el) return;
    end.current = stopAt;
    const go = (): void => {
      el.currentTime = start;
      el.play().then(() => setPlaying(true), () => setPlaying(false));
    };
    if (el.getAttribute("src") === src && el.readyState >= 1) return go();
    el.addEventListener("loadedmetadata", go, { once: true });
    el.src = src;
    el.load();
  };
  return { ref, playing, play, stop };
};
