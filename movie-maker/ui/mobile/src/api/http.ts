/** 人の待ち受けのパスの根。`GET /m/` の画面と API が同じオリジンで同じ根の下にある。 */
export const MOBILE_ROOT = "/m";
export const API_ROOT = `${MOBILE_ROOT}/api`;
export const PAIR_PATH = `${MOBILE_ROOT}/pair`;

/** 書き込みの要求に付ける見出し。別のオリジンのフォームからの送信を母艦が止める。 */
export const MOBILE_HEADER = "X-MM-Mobile";

export type FetchLike = (input: string, init?: RequestInit) => Promise<Response>;

const SERVER_ERROR = 500;
const TOO_MANY_REQUESTS = 429;

/** `status` が 0 のときは通信そのものの失敗（圏外、母艦が止まっている）。 */
export class ApiError extends Error {
  constructor(
    readonly status: number,
    readonly code: string,
    message: string,
  ) {
    super(message);
    this.name = "ApiError";
  }
}

/** 再送で解消し得る失敗。通信の失敗、混雑、母艦の内部の失敗。 */
export const isRetryable = (e: unknown): boolean =>
  e instanceof ApiError && (e.status === 0 || e.status === TOO_MANY_REQUESTS || e.status >= SERVER_ERROR);

const UNAUTHORIZED = 401;
const FORBIDDEN = 403;

/** 資格が無い、または取り消された（401）か、待ち受けに拒まれた（403）。登録の画面へ送る。 */
export const isUnauthorized = (e: unknown): boolean =>
  e instanceof ApiError && (e.status === UNAUTHORIZED || e.status === FORBIDDEN);

export interface Api {
  get: <T>(path: string) => Promise<T>;
  post: <T>(path: string, body: unknown) => Promise<T>;
}

const failure = async (res: Response): Promise<ApiError> => {
  const body: unknown = await res.json().catch(() => null);
  const err = (body as { error?: { code?: string; message?: string } } | null)?.error;
  return new ApiError(res.status, err?.code ?? "http", err?.message ?? (res.statusText || `HTTP ${res.status}`));
};

const call = async <T>(fetcher: FetchLike, path: string, init: RequestInit): Promise<T> => {
  let res: Response;
  try {
    res = await fetcher(path, { ...init, credentials: "same-origin" });
  } catch (e) {
    throw new ApiError(0, "network", e instanceof Error ? e.message : String(e));
  }
  if (!res.ok) throw await failure(res);
  const text = await res.text();
  return (text ? JSON.parse(text) : null) as T;
};

/** 資格のクッキーを付け、書き込みには `X-MM-Mobile: 1` を付ける HTTP の呼び出し。 */
export const createApi = (fetcher: FetchLike): Api => ({
  get: (path) => call(fetcher, path, { method: "GET" }),
  post: (path, body) =>
    call(fetcher, path, {
      method: "POST",
      headers: { "Content-Type": "application/json", [MOBILE_HEADER]: "1" },
      body: JSON.stringify(body),
    }),
});
