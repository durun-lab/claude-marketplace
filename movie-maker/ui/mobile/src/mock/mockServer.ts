import type { Platform } from "../controller.ts";
import type { EventSourceLike } from "../connection.ts";
import type { CommentItem, CommentState, ProjectInfo } from "../api/types.ts";
import type { Proposal } from "../proposals/api.ts";
import { MOBILE_HEADER } from "../api/http.ts";

/** デモとテストの偽の PC。人の待ち受けの API が出来るまで、12-mobile.md の「HTTP の形」の応答を手元で返す。 */
export const MOCK_PAIR_CODE = "ABCD2345";

const HTTP_OK = 200;
const HTTP_BAD_REQUEST = 400;
const HTTP_UNAUTHORIZED = 401;
const HTTP_FORBIDDEN = 403;
const HTTP_NOT_FOUND = 404;

export interface MockServer {
  platform: Pick<Platform, "fetch" | "openEvents">;
  setOnline: (online: boolean) => void;
  setPaired: (paired: boolean) => void;
  project: ProjectInfo;
  comments: CommentItem[];
  proposals: Proposal[];
  /** PC が受けた書き込みの要求（パスと本文）。 */
  writes: { path: string; body: unknown; header: string | null }[];
  /** PC から SSE のイベントを流す。 */
  emit: (type: string, data: unknown) => void;
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

const MOBILE_AUTHOR = { kind: "mobile", name: "iPhone" };

const createComment = (server: MockServer, body: Record<string, unknown>): Response => {
  const clientId = body.client_id as string | undefined;
  const dup = clientId ? server.comments.find((c) => c.client_id === clientId) : undefined;
  if (dup) return json(HTTP_OK, dup);
  if (!body.text && !(body.drawing as unknown[] | undefined)?.length) {
    return errorBody(HTTP_BAD_REQUEST, "invalid", "A comment needs text or a drawing.");
  }
  const comment = {
    id: `m${server.comments.length + 1}`,
    comp: body.comp as string,
    at: (body.at as CommentItem["at"]) ?? null,
    ...(body.range ? { range: body.range as CommentItem["range"] } : {}),
    text: (body.text as string | undefined) ?? "",
    drawing: (body.drawing as CommentItem["drawing"] | undefined) ?? [],
    author: MOBILE_AUTHOR,
    replies: [],
    state: "open" as const,
    created_ms: Date.now(),
    client_id: clientId,
  };
  server.comments.push(comment);
  return json(HTTP_OK, comment);
};

const commentAction = (server: MockServer, req: Request1, id: string, action: string): Response => {
  const comment = server.comments.find((c) => c.id === id);
  if (!comment) return errorBody(HTTP_NOT_FOUND, "not_found", id);
  const body = req.body as { text?: string; state?: CommentState };
  if (action === "replies") {
    comment.replies.push({ id: `r${comment.replies.length + 1}`, author: MOBILE_AUTHOR, text: body.text ?? "", created_ms: Date.now() });
  } else if (body.state === "fresh") {
    return errorBody(HTTP_BAD_REQUEST, "invalid_state", "fresh is set by a response.");
  } else if (body.state) {
    comment.state = body.state;
  }
  return json(HTTP_OK, comment);
};

const oneComment = (server: MockServer, id: string): Response => {
  const comment = server.comments.find((c) => c.id === id);
  return comment ? json(HTTP_OK, comment) : errorBody(HTTP_NOT_FOUND, "not_found", id);
};

const decide = (server: MockServer, id: number, action: string, body: { ops?: number[] }): Response => {
  const p = server.proposals.find((x) => x.id === id);
  if (!p) return errorBody(HTTP_NOT_FOUND, "not_found", String(id));
  if (action === "approve") {
    Object.assign(p, { state: "approved", decided_by: "mobile", applied: true, undoable: true, approved_ops: body.ops ?? null });
  } else {
    Object.assign(p, { state: action === "reject" ? "rejected" : "pending", decided_by: action === "reject" ? "mobile" : null });
  }
  return json(HTTP_OK, action === "approve" ? { proposal: p, change: null } : p);
};

const undoLast = (server: MockServer): Response => {
  const last = [...server.proposals].reverse().find((p) => p.undoable);
  if (last) Object.assign(last, { state: "pending", applied: false, undoable: false, decided_by: null, approved_ops: null });
  return json(HTTP_OK, { change: null });
};

const COMMENT_PATH = /^\/m\/api\/comments\/([^/]+)\/(replies|state)$/;
const ONE_COMMENT = /^\/m\/api\/comments\/([^/]+)$/;
const PROPOSAL_PATH =/^\/m\/api\/proposals\/(\d+)\/(approve|reject|reopen)$/;

const api = (server: MockServer, req: Request1): Response => {
  const { path, method } = req;
  if (path === "/m/api/project") return json(HTTP_OK, server.project);
  if (path === "/m/api/comments" && method === "GET") return json(HTTP_OK, { comments: server.comments });
  if (path === "/m/api/comments") return createComment(server, req.body as Record<string, unknown>);
  const one = method === "GET" ? ONE_COMMENT.exec(path) : null;
  if (one) return oneComment(server, decodeURIComponent(one[1] ?? ""));
  const c = COMMENT_PATH.exec(path);
  if (c) return commentAction(server, req, decodeURIComponent(c[1] ?? ""), c[2] ?? "");
  if (path === "/m/api/proposals") return json(HTTP_OK, { proposals: server.proposals });
  const p = PROPOSAL_PATH.exec(path);
  if (p) return decide(server, Number(p[1]), p[2] ?? "", (req.body ?? {}) as { ops?: number[] });
  if (path === "/m/api/undo") return undoLast(server);
  if (path === "/m/api/ops") return json(HTTP_OK, { version: server.project.version + 1 });
  return errorBody(HTTP_NOT_FOUND, "not_found", path);
};

const readRequest = (path: string, init?: RequestInit): Request1 => ({
  path,
  method: init?.method ?? "GET",
  header: new Headers(init?.headers).get(MOBILE_HEADER),
  body: typeof init?.body === "string" ? JSON.parse(init.body) : null,
});

type Listener = (ev: { data?: string }) => void;

const createSource = (
  sources: Set<EventSourceLike>,
  handlers: Map<EventSourceLike, Map<string, Listener[]>>,
  online: () => boolean,
): EventSourceLike => {
  const mine = new Map<string, Listener[]>();
  const es: EventSourceLike = {
    onopen: null,
    onerror: null,
    addEventListener: (type, fn) => void mine.set(type, [...(mine.get(type) ?? []), fn]),
    close: () => {
      sources.delete(es);
      handlers.delete(es);
    },
  };
  sources.add(es);
  handlers.set(es, mine);
  queueMicrotask(() => (online() ? es.onopen?.() : es.onerror?.()));
  return es;
};

export const createMockServer = (init: {
  paired: boolean;
  online: boolean;
  comments?: CommentItem[];
  proposals?: Proposal[];
  cuts?: number[];
}): MockServer => {
  let { paired, online } = init;
  const sources = new Set<EventSourceLike>();
  const handlers = new Map<EventSourceLike, Map<string, Listener[]>>();
  const server: MockServer = {
    project: { ...PROJECT, ...(init.cuts ? { cuts: init.cuts } : {}) },
    comments: init.comments ?? [],
    proposals: init.proposals ?? [],
    writes: [],
    emit: (type, data) =>
      handlers.forEach((m) => m.get(type)?.forEach((fn) => fn({ data: JSON.stringify(data) }))),
    setOnline: (v) => {
      online = v;
      sources.forEach((es) => (v ? es.onopen?.() : es.onerror?.()));
    },
    setPaired: (v) => (paired = v),
    platform: {
      openEvents: () => createSource(sources, handlers, () => online),
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
