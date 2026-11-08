// Service Worker の本体。ビルドが `__CACHE_NAME__` と `__ASSETS__`（画面の資源の相対パス）を埋める。
import { isCacheable, staleCaches } from "./policy.ts";

declare const __CACHE_NAME__: string;
declare const __ASSETS__: string[];

interface WorkerScope {
  addEventListener: (type: string, fn: (e: never) => void) => void;
  location: { origin: string };
  skipWaiting: () => Promise<void>;
  clients: { claim: () => Promise<void> };
}
interface WaitEvent {
  waitUntil: (p: Promise<unknown>) => void;
}
interface FetchEvt {
  request: Request;
  respondWith: (p: Promise<Response>) => void;
}

const scope = self as unknown as WorkerScope;

scope.addEventListener("install", (e: WaitEvent) => {
  e.waitUntil(caches.open(__CACHE_NAME__).then((c) => c.addAll(__ASSETS__)).then(() => scope.skipWaiting()));
});

scope.addEventListener("activate", (e: WaitEvent) => {
  e.waitUntil(
    caches
      .keys()
      .then((names) => Promise.all(staleCaches(names, __CACHE_NAME__).map((n) => caches.delete(n))))
      .then(() => scope.clients.claim()),
  );
});

// 資源は取得済みを優先し、無ければ取りに行く。画面の遷移は圏外でも起動できるよう index.html にフォールバックする。
scope.addEventListener("fetch", (e: FetchEvt) => {
  const url = new URL(e.request.url);
  if (!isCacheable(e.request.method, url, scope.location.origin)) return;
  e.respondWith(
    caches.open(__CACHE_NAME__).then(async (cache) => {
      const hit = await cache.match(e.request, { ignoreSearch: true });
      if (hit) return hit;
      try {
        return await fetch(e.request);
      } catch (err) {
        const shell = e.request.mode === "navigate" ? await cache.match("./index.html") : undefined;
        if (shell) return shell;
        throw err;
      }
    }),
  );
});
