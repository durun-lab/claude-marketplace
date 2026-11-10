import { useMemo } from "preact/hooks";
import type { DiagnosticsApi, DiagnosticsInfo, LayerInfo } from "./api.ts";
import { clockText, compositionText, lagText, NO_ADAPTER, rangeText } from "./model.ts";
import { useDiagnostics } from "./useDiagnostics.ts";

export interface DiagnosticsPanelProps {
  api: DiagnosticsApi;
  /** タブが選ばれているか。選ばれている間だけ読む。 */
  active: boolean;
  /** 増えるたびに読み直す（タブを開いたとき、プロジェクトのデータが変わったとき）。 */
  refreshKey?: number;
}

const Row = ({ name, value }: { name: string; value: string }) => (
  <>
    <dt>{name}</dt>
    <dd>{value}</dd>
  </>
);

/** 再生と描画が選んだものと、開いたプロジェクト。GPU を先頭に置く。 */
const Summary = ({ info }: { info: DiagnosticsInfo }) => (
  <dl class="mm-diag-list" aria-label="Summary">
    <Row name="GPU" value={info.adapter || NO_ADAPTER} />
    <Row name="Clock" value={clockText(info.clock)} />
    <Row name="Audio Output" value={info.audio_device ?? "None"} />
    <Row name="Sync Lag" value={lagText(info.lag_ms)} />
    <Row name="Project" value={info.name} />
    <Row name="Composition" value={compositionText(info)} />
  </dl>
);

const Backends = ({ backends }: { backends: [string, string][] }) => (
  <>
    <h3 class="mm-diag-heading">Backends</h3>
    <dl class="mm-diag-list" aria-label="Backends">
      {backends.map(([name, id]) => (
        <Row key={name} name={name} value={id} />
      ))}
    </dl>
  </>
);

/** レイヤーの生の一覧。種類、ID、時間の範囲の列をそろえる。 */
const Layers = ({ layers }: { layers: LayerInfo[] }) => (
  <>
    <h3 class="mm-diag-heading">Layers ({layers.length})</h3>
    <ul class="mm-diag-layers" aria-label="Layers">
      {layers.map((l) => (
        <li key={l.id} class="mm-diag-layer">
          <span class="mm-diag-kind">{l.kind}</span>
          <span class="mm-diag-id">{l.id}</span>
          <span class="mm-diag-range">{rangeText(l)}</span>
        </li>
      ))}
    </ul>
  </>
);

/** 開発の情報。上から、GPU と時計と通知の遅れとプロジェクト、選ばれた実装、レイヤーの生の一覧。 */
export const DiagnosticsPanel = ({ api, active, refreshKey = 0 }: DiagnosticsPanelProps) => {
  const { info, error } = useDiagnostics(api, active, refreshKey);
  // 一覧が変わらない読み直しでは同じ要素を返し、Preact に一覧の比べ直しをさせない。
  const layers = info?.layers;
  const layerList = useMemo(() => layers && <Layers layers={layers} />, [layers]);
  return (
    <section class="mm-diag" aria-label="Diagnostics">
      {error && (
        <p class="mm-diag-error" role="alert">
          {error}
        </p>
      )}
      {info && (
        <>
          <Summary info={info} />
          <Backends backends={info.backends} />
          {layerList}
        </>
      )}
      {!info && !error && <p class="mm-diag-hint">Loading…</p>}
    </section>
  );
};
