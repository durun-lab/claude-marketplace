import type { Platform } from "../controller.ts";
import type { EventSourceLike } from "../connection.ts";
import type { CommentItem, ProjectInfo } from "../api/types.ts";
import { MOBILE_HEADER } from "../api/http.ts";

/** デモとテストの母艦。母艦の API が出来るまで、12-mobile.md の「HTTP の形」の応答を手元で返す。 */
export const MOCK_PAIR_CODE = "ABCD2345";

const HTTP_OK = 200;
const HTTP_UNAUTHORIZED = 401;
const HTTP_FORBIDDEN = 403;
const HTTP_NOT_FOUND = 404;

export interface MockServer {
  platform: Pick<Platform, "fetch" | "openEvents">;
  setOnline: (online: boolean) => void;
  setPaired: (paired: boolean) => void;
  comments: CommentItem[];
  /** 母艦が受けた書き込みの要求（パスと本文）。 */
  writes: { path: string; body: unknown; header: string | null }[];
}

const PROJECT: ProjectInfo = {
  version: 12,
  compositions: [{ id: "c1", name: "Vlog 14" }],
  sequences: [
    { id: "s1", name: "Main" },
    { id: "s2", name: "Shorts" },
  ],
  open_composition: "c1",
};

const json = (status: number, body: unknown): Response =>
  new Response(JSON.stringify(body), { status, headers: { "Content-Type": "application/json" } });

const errorBody = (status: number, code: string, message: string): Response =>
  json(status, { error: { code, message } });

interface Request1 {
  path: string;
  method: string;
  header: string | null;
  body: unknown;
}

const pair = (req: Request1, setPaired: () => void): Response => {
  if ((req.body as { code?: string } | null)?.code !== MOCK_PAIR_CODE) {
    return errorBody(HTTP_UNAUTHORIZED, "invalid_code", "The code is wrong or has expired.");
  }
  setPaired();
  return json(HTTP_OK, { device_name: (req.body as { device_name: string }).device_name });
};

const api = (server: MockServer, req: Request1): Response => {
  if (req.path === "/m/api/project") return json(HTTP_OK, PROJECT);
  if (req.path === "/m/api/comments" && req.method === "GET") return json(HTTP_OK, server.comments);
  if (req.path === "/m/api/comments") {
    const id = `m${server.comments.length + 1}`;
    server.comments.unshift({ id, text: (req.body as { text: string }).text, state: "open", at: null, author: "You" });
    return json(HTTP_OK, { id });
  }
  if (req.path === "/m/api/ops") return json(HTTP_OK, { version: PROJECT.version + 1 });
  return errorBody(HTTP_NOT_FOUND, "not_found", req.path);
};

const readRequest = (path: string, init?: RequestInit): Request1 => ({
  path,
  method: init?.method ?? "GET",
  header: new Headers(init?.headers).get(MOBILE_HEADER),
  body: typeof init?.body === "string" ? JSON.parse(init.body) : null,
});

export const createMockServer = (init: { paired: boolean; online: boolean; comments?: CommentItem[] }): MockServer => {
  let { paired, online } = init;
  const sources = new Set<EventSourceLike>();
  const server: MockServer = {
    comments: init.comments ?? [],
    writes: [],
    setOnline: (v) => {
      online = v;
      sources.forEach((es) => (v ? es.onopen?.() : es.onerror?.()));
    },
    setPaired: (v) => (paired = v),
    platform: {
      openEvents: () => {
        const es: EventSourceLike = {
          onopen: null,
          onerror: null,
          addEventListener: () => undefined,
          close: () => void sources.delete(es),
        };
        sources.add(es);
        queueMicrotask(() => (online ? es.onopen?.() : es.onerror?.()));
        return es;
      },
      fetch: async (path, reqInit) => {
        if (!online) throw new TypeError("Failed to fetch");
        const req = readRequest(path, reqInit);
        if (req.method === "POST") server.writes.push({ path, body: req.body, header: req.header });
        if (path === "/m/pair") return pair(req, () => (paired = true));
        if (!paired) return errorBody(HTTP_UNAUTHORIZED, "unauthorized", "Device is not paired.");
        if (req.method === "POST" && req.header !== "1") return errorBody(HTTP_FORBIDDEN, "forbidden", "Missing header.");
        return api(server, req);
      },
    },
  };
  return server;
};
