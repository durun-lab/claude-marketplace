/** Service Worker を画面と同じ階層の `sw.js` で登録する。非対応や失敗は黙って無視し、通常のページとして動く。 */
export const registerServiceWorker = async (): Promise<void> => {
  if (!("serviceWorker" in navigator)) return;
  try {
    await navigator.serviceWorker.register("./sw.js", { scope: "./" });
  } catch (e) {
    console.warn("service worker registration failed", e);
  }
};
