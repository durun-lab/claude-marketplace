import { describe, expect, it } from "vitest";
import { errorMessage } from "./errors.ts";

describe("失敗の文言", () => {
  it("人の待ち受けの code は英語の文言にする", () => {
    expect(errorMessage("pairing_wrong_code", 401)).toMatch(/^That code is wrong/);
    expect(errorMessage("name_taken", 409)).toMatch(/different name/);
    expect(errorMessage("not_on_mobile", 403)).toMatch(/Decide on PC/);
    expect(errorMessage("not_latest", 409)).toMatch(/can't be undone/);
  });

  it("知らない code は HTTP の状態つきの汎用の文言、通信の失敗はつながらない旨にする", () => {
    expect(errorMessage("teapot", 418)).toBe("The PC could not do this (HTTP 418).");
    expect(errorMessage("network", 0)).toMatch(/Can't reach the PC/);
    expect(errorMessage("whatever", 0)).toMatch(/Can't reach the PC/);
  });
});
