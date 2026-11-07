import { useEffect, useRef, useState } from "preact/hooks";

export interface Player {
  /** `<audio>` に渡す ref。 */
  ref: { current: HTMLAudioElement | null };
  playing: boolean;
  /** `src` の素材の音を `start` 秒から `end` 秒まで鳴らす。 */
  play: (src: string, start: number, end: number) => void;
  stop: () => void;
}

/** `startPlayback` が触る `<audio>` の部分。 */
export type MediaLike = Pick<HTMLMediaElement, "currentTime" | "readyState" | "getAttribute" | "play"> & { src: string };

const HAVE_METADATA = 1;

/**
 * `src` を `start` 秒から鳴らし始める。iOS の Safari は押した操作の中で `play()` を呼ばないと拒否するので、
 * 読み込みを待たずにすぐ呼び、始めの位置は読み込み済みならシーク、まだならメディアフラグメント（`#t=`）で渡す。
 */
export const startPlayback = (el: MediaLike, src: string, start: number): Promise<void> => {
  if (el.getAttribute("src")?.split("#")[0] === src && el.readyState >= HAVE_METADATA) el.currentTime = start;
  else el.src = `${src}#t=${start}`;
  return el.play();
};

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
    startPlayback(el, src, start).then(
      () => setPlaying(true),
      () => setPlaying(false),
    );
  };
  return { ref, playing, play, stop };
};
