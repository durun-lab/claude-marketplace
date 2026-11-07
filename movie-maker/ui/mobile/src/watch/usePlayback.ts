import { useCallback, useEffect, useRef, useState } from "preact/hooks";
import { attachStream, detectSupport } from "./hls.ts";

/** `<video>` をつなぐ関数。テストでは差し替える。 */
export type Attach = typeof attachStream;

export interface Playback {
  time: number;
  duration: number;
  playing: boolean;
  /** 映像をつなげなかった、または再生で失敗した。 */
  failed: boolean;
}

const SYNC_EVENTS = ["timeupdate", "durationchange", "loadedmetadata", "play", "pause", "seeked"];

const readState = (video: HTMLVideoElement): Omit<Playback, "failed"> => ({
  time: video.currentTime,
  duration: Number.isFinite(video.duration) ? video.duration : 0,
  playing: !video.paused,
});

/** 映像の再生の状態と、再生・停止・シークの操作。`src` が変わると張り直す。 */
export const usePlayback = (src: string, attach: Attach = attachStream) => {
  const ref = useRef<HTMLVideoElement>(null);
  const [state, setState] = useState<Playback>({ time: 0, duration: 0, playing: false, failed: false });
  useEffect(() => {
    const video = ref.current;
    if (!video || !src) return undefined;
    const sync = (): void => setState((s) => ({ ...s, ...readState(video) }));
    const fail = (): void => setState((s) => ({ ...s, failed: true }));
    SYNC_EVENTS.forEach((e) => video.addEventListener(e, sync));
    video.addEventListener("error", fail);
    let detach: (() => void) | null = null;
    let stopped = false;
    attach(video, src, detectSupport(video, navigator.userAgent, navigator.maxTouchPoints)).then((d) => {
      if (stopped) d();
      else detach = d;
    }, fail);
    return () => {
      stopped = true;
      detach?.();
      SYNC_EVENTS.forEach((e) => video.removeEventListener(e, sync));
      video.removeEventListener("error", fail);
    };
  }, [src, attach]);
  const seek = useCallback((seconds: number) => {
    const video = ref.current;
    if (!video) return;
    video.currentTime = seconds;
    setState((s) => ({ ...s, time: seconds }));
  }, []);
  const pause = useCallback(() => ref.current?.pause(), []);
  const toggle = useCallback(() => {
    const video = ref.current;
    if (!video) return;
    if (video.paused) void video.play().catch(() => undefined);
    else video.pause();
  }, []);
  return { ref, ...state, seek, pause, toggle };
};
