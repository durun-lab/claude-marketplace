import { useEffect, useRef, useState } from "preact/hooks";
import { GraphPanel } from "@mm/graph";
import { useFitToColumn } from "./fit.ts";
import { InspectorPanel } from "./InspectorPanel.tsx";
import { useLayerKinds, type KindLookup } from "./kinds.ts";
import { describeLayer } from "./model.ts";
import type { Asset, CompositionSummary, Layer, Op } from "@mm/sdk";

/** 再生位置を知らせる DOM のイベント。アプリの WebView の橋（`web/transport.js`）が時計の通知ごとに投げる。 */
export const PLAYHEAD_EVENT = "mm:playhead";
/** 再生位置を動かしてほしいときに投げる DOM のイベント（`detail.time` は秒）。`web/transport.js` が受けてシークする。 */
export const SEEK_EVENT = "mm:seek";

export interface Playhead {
  time: number;
  fps: number;
}

/**
 * 再生位置のイベントを受けて持つ。届くまでは null（Inspector はキーフレームナビゲーターを出さない）。
 * 止まっている間も同じ位置が繰り返し届くので、位置が変わったときだけ描き直す（描き直すと入力中の入力欄が元の値に戻る）。
 */
const usePlayhead = (): Playhead | null => {
  const [playhead, setPlayhead] = useState<Playhead | null>(null);
  useEffect(() => {
    const on = (e: Event) => {
      const next = (e as CustomEvent<Playhead>).detail;
      setPlayhead((prev) => (prev && prev.time === next.time && prev.fps === next.fps ? prev : next));
    };
    window.addEventListener(PLAYHEAD_EVENT, on);
    return () => window.removeEventListener(PLAYHEAD_EVENT, on);
  }, []);
  return playhead;
};

const seek = (time: number) => window.dispatchEvent(new CustomEvent(SEEK_EVENT, { detail: { time } }));

export interface KeyframePanelsProps {
  layer: Layer | null;
  assets?: readonly Asset[];
  /** 何も選んでいないときに出す、コンポジションの要約。 */
  composition?: CompositionSummary;
  onOp: (op: Op) => void;
  /** パラメータの定義を引く。渡さなければプロジェクトにある値だけで入力欄を作る。 */
  lookup?: KindLookup;
}

/**
 * 右の列の Inspector と、その下の Graph。Inspector で名前を押したパラメータを Graph に出す。
 * 列の見えている高さに収め、Inspector の行はその中でスクロールして、Graph は下に残す。
 */
export const KeyframePanels = ({ layer, assets, composition, onOp, lookup }: KeyframePanelsProps) => {
  const box = useRef<HTMLDivElement>(null);
  useFitToColumn(box);
  const kinds = useLayerKinds(layer, lookup);
  const playhead = usePlayhead();
  const [path, setPath] = useState<string | null>(null);
  const row =
    layer && path
      ? describeLayer(layer, undefined, kinds).flatMap((s) => s.rows).find((r) => r.path === path)
      : undefined;
  return (
    <div class="mm-insp-panels" ref={box}>
      <div class="mm-insp-scroll">
        <InspectorPanel
          layer={layer}
          assets={assets}
          composition={composition}
          kinds={kinds}
          onOp={onOp}
          time={playhead?.time}
          fps={playhead?.fps}
          onSeek={seek}
          selectedPath={path}
          onSelectParam={setPath}
        />
      </div>
      {layer && (
        <GraphPanel
          target={row ? { layer: layer.id, path: row.path } : null}
          param={row?.param ?? null}
          label={row ? `Graph: ${row.label}` : "Graph"}
          time={playhead?.time}
          fps={playhead?.fps}
          onOp={onOp}
        />
      )}
    </div>
  );
};
