const API_PREFIX = "/m/api/";
const PAIR_PATH = "/m/pair";

/**
 * 画面の資源だけをキャッシュする。API と登録の応答は資格と状態に依存するので、
 * 古い応答を返さないよう Service Worker は触らない。
 */
export const isCacheable = (method: string, url: URL, origin: string): boolean =>
  method === "GET" && url.origin === origin && !url.pathname.startsWith(API_PREFIX) && url.pathname !== PAIR_PATH;

/** バージョンの違う古いキャッシュの名前。 */
export const staleCaches = (names: string[], current: string): string[] =>
  names.filter((n) => n.startsWith("mm-mobile-") && n !== current);
