import { useEffect, useState } from "preact/hooks";
import type { Layer, Param, ParamValue } from "@mm/sdk";
import type { ParamHint } from "./model.ts";

/** パラメータの型（Rust の `ParamType`）。範囲と選択肢はここにある。 */
export type KindParamType =
  | { type: "number"; min?: number | null; max?: number | null }
  | { type: "choice"; options: string[] }
  | { type: "vec2" | "vec3" | "color" | "bool" | "text" | "asset_ref" | "json" };

/** 種類が持つ 1 つのパラメータの定義（Rust の `ParamSpec`）。 */
export interface KindParamSpec {
  name: string;
  ty: KindParamType;
  default: ParamValue;
}

/** 種類の定義。Inspector が読むのはパラメータの一覧だけ。アプリの `inspector_kind` の戻り値。 */
export interface KindInfo {
  id: string;
  params: { params: KindParamSpec[] };
}

/** 種類の名前と、レイヤーにある今の値から、その種類のパラメータの定義を引く。引けなければ reject。 */
export type KindLookup = (kind: string, params: Record<string, ParamValue>) => Promise<KindParamSpec[]>;

/** レイヤーのソースとエフェクトごとの、種類のパラメータの定義。 */
export interface LayerKinds {
  source?: readonly KindParamSpec[];
  /** エフェクトの ID から。 */
  effects: Readonly<Record<string, readonly KindParamSpec[]>>;
}

export const NO_KINDS: LayerKinds = { effects: {} };

/** 入力欄に渡す、型に含まれる範囲と選択肢。 */
export const hintOf = (spec: KindParamSpec): ParamHint => {
  const ty = spec.ty;
  if (ty.type === "number") {
    return {
      ...(typeof ty.min === "number" ? { min: ty.min } : {}),
      ...(typeof ty.max === "number" ? { max: ty.max } : {}),
    };
  }
  if (ty.type === "choice") return { choices: ty.options };
  return {};
};

const plainValues = (params: Record<string, Param> | undefined): Record<string, ParamValue> =>
  Object.fromEntries(Object.entries(params ?? {}).map(([name, p]) => [name, p.keyframes?.[0]?.value ?? p.value]));

interface Target {
  /** `source` かエフェクトの ID。 */
  key: string;
  kind: string;
  params: Record<string, ParamValue>;
}

const targetsOf = (layer: Layer): Target[] => [
  { key: "source", kind: layer.source.kind, params: plainValues(layer.source.params) },
  ...(layer.effects ?? []).map((e) => ({ key: `effect:${e.id}`, kind: e.node.kind, params: plainValues(e.node.params) })),
];

interface Resolved {
  kind: string;
  specs: readonly KindParamSpec[];
}

/**
 * レイヤーのソースとエフェクトの種類の定義を `lookup` で引く。引けた種類だけ入力欄を広げ、
 * 引けなかった種類（`composition` など登録の無いもの）は、プロジェクトにある値だけで出す。
 * 値が変わって引き直す間は、前に引けた定義を使い続ける（入力欄が点滅しない）。
 */
export const useLayerKinds = (layer: Layer | null, lookup: KindLookup | undefined): LayerKinds => {
  const [resolved, setResolved] = useState<Record<string, Resolved>>({});
  const targets = layer && lookup ? targetsOf(layer) : [];
  const signature = JSON.stringify([layer?.id, targets]);
  useEffect(() => {
    if (!lookup) return;
    let cancelled = false;
    const [, queries] = JSON.parse(signature) as [string, Target[]];
    for (const t of queries) {
      lookup(t.kind, t.params).then(
        (specs) => {
          if (!cancelled) setResolved((prev) => ({ ...prev, [t.key]: { kind: t.kind, specs } }));
        },
        () => undefined,
      );
    }
    return () => {
      cancelled = true;
    };
  }, [signature, lookup]);
  if (!layer) return NO_KINDS;
  const live = (t: Target): readonly KindParamSpec[] | undefined => {
    const r = resolved[t.key];
    return r && r.kind === t.kind ? r.specs : undefined;
  };
  const [source, ...effects] = targets;
  return {
    source: source && live(source),
    effects: Object.fromEntries(
      effects.flatMap((t) => {
        const specs = live(t);
        return specs ? [[t.key.slice("effect:".length), specs]] : [];
      }),
    ),
  };
};
