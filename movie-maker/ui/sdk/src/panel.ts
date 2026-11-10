import type { ComponentChild } from "preact";
import type { UiCommand } from "./commands.ts";
import type { Asset, Layer, Op } from "./generated/ops.ts";

/** パネルを出す列。列の中のタブは `order` の小さい順に並ぶ。 */
export type PanelColumn = "left" | "right";

/** 開いているコンポジションの要約。アプリ（`ipc.rs` の `CompositionSummary`）が選択のイベントに載せる。 */
export interface CompositionSummary {
  /** コンポジションの ID（コンポジションは名前を別に持たない）。 */
  name: string;
  width: number;
  height: number;
  fps: number;
  /** 秒。 */
  duration: number;
  layer_count: number;
}

/**
 * Timeline のキーフレームの行で選んだパラメータ（アプリの `keyframe_edit.rs` の `ParamSelection`）。Inspector と Graph がそのパラメータを選ぶ。
 * Timeline で選び直すたびに `serial` が進むので、Inspector で別のパラメータを選んだ後に同じ行を押しても選び直せる。
 */
export interface ParamSelection {
  /** パラメータのパス（`setParam` の target.path）。 */
  path: string;
  serial: number;
}

/** Timeline で選ばれているレイヤーと、それが指す素材。 */
export interface LayerSelection {
  layer: Layer | null;
  assets?: Asset[];
  /** 開いているコンポジションの要約。何も選んでいない Inspector が出す。 */
  composition?: CompositionSummary;
  /** Timeline のキーフレームの行で選んだパラメータ。 */
  param?: ParamSelection | null;
}

/** Shell がパネルの `render` に渡すもの。 */
export interface PanelContext<A> {
  api: A;
  /** `mm:doc-changed` が届いた回数。読み直しの合図に使う。 */
  docChanges: number;
  /** タブを選んだ回数。開くたびに読み直したいパネルが使う。 */
  visits: number;
  /** このパネルが持ち主のコマンドが実行された回数。 */
  requests: number;
  /** このパネルのタブが選ばれているか。 */
  active: boolean;
  selection: LayerSelection;
  /** 操作を送る。拒否の理由は Shell が画面に出す。 */
  sendOp: (op: Op) => Promise<void>;
  /** UI のコマンドの表。アプリから読めるまでは null。 */
  commands: readonly UiCommand[] | null;
  /** 表のコマンドを ID で実行する。持ち主がパネルならそのタブを開き、アプリなら `run_command` を呼ぶ。 */
  runCommand: (command: string) => void;
}

/**
 * パネルの登録 1 件。パネルのパッケージが `src/panel.ts` で作り、`ui/shell/src/panels.ts` が 1 行で集める。
 * `createApi` の無いパネルは常に出し、有るパネルは Shell に API が渡されたときだけ出す。
 */
export interface PanelDescriptor<A = undefined> {
  id: string;
  label: string;
  column: PanelColumn;
  order: number;
  createApi?: () => A;
  render: (ctx: PanelContext<A>) => ComponentChild;
}

/** API の型を消した記述子。Shell が種類の違うパネルを 1 つの配列で持つ。 */
export interface AnyPanel {
  id: string;
  label: string;
  column: PanelColumn;
  order: number;
  createApi?: () => unknown;
  render: (ctx: PanelContext<unknown>) => ComponentChild;
}

/** 記述子を作る。`render` の API の型は `createApi` の返り値から決まる。 */
export const definePanel = <A = undefined>(panel: PanelDescriptor<A>): AnyPanel => ({
  ...panel,
  render: (ctx) => panel.render(ctx as PanelContext<A>),
});
