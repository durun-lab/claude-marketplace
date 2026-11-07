import { describe, expect, it, vi } from "vitest";
import { errorMessage } from "./errors.ts";
import { ApiError, MOBILE_HEADER, createApi, isRetryable, type FetchLike } from "./http.ts";
import { createMobileTransport } from "./transport.ts";

const ok = (body: unknown, status = 200) => new Response(JSON.stringify(body), { status });

describe("createApi", () => {
  it("書き込みには X-MM-Mobile を付け、読み出しには付けない", async () => {
    const fetcher = vi.fn<FetchLike>(async () => ok({}));
    const api = createApi(fetcher);
    await api.get("/m/api/project");
    await api.post("/m/api/comments", { text: "a" });
    const [get, post] = fetcher.mock.calls.map((c) => c[1]);
    expect(new Headers(get?.headers).get(MOBILE_HEADER)).toBeNull();
    expect(new Headers(post?.headers).get(MOBILE_HEADER)).toBe("1");
    expect(post?.credentials).toBe("same-origin");
  });

  it("PC の失敗は status と code つきの ApiError にし、画面の文言は code から英語で引く", async () => {
    const api = createApi(async () => ok({ error: { code: "not_latest", message: "stale" } }, 409));
    await expect(api.post("/m/api/undo", {})).rejects.toMatchObject({ status: 409, code: "not_latest", detail: "stale", message: errorMessage("not_latest", 409) });
  });

  it("通信の失敗は status 0 で、再送できる失敗に数える", async () => {
    const api = createApi(async () => {
      throw new TypeError("Failed to fetch");
    });
    const e = await api.get("/m/api/project").catch((x: unknown) => x);
    expect(e).toBeInstanceOf(ApiError);
    expect(isRetryable(e)).toBe(true);
    expect(isRetryable(new ApiError(403, "forbidden", ""))).toBe(false);
    expect(isRetryable(new ApiError(503, "busy", ""))).toBe(true);
  });
});

describe("createMobileTransport", () => {
  it("操作を base_version つきで /m/api/ops に送る", async () => {
    const fetcher = vi.fn<FetchLike>(async () => ok({ version: 8 }));
    const t = createMobileTransport(createApi(fetcher), () => 7);
    const res = await t.send({ op: "addMarker" } as never);
    expect(res).toEqual({ ok: true, value: { version: 8 } });
    expect(fetcher.mock.calls[0]?.[0]).toBe("/m/api/ops");
    expect(JSON.parse(String(fetcher.mock.calls[0]?.[1]?.body))).toEqual({ op: { op: "addMarker" }, base_version: 7 });
  });

  it("403 not_on_mobile は error の文字列で返す", async () => {
    const api = createApi(async () => ok({ error: { code: "not_on_mobile", message: "not on mobile" } }, 403));
    const res = await createMobileTransport(api, () => 1).send({ op: "trim" } as never);
    expect(res).toEqual({ ok: false, error: errorMessage("not_on_mobile", 403) });
  });
});
