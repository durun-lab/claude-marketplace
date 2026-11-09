import { describe, expect, it, vi } from "vitest";
import type { MediaListen } from "@mm/sdk";
import { createIpcFeedbackApi, FEEDBACK_CHANGED_EVENT } from "./api.ts";

describe("createIpcFeedbackApi", () => {
  it("コマンドの名前と引数をアプリの形で渡す", async () => {
    const call = vi.fn().mockResolvedValue(null);
    const api = createIpcFeedbackApi(call, vi.fn());
    const feedback = { text: "x", targets: [], context: {} };
    await api.status();
    await api.list("pending");
    await api.get("id1");
    await api.submit(feedback);
    await api.settings({ enabled: true, budgetUsd: 1 });
    await api.stop();
    await api.capture();
    await api.reportError({ message: "TypeError: x", where: "a.js:1:2" });
    expect(call.mock.calls).toEqual([
      ["feedback_status", undefined],
      ["feedback_list", { filter: "pending" }],
      ["feedback_get", { id: "id1" }],
      ["feedback_submit", { feedback }],
      ["feedback_settings", { patch: { enabled: true, budgetUsd: 1 } }],
      ["feedback_stop", undefined],
      ["feedback_capture", undefined],
      ["feedback_report_error", { error: { source: "webview", message: "TypeError: x", where: "a.js:1:2" } }],
    ]);
  });

  it("アプリの失敗（文字列）を理由つきの Error にする", async () => {
    const api = createIpcFeedbackApi(vi.fn().mockRejectedValue("Auto-improve is unavailable"), vi.fn());
    await expect(api.status()).rejects.toThrow("Auto-improve is unavailable");
  });

  it("mm:feedback-changed を受ける", async () => {
    let fire: () => void = () => undefined;
    const off = vi.fn();
    const listen: MediaListen = async (name, h) => {
      expect(name).toBe(FEEDBACK_CHANGED_EVENT);
      fire = () => h({ payload: null });
      return off;
    };
    const api = createIpcFeedbackApi(vi.fn(), listen);
    const handler = vi.fn();
    const unlisten = await api.onChanged(handler);
    fire();
    expect(handler).toHaveBeenCalledTimes(1);
    unlisten();
    expect(off).toHaveBeenCalled();
  });
});
