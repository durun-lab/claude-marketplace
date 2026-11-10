import { useEffect, useRef, useState } from "preact/hooks";
import type { DiagnosticsApi, DiagnosticsInfo, LayerInfo } from "./api.ts";

/** タブを開いている間に読み直す間隔。通知の遅れと GPU の名前は、再生と描画のスレッドが後から変える。 */
export const REFRESH_MS = 1000;

const message = (e: unknown): string => (e instanceof Error ? e.message : String(e));

const sameLayer = (a: LayerInfo, b: LayerInfo | undefined): boolean =>
  b !== undefined && a.id === b.id && a.kind === b.kind && a.start === b.start && a.end === b.end;

/**
 * 読み直した中身。レイヤーの一覧が前と同じなら前の配列を使い、一覧を描き直さない
 * （レイヤーが数千あるプロジェクトでも、毎秒の読み直しが通知の遅れだけを描き直す）。
 */
export const keepLayers = (prev: DiagnosticsInfo | null, next: DiagnosticsInfo): DiagnosticsInfo =>
  prev && prev.layers.length === next.layers.length && next.layers.every((l, i) => sameLayer(l, prev.layers[i]))
    ? { ...next, layers: prev.layers }
    : next;

export interface DiagnosticsView {
  info: DiagnosticsInfo | null;
  error: string | null;
}

/**
 * タブを開いたとき、`refreshKey` が変わったとき、開いている間は `REFRESH_MS` ごとに読む。閉じている間は読まない。
 * 読み直しが重なったら、最後に頼んだものだけを使う。
 */
export const useDiagnostics = (api: DiagnosticsApi, active: boolean, refreshKey: number): DiagnosticsView => {
  const [info, setInfo] = useState<DiagnosticsInfo | null>(null);
  const [error, setError] = useState<string | null>(null);
  const loads = useRef(0);
  useEffect(() => {
    if (!active) return undefined;
    const load = async () => {
      const id = ++loads.current;
      try {
        const next = await api.info();
        if (id !== loads.current) return;
        setInfo((prev) => keepLayers(prev, next));
        setError(null);
      } catch (e) {
        if (id === loads.current) setError(message(e));
      }
    };
    void load();
    const timer = setInterval(() => void load(), REFRESH_MS);
    return () => clearInterval(timer);
  }, [api, active, refreshKey]);
  return { info, error };
};
