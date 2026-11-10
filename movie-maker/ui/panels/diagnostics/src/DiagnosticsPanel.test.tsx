// @vitest-environment jsdom
import { render } from "preact";
import { act } from "preact/test-utils";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { DiagnosticsApi, DiagnosticsInfo } from "./api.ts";
import { createIpcDiagnosticsApi } from "./api.ts";
import { DiagnosticsPanel } from "./DiagnosticsPanel.tsx";
import { compositionText, rangeText } from "./model.ts";
import { keepLayers, REFRESH_MS } from "./useDiagnostics.ts";

const host = document.createElement("div");
document.body.append(host);
afterEach(() => {
  render(null, host);
  vi.useRealTimers();
});

const INFO: DiagnosticsInfo = {
  name: "ep03",
  composition: "main",
  width: 1920,
  height: 1080,
  fps: 30000 / 1001,
  duration: 578.4,
  clock: "audio",
  audio_device: "Speakers (48000 Hz, 2 ch)",
  backends: [
    ["render", "wgpu"],
    ["decode", "ffmpeg-d3d12va"],
  ],
  layers: [
    { id: "fxtext-32", kind: "image", start: 527.81, end: 527.93 },
    { id: "main-seq", kind: "sequence", start: 0, end: 578.4 },
  ],
  adapter: "NVIDIA GeForce RTX 5090 (Dx12)",
  lag_ms: 3.25,
};

const fakeApi = (info: DiagnosticsInfo = INFO): DiagnosticsApi => ({ info: vi.fn(async () => info) });

const flush = () => act(() => new Promise<void>((ok) => setTimeout(ok, 0)));
const mount = async (api: DiagnosticsApi, active = true, refreshKey = 0) => {
  act(() => render(<DiagnosticsPanel api={api} active={active} refreshKey={refreshKey} />, host));
  await flush();
};
/** `<dl>` の名前と値の組。 */
const rows = (label: string): [string, string][] => {
  const list = host.querySelector(`dl[aria-label="${label}"]`);
  const names = [...(list?.querySelectorAll("dt") ?? [])].map((dt) => dt.textContent ?? "");
  const values = [...(list?.querySelectorAll("dd") ?? [])].map((dd) => dd.textContent ?? "");
  return names.map((n, i) => [n, values[i] ?? ""]);
};

describe("DiagnosticsPanel の中身", () => {
  it("GPU を先頭に、時計、音の出力、通知の遅れ、プロジェクトとコンポジションを出す", async () => {
    await mount(fakeApi());
    expect(rows("Summary")).toEqual([
      ["GPU", "NVIDIA GeForce RTX 5090 (Dx12)"],
      ["Clock", "Audio Output"],
      ["Audio Output", "Speakers (48000 Hz, 2 ch)"],
      ["Sync Lag", "3.3 ms"],
      ["Project", "ep03"],
      ["Composition", "main 1920x1080 29.97 fps"],
    ]);
  });

  it("選ばれた実装を名前ごとの行で出す", async () => {
    await mount(fakeApi());
    expect(rows("Backends")).toEqual([
      ["render", "wgpu"],
      ["decode", "ffmpeg-d3d12va"],
    ]);
  });

  it("レイヤーの生の一覧に種類、ID、時間の範囲を上のレイヤーから並べる", async () => {
    await mount(fakeApi());
    const lines = [...host.querySelectorAll(".mm-diag-layer")].map((li) =>
      [...li.querySelectorAll("span")].map((s) => s.textContent),
    );
    expect(lines).toEqual([
      ["image", "fxtext-32", "527.810-527.930s"],
      ["sequence", "main-seq", "0.000-578.400s"],
    ]);
    expect(host.textContent).toContain("Layers (2)");
  });

  it("音の出力が無く GPU を選ぶ前は、壁時計と未選択を出す", async () => {
    await mount(fakeApi({ ...INFO, clock: "wall", audio_device: null, adapter: "" }));
    const summary = Object.fromEntries(rows("Summary"));
    expect(summary["GPU"]).toBe("Not selected yet");
    expect(summary["Clock"]).toBe("System clock (no audio output)");
    expect(summary["Audio Output"]).toBe("None");
  });
});

describe("DiagnosticsPanel の読み込み", () => {
  it("タブが選ばれていない間は読まず、選ばれると読む", async () => {
    const api = fakeApi();
    await mount(api, false);
    expect(api.info).not.toHaveBeenCalled();
    await mount(api, true);
    expect(api.info).toHaveBeenCalledTimes(1);
  });

  it("開いている間は一定の間隔で読み直し、閉じると止める", async () => {
    vi.useFakeTimers();
    const api = fakeApi();
    const advance = (ms: number) =>
      act(async () => {
        await vi.advanceTimersByTimeAsync(ms);
      });
    act(() => render(<DiagnosticsPanel api={api} active={true} />, host));
    await advance(REFRESH_MS * 2);
    expect(api.info).toHaveBeenCalledTimes(3);
    act(() => render(<DiagnosticsPanel api={api} active={false} />, host));
    await advance(REFRESH_MS * 2);
    expect(api.info).toHaveBeenCalledTimes(3);
  });

  it("読み直しの合図が変わるとすぐ読み、通知の遅れを描き直す", async () => {
    const api: DiagnosticsApi = {
      info: vi.fn().mockResolvedValueOnce(INFO).mockResolvedValue({ ...INFO, lag_ms: 9 }),
    };
    await mount(api, true, 0);
    await mount(api, true, 1);
    expect(api.info).toHaveBeenCalledTimes(2);
    expect(Object.fromEntries(rows("Summary"))["Sync Lag"]).toBe("9.0 ms");
  });

  it("読めなければ理由を出す", async () => {
    await mount({ info: vi.fn(async () => Promise.reject(new Error("No project open"))) });
    expect(host.querySelector("[role=alert]")?.textContent).toBe("No project open");
  });
});

describe("Diagnostics の文言と IPC", () => {
  it("読み直したレイヤーの一覧が同じなら前の配列を使い、変わったら新しい配列にする", () => {
    const next = { ...INFO, lag_ms: 9, layers: INFO.layers.map((l) => ({ ...l })) };
    expect(keepLayers(INFO, next).layers).toBe(INFO.layers);
    expect(keepLayers(INFO, next).lag_ms).toBe(9);
    const moved = { ...next, layers: [{ id: "fxtext-32", kind: "image", start: 1, end: 527.93 }, ...INFO.layers.slice(1)] };
    expect(keepLayers(INFO, moved).layers).toBe(moved.layers);
    expect(keepLayers(null, next)).toBe(next);
  });

  it("fps は割り切れない桁を出さず、範囲はミリ秒まで出して 1 フレームのレイヤーも長さが見える", () => {
    expect(compositionText({ ...INFO, fps: 30 })).toBe("main 1920x1080 30 fps");
    expect(rangeText({ id: "a", kind: "text", start: 1.25, end: 2 })).toBe("1.250-2.000s");
    expect(rangeText({ id: "b", kind: "sequence", start: 0, end: 1 / 30 })).toBe("0.000-0.033s");
  });

  it("project_info を呼び、拒否された文字は Error にする", async () => {
    const call = vi.fn(async () => INFO);
    await expect(createIpcDiagnosticsApi(call).info()).resolves.toBe(INFO);
    expect(call).toHaveBeenCalledWith("project_info");
    const reject = vi.fn(async () => Promise.reject("No project open"));
    await expect(createIpcDiagnosticsApi(reject).info()).rejects.toThrow("No project open");
  });
});
