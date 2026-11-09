import { useEffect, useMemo, useRef, useState } from "preact/hooks";
import type { Layer, Param, ParamValue } from "@mm/sdk";
import type { ParamHint } from "./model.ts";

/** パラメータの型（Rust の `ParamType`）。範囲と選択肢はここにある。 */
export type KindParamType =
  | { type: "number"; min?: number | null; max?: number | null }
  | { type: "choice"; options: string[] }
  | { type: "vec2" | "vec3" | "color" | "bool" | "text" | "asset_ref" | "json" };

/** ノードの種類が持つ 1 つのパラメータの定義（Rust の `ParamSpec`）。 */
export interface KindParamSpec {
  name: string;
  ty: KindParamType;
  default: ParamValue;
}

/** 種類の一覧の 1 件（Rust の `KindInfo`）。アプリの `inspector_kind` の戻り値で、Inspector が読むのはパラメータの定義だけ。 */
export interface KindInfo {
  id: string;
  params: { params: KindParamSpec[] };
}

/** 種類の名前と、レイヤーにある今の値（パラメータの組）から、パラメータの組ごとの定義を引く。引けなければ reject。 */
export type KindLookup = (kind: string, params: Record<string, ParamValue>) => Promise<KindParamSpec[]>;

/** レイヤーのソースとエフェクトごとの、パラメータの定義。 */
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

/** 値が変わってから引き直すまでの待ち（ミリ秒）。ドラッグの途中の値ごとに引かず、止まってから 1 回だけ引く。 */
export const LOOKUP_DEBOUNCE_MS = 250;

/** 引き直すかを決める、種類と値の組の文字列。 */
const queryKey = (t: Target): string => JSON.stringify([t.kind, t.params]);

/**
 * レイヤーのソースとエフェクトのパラメータの定義を `lookup` で引く。引けた種類だけ入力欄を広げ、
 * 引けなかった種類（`composition` など登録の無いもの、書き手として開いていないプロジェクト）は、プロジェクトにある値だけで出す。
 * 引くのは種類か値が変わったソースとエフェクトだけ。まだ定義の無い種類はすぐ引き、定義のある種類の値の変更は
 * `LOOKUP_DEBOUNCE_MS` 止まってから引く。引き直す間は前の定義を使い続け（入力欄が点滅しない）、古い応答は捨てる。
 */
export const useLayerKinds = (layer: Layer | null, lookup: KindLookup | undefined): LayerKinds => {
  const [resolved, setResolved] = useState<Record<string, Resolved>>({});
  const latest = useRef(resolved);
  latest.current = resolved;
  /** ソースかエフェクトごとの、最後に送った問い合わせ。応答がこれと違えば古い。 */
  const sent = useRef<Record<string, string>>({});
  const targets = layer && lookup ? targetsOf(layer) : [];
  const signature = JSON.stringify(targets);
  useEffect(() => {
    if (!lookup) return;
    const pending = (JSON.parse(signature) as Target[]).filter((t) => sent.current[t.key] !== queryKey(t));
    if (pending.length === 0) return;
    const known = pending.every((t) => latest.current[t.key]?.kind === t.kind);
    const timer = setTimeout(
      () => {
        for (const t of pending) {
          const key = queryKey(t);
          sent.current[t.key] = key;
          lookup(t.kind, t.params).then(
            (specs) => {
              if (sent.current[t.key] === key) setResolved((prev) => ({ ...prev, [t.key]: { kind: t.kind, specs } }));
            },
            () => undefined,
          );
        }
      },
      known ? LOOKUP_DEBOUNCE_MS : 0,
    );
    return () => clearTimeout(timer);
  }, [signature, lookup]);
  // 再生位置が届くたびに呼ばれる。レイヤーと定義が変わるまで同じオブジェクトを返し、Inspector の区画を作り直させない。
  return useMemo(() => {
    if (!layer) return NO_KINDS;
    const live = (t: Target): readonly KindParamSpec[] | undefined => {
      const r = resolved[t.key];
      return r && r.kind === t.kind ? r.specs : undefined;
    };
    const [source, ...effects] = lookup ? targetsOf(layer) : [];
    return {
      source: source && live(source),
      effects: Object.fromEntries(
        effects.flatMap((t) => {
          const specs = live(t);
          return specs ? [[t.key.slice("effect:".length), specs]] : [];
        }),
      ),
    };
  }, [layer, lookup, resolved]);
};
