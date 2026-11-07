// @vitest-environment jsdom
import { render } from "preact";
import { act } from "preact/test-utils";
import type { Layer, Op } from "@mm/sdk";
import { afterEach, describe, expect, it, vi } from "vitest";
import { InspectorPanel } from "./InspectorPanel.tsx";
import { KeyframePanels } from "./KeyframePanels.tsx";
import { hintOf, type KindLookup, type KindParamSpec, type LayerKinds } from "./kinds.ts";
import { createIpcKindLookup } from "./kindsApi.ts";
import { describeLayer } from "./model.ts";

const host = document.createElement("div");
document.body.append(host);
afterEach(() => render(null, host));

const t0 = { num: 0, den: 1 };

const layer = {
  id: "l1",
  start: t0,
  in: t0,
  out: { num: 1, den: 1 },
  opacity: { value: { type: "number", value: 1 } },
  source: {
    kind: "lottie",
    version: 1,
    params: { speed: { value: { type: "number", value: 2 } }, stale: { value: { type: "text", value: "x" } } },
  },
  effects: [{ id: "fx1", node: { kind: "ofx", version: 1, params: {} } }],
} as unknown as Layer;

const specs: KindParamSpec[] = [
  { name: "speed", ty: { type: "number", min: 0, max: 4 }, default: { type: "number", value: 1 } },
  { name: "loop", ty: { type: "bool" }, default: { type: "bool", value: true } },
  { name: "mode", ty: { type: "choice", options: ["fit", "fill"] }, default: { type: "choice", value: "fit" } },
  { name: "tint", ty: { type: "color" }, default: { type: "color", value: { r: 1, g: 0, b: 0, a: 1 } } },
];

const kinds: LayerKinds = { source: specs, effects: { fx1: [specs[1] as KindParamSpec] } };

const input = (label: string) => host.querySelector<HTMLInputElement | HTMLSelectElement>(`[aria-label="${label}"]`);

describe("describeLayer と種類の定義", () => {
  const source = (k: LayerKinds) => describeLayer(layer, {}, k).find((s) => s.id === "source")?.rows ?? [];

  it("値の無いパラメータを既定値の行にし、定義の順に並べ、定義に無い値は後ろに残す", () => {
    const rows = source(kinds);
    expect(rows.map((r) => [r.path, r.isDefault])).toEqual([
      ["source.speed", false],
      ["source.loop", true],
      ["source.mode", true],
      ["source.tint", true],
      ["source.stale", undefined],
    ]);
    expect(rows[1]?.param.value).toEqual({ type: "bool", value: true });
  });

  it("定義が無ければプロジェクトの値だけ", () => {
    expect(source({ effects: {} }).map((r) => r.path)).toEqual(["source.speed", "source.stale"]);
  });

  it("型の範囲と選択肢を入力欄の補足にし、明示の補足を優先する", () => {
    const rows = describeLayer(layer, { "source.speed": { max: 8 } }, kinds).find((s) => s.id === "source")?.rows ?? [];
    expect(rows[0]?.hint).toEqual({ min: 0, max: 8 });
    expect(rows[2]?.hint.choices).toEqual(["fit", "fill"]);
    expect(hintOf(specs[1] as KindParamSpec)).toEqual({});
  });

  it("エフェクトのパラメータも定義から出す", () => {
    const fx = describeLayer(layer, {}, kinds).find((s) => s.id === "effect-fx1");
    expect((fx?.rows ?? []).map((r) => r.path)).toEqual(["effects.fx1.loop"]);
  });
});

describe("InspectorPanel の既定値の行", () => {
  it("型ごとの入力欄が出て、触ると setParam で値を書く", () => {
    const ops: Op[] = [];
    render(<InspectorPanel layer={layer} kinds={kinds} onOp={(op) => ops.push(op)} />, host);
    const check = input("loop") as HTMLInputElement;
    expect(check.type).toBe("checkbox");
    expect(check.checked).toBe(true);
    expect((input("mode") as HTMLSelectElement).options.length).toBe(2);
    expect((input("tint") as HTMLInputElement).type).toBe("color");
    expect(host.querySelector('[data-default="true"]')).not.toBeNull();

    act(() => {
      check.checked = false;
      check.dispatchEvent(new Event("change", { bubbles: true }));
    });
    expect(ops).toEqual([
      { op: "setParam", target: { layer: "l1", path: "source.loop" }, value: { type: "bool", value: false } },
    ]);
  });

  it("choice を選ぶと choice の値を書く", () => {
    const ops: Op[] = [];
    render(<InspectorPanel layer={layer} kinds={kinds} onOp={(op) => ops.push(op)} />, host);
    const select = input("mode") as HTMLSelectElement;
    act(() => {
      select.value = "fill";
      select.dispatchEvent(new Event("change", { bubbles: true }));
    });
    expect(ops[0]).toMatchObject({ value: { type: "choice", value: "fill" } });
  });
});

describe("KeyframePanels の種類の引き", () => {
  it("レイヤーの種類と今の値で引き、引けた定義を入力欄に出す。引けない種類は無視する", async () => {
    const lookup = vi.fn<KindLookup>(async (kind) => {
      if (kind === "lottie") return specs;
      throw new Error("登録が無い");
    });
    act(() => {
      render(<KeyframePanels layer={layer} onOp={() => undefined} lookup={lookup} />, host);
    });
    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 0));
    });
    expect(lookup).toHaveBeenCalledWith("lottie", { speed: { type: "number", value: 2 }, stale: { type: "text", value: "x" } });
    expect(lookup).toHaveBeenCalledWith("ofx", {});
    expect(input("loop")).not.toBeNull();
    expect(input("speed")).not.toBeNull();
  });
});

describe("createIpcKindLookup", () => {
  it("inspector_kind を呼び、パラメータの一覧を返す", async () => {
    const call = vi.fn(async () => ({ id: "lottie", params: { params: specs } }));
    const lookup = createIpcKindLookup(call);
    await expect(lookup("lottie", {})).resolves.toEqual(specs);
    expect(call).toHaveBeenCalledWith("inspector_kind", { kind: "lottie", params: {} });
  });
});
