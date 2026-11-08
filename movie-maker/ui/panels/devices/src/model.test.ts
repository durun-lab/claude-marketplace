import { describe, expect, it } from "vitest";
import { formatCode, formatPairedAt, remaining } from "./model.ts";

describe("登録コードと時刻の表示", () => {
  it("8 文字の登録コードを 4 文字ずつに分ける", () => {
    expect(formatCode("ABCD2345")).toBe("ABCD 2345");
  });

  it("残りは分と秒で、過ぎたら null", () => {
    const expires = 1_000_000;
    expect(remaining(expires, expires - 272_000)).toBe("4:32");
    expect(remaining(expires, expires - 59_500)).toBe("1:00");
    expect(remaining(expires, expires - 1)).toBe("0:01");
    expect(remaining(expires, expires)).toBeNull();
    expect(remaining(expires, expires + 5)).toBeNull();
  });

  it("登録した日時はこの PC の時刻で分まで出す", () => {
    expect(formatPairedAt(new Date(2026, 10, 8, 9, 5).getTime())).toBe("2026-11-08 09:05");
  });
});
