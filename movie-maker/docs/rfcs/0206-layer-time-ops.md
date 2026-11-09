# RFC 0206 layer-time-ops: レイヤーを時間の上で動かす `shiftLayer` と、有効を切り替える `setLayerEnabled`

状態: 提案

## 変える理由

Timeline の帯のドラッグでレイヤーを時間の上で動かす操作と、見出しの目のボタンでレイヤーの表示を切り替える操作が `mm-ops` に無い（[18-ui-clarity.md](../design/18-ui-clarity.md) の B1）。画面の部品（T3、Z4）はこの操作を使う。

## 先行事例

| 先行事例 | 採ったもの |
|---|---|
| After Effects でレイヤーの帯をドラッグすると、レイヤーのキーフレームも一緒に動く | `start` とレイヤーのパラメータのキーフレームを同じだけ動かす。素材のイン点とアウト点は変えない |
| After Effects の目のアイコン（Video）と Premiere Pro のトラックの目（Toggle Track Output） | レイヤーごとの有効の切り替え。今の `Layer::enabled`（描かず鳴らさない）をそのまま使う |
| 社内の `lockLayer` | 前の値を置く同じ操作を逆操作にし、`touches` はレイヤーだけ |

## 新しく決めたこと

- `shiftLayer { layer, by }`: `by` はコンポジションの時間軸。`start` と、コンポジションの時刻で評価するキーフレーム（不透明度、変形、切り抜き、マスク、音、効果とノードのパラメータ、字幕の置き場所）を `by` だけ動かす。`mm-render` は `CompCx::t` で評価するので、動かさないとキーフレームが元の時刻に取り残される。
  - 動かさないもの: 素材の時刻の再フレーミング（被写体の位置は素材の時刻で決まる）、レイヤーの始まりからの時刻のタイムリマップ、イン点とアウト点。
  - 動かした先の `start` が負になるときは拒否する（`Layer l-title would start before 0`）。時刻の桁あふれも拒否し、レイヤーは書き換えない。
  - 逆操作は `-by` の `shiftLayer`。`touches` はそのレイヤー。ロックしたレイヤーは既存の経路で拒否する。
  - `attach_to` のあるレイヤーも動かせる（接続はカットのリップルに追従する規則で、別の規則）。
- `setLayerEnabled { layer, enabled }`: データの形は今の `Layer::enabled`（既定は有効）のまま。逆操作は前の値を置く同じ操作、`touches` はそのレイヤー。

## インターフェースの変更

`mm-ops`（`mm-*-api` の操作の口）に操作を 2 つ足す。生成物（`schema/ops.json`、`ui/sdk/src/generated/ops.ts`）が増える。データの形、`sidecar.schema.json` は変わらない。既存の操作は変えない。
