import { describe, expect, it } from "vitest";
import { isCacheable, staleCaches } from "./policy.ts";

const origin = "https://pc.tail.ts.net:8443";
const can = (method: string, path: string, base = origin) => isCacheable(method, new URL(path, base), origin);

describe("isCacheable", () => {
  it("画面の資源は対象にする", () => {
    expect(can("GET", "/m/app.js")).toBe(true);
    expect(can("GET", "/m/")).toBe(true);
  });
  it("API、登録、書き込み、ほかのオリジンは対象にしない", () => {
    expect(can("GET", "/m/api/project")).toBe(false);
    expect(can("GET", "/m/api/frame?t=1")).toBe(false);
    expect(can("POST", "/m/pair")).toBe(false);
    expect(can("GET", "/m/pair")).toBe(false);
    expect(can("POST", "/m/app.js")).toBe(false);
    expect(can("GET", "https://example.com/m/app.js", "https://example.com")).toBe(false);
  });
});

describe("staleCaches", () => {
  it("今のバージョン以外の mm-mobile のキャッシュだけを消す", () => {
    expect(staleCaches(["mm-mobile-a", "mm-mobile-b", "other"], "mm-mobile-b")).toEqual(["mm-mobile-a"]);
  });
});
