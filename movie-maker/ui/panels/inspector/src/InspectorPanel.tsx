import { useState } from "preact/hooks";
import { type Asset, type Layer, type Op, type ParamRef, type ParamValue, evalParam } from "@mm/sdk";
import { Editor } from "./editors.tsx";
import { AddMaskButton, FadeRows, RemoveMaskButton } from "./Extras.tsx";
import { fieldSpecFor } from "./fields.ts";
import { KeyframeNav } from "./KeyframeNav.tsx";
import { KeyframeStrip, KeyframeSummary, layerSpan, type LayerSpan } from "./KeyframeStrip.tsx";
import type { LayerKinds } from "./kinds.ts";
import { valueAtOp, type Playhead } from "./keyframes.ts";
import {
  DEFAULT_HINTS,
  describeLayer,
  hasKeyframes,
  setParamOp,
  shownValue,
  type FieldRow,
  type ParamHints,
  type Section,
} from "./model.ts";

/** fps が渡されないときのフレームの数え方。 */
const DEFAULT_FPS = 30;

export interface InspectorPanelProps {
  /** 選んだレイヤー。無ければ空の表示。 */
  layer: Layer | null;
  /** `asset_ref` の選択肢。 */
  assets?: readonly Asset[];
  hints?: ParamHints;
  /** ソースとエフェクトのパラメータの定義。渡すと、プロジェクトに値の無いパラメータも既定値で出す。 */
  kinds?: LayerKinds;
  /** 値を変えたときに操作を渡す。送るのは呼び出し側（sdk の client）。 */
  onOp: (op: Op) => void;
  /** 再生位置（コンポジションの秒）。渡すとキーフレームナビゲーターを出し、動きのある値を再生位置で編集する。 */
  time?: number;
  /** コンポジションの fps。キーフレームの時刻をフレームにそろえる。 */
  fps?: number;
  /** 前後のキーフレームへ再生位置を動かす。 */
  onSeek?: (sec: number) => void;
  /** Graph に出しているパラメータのパス。 */
  selectedPath?: string | null;
  /** 行の名前を押したときに、そのパラメータを Graph に出す。 */
  onSelectParam?: (path: string) => void;
}

interface RowViewProps {
  row: FieldRow;
  target: ParamRef;
  assets: readonly Asset[];
  at: Playhead | null;
  span: LayerSpan;
  selected: boolean;
  onOp: (op: Op) => void;
  onSeek?: (sec: number) => void;
  onSelect?: () => void;
}

const KeyframeMark = ({ count }: { count: number }) => (
  <span
    class="mm-insp-kf"
    role="img"
    title={`${count} keyframes`}
    aria-label={`${count} keyframes`}
  />
);

const RowLabel = ({ row, onSelect }: { row: FieldRow; onSelect?: () => void }) => {
  const count = row.param.keyframes?.length ?? 0;
  const body = (
    <>
      {count > 0 && <KeyframeMark count={count} />}
      <span>{row.label}</span>
    </>
  );
  if (!onSelect) return <label class="mm-insp-label">{body}</label>;
  return (
    <button type="button" class="mm-insp-label mm-insp-pick" title="Show in Graph" onClick={onSelect}>
      {body}
    </button>
  );
};

const RowView = ({ row, target, assets, at, span, selected, onOp, onSeek, onSelect }: RowViewProps) => {
  const animated = hasKeyframes(row.param);
  const value: ParamValue = at && animated ? evalParam(row.param, at.sec) : shownValue(row.param);
  const commit = (next: ParamValue) =>
    onOp(animated && at ? valueAtOp(target, row.param, at, next) : setParamOp(target.layer ?? "", row.path, next));
  return (
    <div class="mm-insp-row" data-animated={animated} data-selected={selected} data-default={row.isDefault === true}>
      <RowLabel row={row} onSelect={onSelect} />
      <div class="mm-insp-value">
        <Editor
          spec={fieldSpecFor(value.type)}
          value={value}
          hint={row.hint}
          assets={assets}
          disabled={animated && !at}
          label={row.label}
          onCommit={commit}
        />
        {at && <KeyframeNav target={target} param={row.param} at={at} label={row.label} onOp={onOp} onSeek={onSeek} />}
      </div>
      <KeyframeStrip
        target={target}
        param={row.param}
        label={row.label}
        span={span}
        fps={at?.fps ?? DEFAULT_FPS}
        playheadSec={at?.sec}
        onOp={onOp}
        onSeek={onSeek}
      />
    </div>
  );
};

interface SectionViewProps {
  layer: Layer;
  section: Section;
  assets: readonly Asset[];
  at: Playhead | null;
  span: LayerSpan;
  /** 音量の区画にフェードの行を出すか（Show Only Keyframed Properties の間は出さない）。 */
  showFades: boolean;
  selectedPath?: string | null;
  onOp: (op: Op) => void;
  onSeek?: (sec: number) => void;
  onSelectParam?: (path: string) => void;
}

/** 区画の見出し（右にキーフレームのまとめの欄）と、パラメータの行。 */
const SectionView = ({ layer, section, assets, at, span, showFades, selectedPath, onOp, onSeek, onSelectParam }: SectionViewProps) => (
  <section class="mm-insp-section">
    <h3 class="mm-insp-title">
      <span class="mm-insp-titletext">
        {section.title}
        {section.maskId !== undefined && <RemoveMaskButton maskId={section.maskId} onOp={onOp} />}
      </span>
      <KeyframeSummary
        span={span}
        times={section.rows.flatMap((r) => (r.param.keyframes ?? []).map((k) => k.time))}
        playheadSec={at?.sec}
      />
    </h3>
    <div class="mm-insp-body">
      {section.rows.map((row) => (
        <RowView
          key={`${layer.id}:${row.path}`}
          row={row}
          target={{ layer: layer.id, path: row.path }}
          assets={assets}
          at={at}
          span={span}
          selected={selectedPath === row.path}
          onOp={onOp}
          onSeek={onSeek}
          onSelect={onSelectParam && (() => onSelectParam(row.path))}
        />
      ))}
      {section.id === "audio" && showFades && <FadeRows layer={layer} onOp={onOp} />}
    </div>
  </section>
);

/**
 * 選んだレイヤーのパラメータを、値の型の JSON Schema から作った入力欄で編集する。
 * 再生位置を渡すと、各パラメータにキーフレームナビゲーター（前へ・足す外す・次へ・補間）を出す。
 */
export const InspectorPanel = (props: InspectorPanelProps) => {
  const { layer, assets = [], hints = DEFAULT_HINTS, kinds, onOp, time, onSeek, selectedPath, onSelectParam } = props;
  const [keyedOnly, setKeyedOnly] = useState(false);
  if (!layer) {
    return <div class="mm-insp mm-insp-empty">No layer selected</div>;
  }
  const at: Playhead | null = time === undefined ? null : { sec: time, fps: props.fps ?? DEFAULT_FPS };
  const span = layerSpan(layer);
  const sections = describeLayer(layer, hints, kinds)
    .map((s) => (keyedOnly ? { ...s, rows: s.rows.filter((r) => hasKeyframes(r.param)) } : s))
    .filter((s) => !keyedOnly || s.rows.length > 0);
  return (
    <div class="mm-insp">
      <h2 class="mm-insp-header">{layer.id}</h2>
      <label class="mm-insp-keyedonly">
        <input type="checkbox" checked={keyedOnly} onChange={(e) => setKeyedOnly(e.currentTarget.checked)} />
        <span>Show Only Keyframed Properties</span>
      </label>
      {sections.map((section) => (
        <SectionView
          key={section.id}
          layer={layer}
          section={section}
          assets={assets}
          at={at}
          span={span}
          showFades={!keyedOnly}
          selectedPath={selectedPath}
          onOp={onOp}
          onSeek={onSeek}
          onSelectParam={onSelectParam}
        />
      ))}
      {!keyedOnly && (
        <section class="mm-insp-section">
          <h3 class="mm-insp-title">
            <span class="mm-insp-titletext">Masks</span>
          </h3>
          <div class="mm-insp-body">
            <AddMaskButton layer={layer} onOp={onOp} />
          </div>
        </section>
      )}
    </div>
  );
};
