export type Engine = "native" | "hlsjs" | "none";

export interface Support {
  /** `video.canPlayType("application/vnd.apple.mpegurl")` が空でない。 */
  nativeHls: boolean;
  /** Media Source Extensions が使える。 */
  mse: boolean;
  /** iPhone、iPad の Safari。 */
  ios: boolean;
}

/**
 * 再生の方法を選ぶ。iOS の Safari は HLS をそのまま再生する（MSE は iPhone に無い）。
 * Android の Chrome は直接の再生の対応が版でまちまちなので、MSE の上の hls.js に揃える。
 */
export const chooseEngine = ({ nativeHls, mse, ios }: Support): Engine => {
  if (ios && nativeHls) return "native";
  if (mse) return "hlsjs";
  return nativeHls ? "native" : "none";
};

const HLS_MIME = "application/vnd.apple.mpegurl";

/** この端末の対応の状況。 */
export const detectSupport = (video: HTMLVideoElement, userAgent: string, maxTouchPoints: number): Support => ({
  nativeHls: video.canPlayType(HLS_MIME) !== "",
  mse: typeof MediaSource !== "undefined" || "ManagedMediaSource" in globalThis,
  // iPadOS は Mac を名乗るので、タッチの点の数で見分ける
  ios: /iPhone|iPad|iPod/.test(userAgent) || (/Macintosh/.test(userAgent) && maxTouchPoints > 1),
});

/** 再生を止めて後始末する関数。 */
export type Detach = () => void;

/**
 * `video` に HLS の `url` をつなぐ。つなげなければ例外を投げる。
 * hls.js は使うときまで読まない（iOS では読み込まない）。
 */
export const attachStream = async (video: HTMLVideoElement, url: string, support: Support): Promise<Detach> => {
  const engine = chooseEngine(support);
  if (engine === "none") throw new Error("This browser cannot play HLS.");
  if (engine === "native") {
    video.src = url;
    return () => video.removeAttribute("src");
  }
  const { default: Hls } = await import("hls.js");
  if (!Hls.isSupported()) throw new Error("This browser cannot play HLS.");
  const hls = new Hls({ enableWorker: true });
  hls.loadSource(url);
  hls.attachMedia(video);
  return () => hls.destroy();
};
