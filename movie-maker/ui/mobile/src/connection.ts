import { API_ROOT, isUnauthorized, type Api } from "./api/http.ts";
import type { ProjectInfo } from "./api/types.ts";

/** 接続の状態。`unpaired` は資格が無いか取り消された（401）。 */
export type LinkState = "checking" | "unpaired" | "online" | "offline";

export interface ConnectionSnapshot {
  state: LinkState;
  project: ProjectInfo | null;
}

/** `EventSource` のうち使う部分。テストとデモが差し替える。 */
export interface EventSourceLike {
  onopen: (() => void) | null;
  onerror: (() => void) | null;
  addEventListener: (type: string, fn: (ev: { data?: string }) => void) => void;
  close: () => void;
}

export interface ConnectionDeps {
  api: Api;
  openEvents: (url: string) => EventSourceLike;
  /** 圏外から戻ったことを知らせる（`window` の online）。 */
  onNetworkBack: (fn: () => void) => void;
  setTimer?: (fn: () => void, ms: number) => unknown;
}

export interface Connection {
  get: () => ConnectionSnapshot;
  subscribe: (fn: () => void) => () => void;
  /** プロジェクトを読み直し、つながっていれば状態を更新して SSE を開く。 */
  refresh: () => Promise<void>;
  /** 登録の直後など、資格が変わった後にやり直す。 */
  start: () => Promise<void>;
  /** SSE の `comment`、`proposal` を受けるたびに `fn` を呼ぶ。中身は JSON を読んだ値で、読めなければ null。 */
  onEvent: (type: EventType, fn: (data: unknown) => void) => () => void;
  /** 画面から見た最後の版。操作の `base_version` に使う。 */
  version: () => number;
}

const RETRY_MS = 5000;
/** SSE でこれらを受けたらプロジェクトを読み直す。 */
const REFRESH_EVENTS = ["change", "resync"];
/** 画面に中身ごと渡す SSE。 */
const FORWARD_EVENTS = ["comment", "proposal"] as const;
export type EventType = (typeof FORWARD_EVENTS)[number];

const parseData = (raw: string | undefined): unknown => {
  try {
    return raw === undefined ? null : JSON.parse(raw);
  } catch {
    return null;
  }
};

/** SSE のイベントの種類ごとの受け手。 */
const createEventHub = () => {
  const byType = new Map<EventType, Set<(data: unknown) => void>>();
  return {
    on: (type: EventType, fn: (data: unknown) => void): (() => void) => {
      const fns = byType.get(type) ?? new Set();
      fns.add(fn);
      byType.set(type, fns);
      return () => fns.delete(fn);
    },
    emit: (type: EventType, data: unknown): void => byType.get(type)?.forEach((fn) => fn(data)),
  };
};

const forwardEvents = (es: EventSourceLike, hub: ReturnType<typeof createEventHub>): void =>
  FORWARD_EVENTS.forEach((t) => es.addEventListener(t, (ev) => hub.emit(t, parseData(ev.data))));

/** 母艦との接続の状態を持つ。SSE の切断と圏外は `offline`、401 は `unpaired`。 */
export const createConnection = (deps: ConnectionDeps): Connection => {
  const setTimer = deps.setTimer ?? ((fn, ms) => setTimeout(fn, ms));
  let snap: ConnectionSnapshot = { state: "checking", project: null };
  let events: EventSourceLike | null = null;
  const listeners = new Set<() => void>();
  const hub = createEventHub();
  const set = (patch: Partial<ConnectionSnapshot>): void => {
    snap = { ...snap, ...patch };
    listeners.forEach((fn) => fn());
  };
  const retryLater = (): void => void setTimer(() => void refresh(), RETRY_MS);
  const openEvents = (): void => {
    if (events) return;
    const es = deps.openEvents(`${API_ROOT}/events`);
    es.onopen = () => set({ state: "online" });
    es.onerror = () => set({ state: "offline" });
    REFRESH_EVENTS.forEach((t) => es.addEventListener(t, () => void refresh()));
    forwardEvents(es, hub);
    events = es;
  };
  const refresh = async (): Promise<void> => {
    try {
      const project = await deps.api.get<ProjectInfo>(`${API_ROOT}/project`);
      set({ state: "online", project });
      openEvents();
    } catch (e) {
      events?.close();
      events = null;
      if (isUnauthorized(e)) {
        set({ state: "unpaired", project: null });
      } else {
        set({ state: "offline" });
        retryLater();
      }
    }
  };
  deps.onNetworkBack(() => void refresh());
  return {
    get: () => snap,
    subscribe: (fn) => {
      listeners.add(fn);
      return () => listeners.delete(fn);
    },
    refresh,
    start: refresh,
    onEvent: hub.on,
    version: () => snap.project?.version ?? 0,
  };
};
