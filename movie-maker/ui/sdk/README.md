# @mm/sdk

UI から mm-ops の操作を送る SDK。

## 型の生成

`crates/mm-ops/schema/ops.json` から `src/generated/ops.ts` を作る（`node tools/gen-sdk.ts`、`pnpm build` の先頭で実行する）。
生成物は commit する。`ops.json` を作り直したら `pnpm build` を回して差分も commit する。テストが両者のずれを検出する。

- `Op`: 全部の操作の和（`op` が名前）。`OpName`、`OpOf<N>`、`OpArgs<N>`、`OP_NAMES`
- 操作ごとの型 `SetParamOp` など、`$defs` の型（`Layer`、`Param`、`ParamValue` ...）
- `PARAM_VALUE_SCHEMAS`: `ParamValue.type` ごとの `value` の JSON Schema（Inspector が入力欄を決める）

## 送り方

```ts
const client = createClient(createIpcTransport());
await client.send("setParam", { target: { layer: "l1", path: "opacity" }, value: { type: "number", value: 0.5 } });
```

`Transport`（`send(op) => Promise<OpResult>`）が差し替えられるトランスポート。今は IPC だけ。HTTP は同じ形の `Transport` を足す。

## アプリ側の IPC のハンドラー

プレビューのウィンドウの担当が作る。

| 項目 | 値 |
|---|---|
| コマンドの名前 | `apply_op` |
| 引数 | `{ op: Op }`。`Op` は `ops.json` の 1 つの操作を serde で出した JSON そのまま（例 `{ "op": "setParam", "target": { "layer": "l1", "path": "opacity" }, "value": { "type": "number", "value": 0.5 } }`） |
| 成功 | 操作を適用した後の値。形は問わない（sdk は `unknown` で返す） |
| 失敗 | `Err(String)`。sdk は `{ ok: false, error }` にする |

Rust の側は `#[tauri::command] fn apply_op(op: mm_ops::Op, ...) -> Result<_, String>` の形。

## UI のコマンドの表

画面の操作の入口（Tasks の項目、キー）が ID で指すコマンドの表。表はアプリの `apps/movie-maker/src/win/commands.rs`、キーの割り当ては `keys.rs` が持つ。型と関数は `src/commands.ts`（`createIpcCommandsApi`、`commandTitle`）。表は人の操作の入口だけを持ち、データの変更は実行した先の編集が操作（`Op`）で出す。

| コマンド | 引数 | 成功 |
|---|---|---|
| `command_table` | なし | `UiCommand[]`（ID の名前の順） |
| `run_command` | `{ id }` | すぐ `()`（編集は編集のキューに積む）。編集の失敗は Source の状態の `message`（`mm:source`）に出る。アプリが持ち主の、今押せるコマンドだけを受け、知らない ID、パネルが持ち主のコマンド、今押せないコマンドは `Err`。キー（`key_event`）は押せるかを見ずに実行し、拒否した理由は編集の `message` に出る |

| `UiCommand` の項目 | 中身 |
|---|---|
| `id` | `<組>.<動詞>`（`clip.split`） |
| `name` | 画面の名前（英語） |
| `keys` | キーの表の名前（`Ctrl+KeyZ`）。先頭が画面に出すキー |
| `key_label` | 先頭のキーの見た目（`Ctrl+Z`）。キーが無ければ `null` |
| `description` | ツールチップの 2 行目。無ければ `null` |
| `menu` | メニューバーでの場所（`file`、`edit`、`playback`）。無ければ `null` |
| `panel` | 持ち主の WebView のパネルの id。アプリが実行するコマンドは `null` |
| `enabled` | 今押せるか |
| `checked` | 入/切のあるコマンドのチェックの状態。それ以外は `null` |

押せるかが変わると、Tauri のイベント `mm:commands-changed`（`COMMANDS_CHANGED_EVENT`）が同じ形の表を丸ごと届ける。Shell は表を `PanelContext.commands` でパネルに渡し、`runCommand` は持ち主がパネルならそのタブを開き、アプリなら `run_command` を呼ぶ。ボタンの `title` は `commandTitle` で作る（1 行目が「名前 (キー)」、2 行目が説明。表に無いコマンドは `Not available yet`）。

## Media のコマンド

Media パネル（`ui/panels/media`）が使う Tauri のコマンド。ハンドラーは `apps/movie-maker/src/media_ipc.rs`、型は `ui/panels/media/src/api.ts`（`createIpcMediaApi` が呼ぶ）。失敗は `Err(String)`。

| コマンド | 引数 | 成功 |
|---|---|---|
| `media_list` | なし | `MediaItem[]`（`id`、`name`、`path`、`content_hash`、`kind`、`duration`（秒）、`fps`、`width`、`height`、`has_thumbnail`、`has_waveform`、`status`: `ready` / `partial` / `missing`） |
| `media_thumbnail` | `{ id }` | JPEG の生のバイト列（`ArrayBuffer`） |
| `media_waveform` | `{ id, buckets }` | `{ duration, peaks }`。`peaks` は `buckets` 個以下で 0 から 1（チャンネルの最大） |
| `media_pick_files` | なし | 選んだパスの配列。キャンセルは空 |
| `media_import` | `{ paths }` | 全部終わってから `()`。取り込み中にもう一度呼ぶと `Err("Import in progress")` |
| `media_cache_info` | なし | `{ total_bytes, folders: [{ name, bytes, files, clearable }] }` |
| `media_cache_clear` | `{ kinds }` | 空けたバイト数。`preview`、`audio`、`thumbs` だけ受け、ほかの名前は何も消さず `Err` |

`media_import` は `mm ingest` と同じ `mm_ingest::ingest` で素材ごとに `cache/` へ出力を作り、まだ無い素材だけ `addAsset` を再生スレッドに適用させる。進み具合は Tauri のイベント `mm:ingest-progress`（`INGEST_PROGRESS_EVENT`）で送る。中身は `type` で分かれる。

| `type` | 中身 |
|---|---|
| `started` | `index`、`total`、`path` |
| `done` | `index`、`path`、`asset_id`、`added`（偽なら同じコンテンツハッシュの素材がすでにあった） |
| `failed` | `index`、`path`、`error`（失敗した素材はスキップして次へ進む） |
| `finished` | `added`、`skipped`、`failed` |

ファイルのドロップは Tauri の `tauri://drag-enter`、`tauri://drag-leave`、`tauri://drag-drop`（`{ paths }`）を `MediaApi.onDrop` で受ける。

## Story のコマンド

Story パネル（`ui/panels/story`）が使う Tauri のコマンド。ハンドラーは `apps/movie-maker/src/story_ipc.rs`、型は `ui/panels/story/src/api.ts`（`createIpcStoryApi` が呼ぶ）。時刻の `RationalTime` は `{ num, den }`（素材の時間軸）。失敗は `Err(String)`。

| コマンド | 引数 | 成功 |
|---|---|---|
| `story_overview` | なし | `{ assets: [{ id, name, duration, has_transcript }], sequences: [{ id, cuts, duration }], open_sequence }`（`open_sequence` は開いているコンポジションの sequence。無ければ `null`） |
| `story_transcript` | `{ asset }` | `{ asset, words, topics, selections }`。`words` は時刻順の `{ id, segment, start, end, text, fixed, deleted }`（`text` は取り込んだままの文字、`fixed` は直した文字、表示は `fixed ?? text`）。`topics` と `selections` はその素材のもの（`ops.ts` の `Topic`、`Selection`） |
| `story_range` | `{ asset, first, last }` | `{ start, end }`。単語 `first` から `last`（先後はどちらでもよい）までの時刻。`addTopic` の `in`、`out` にそのまま使う |
| `story_search` | `{ query, limit? }` | `[{ asset, word, start, end, context }]`。`mm search` と同じ規則（単語の境をまたぐ、消した単語は外す、直した文字で探す）。語は 1 から 32 文字。`limit` の既定は 50 |
| `story_timeline_time` | `{ asset, at }` | 素材の時刻 `at`（秒）に当たる再生位置（秒）。開いているコンポジションの sequence のカットに入っていなければ `null` |
| `story_cut_time` | `{ cut }` | カット `cut` の始まりの再生位置（秒）。開いているコンポジションの sequence のレイヤーに出ていなければ `null` |

文字起こしの本文は、`project.mmdb` の `Store::transcript_words` があればそれを、無ければ取り込みの付属ファイル（`cache/ingest/<hash>.transcript.json`）を読む。消した単語と直した文字は、アプリが開いているプロジェクトのデータの `transcript_edits` を重ねる。

Story と Media は素材を選んだとき、DOM のイベント `mm:asset-selected`（`detail.asset`）を投げる（`announceAsset`）。`web/transport.js` が受けて Source で開く。

編集は `apply_op` で送る。パネルが使う操作は `addTopic`、`removeTopic`、`cutWords`（Delete）、`deleteWords`（Hide と Restore）、`fixTranscript`、`rate`、`tag`、`insertTopics`。単語をクリックしたときの再生位置の移動は `transport`（`{ action: "seek", value: 秒 }`）で、`story_timeline_time` が返した位置に動かす。

## コメントの型

`src/comments.ts`（`@mm/sdk` から型だけ export）。`Comment`、`NewComment`、`CommentQuery`、`CommentEvent`。HTTP の `/comments`（デスクトップの待ち受けと、人の待ち受けの `/m/api/comments`）の本体と応答の形で、仕様は `docs/design/12-mobile.md`。

## Deliver のコマンド

Deliver パネル（`ui/panels/deliver`）が使う Tauri のコマンド。ハンドラーは `apps/movie-maker/src/deliver_ipc.rs`、型は `ui/panels/deliver/src/api.ts`（`createIpcDeliverApi` が呼ぶ）。時間は秒。失敗は `Err(String)`。

| コマンド | 引数 | 成功 |
|---|---|---|
| `export_overview` | なし | `{ targets, compositions: [{ id, duration }], sequences: [{ id, duration }], open_composition, can_verify }`。`targets` はプロジェクトのデータの `render_targets`（`ops.ts` の `RenderTarget`）で、相対の出力先は開くときに直す前の相対のまま返す（画面が編集で書き戻しても絶対パスにならない）。`can_verify` は FFmpeg なしのビルドと、ffprobe を起動できないとき（`MM_FFPROBE`、PATH）に偽 |
| `export_status` | なし | キューのいまの状態 `ExportStatus`。パネルを開き直したときに読む |
| `export_start` | `{ request: { targets, sequence?, verify } }` | すぐ `()`。`targets` はレンダーターゲットの ID でこの順に書き出す。`sequence` を指すと各レンダーターゲットのコンポジションの代わりにその sequence を全長で書き出す。書き出し中は `Err("Export in progress")`。同じ出力先（大文字小文字とパスの区切り文字の違い、一時ファイルの名前との重なりを含む）を指すレンダーターゲットの組、無い ID、空のスライスは始める前に `Err`。字幕を出すレンダーターゲットには `project.mmdb` の文字起こしを渡す |
| `export_cancel` | なし | `()`。書き出し中の 1 本は次のフレームで止まり（一時ファイルを消す）、残りは書き出さない |
| `export_reveal` | `{ path }` | エクスプローラーで選んで開く。この書き出しで作ったファイル以外は `Err` |
| `export_pick_output` | `{ current?, codec? }` | 出力先の保存ダイアログで選んだパス。`codec` に合う拡張子で絞る（ProRes と DNxHR は `.mov`）。キャンセルは `null` |
| `apply_ops` | `{ ops: Op[] }` | `apply_op` の複数形。全部を 1 つのトランザクションで適用する（取り消し 1 回で戻る）。レンダーターゲットを直すときの `removeRenderTarget` と、元の位置を指す `addRenderTarget`（`index`）の組に使う |

書き出しは `mm_export::export_run` を、再生とは別のスレッドと別の `Engine` で順に回す。1 本が失敗しても残りは書き出す。進み具合は Tauri のイベント `mm:export-progress`（`EXPORT_PROGRESS_EVENT`）で、**キュー全体の状態**を送る（最大 10 回/秒と、状態が変わるたび）。受けた側は差分を畳まず、そのまま描く。

| 項目 | 中身 |
|---|---|
| `running` | キューがまだ動いているか |
| `items[]` | キューの 1 本。`target`、`state`（`queued`、`running`、`done`、`failed`、`cancelled`）、`frames_done`、`frames_total`、`fps`、`elapsed`、`eta`（測れるまで `null`）、`output_path`、`bytes_written`、`verification`（`verify` で書き出したときの ffprobe の結果。`video_codec`、`width`、`height`、`video_frames`、`video_seconds`、`audio_seconds`）、`error` |

検証に失敗した書き出しは、出力を置き換えずに `failed` と理由で終わる。

`sequence` の上書きは依頼の全体に 1 つで、レンダーターゲットはコンポジションを指す形のまま（本編とショートはそれぞれのコンポジションを持つ）。縦長のプリセットは、縦長のコンポジションを既定にする。

レンダーターゲットの欄は `RENDER_TARGET_SCHEMA`（`ops.json` の `$defs.RenderTarget` から作る生成物）から Deliver が入力欄を作る。`RenderTarget` に欄が増えたら、`ops.json` を作り直して `pnpm build` を回すだけで画面に出る。

## Feedback のコマンド

Feedback パネル（`ui/panels/feedback`）が使う Tauri のコマンド。ハンドラーは `apps/movie-maker/src/win/feedback_ipc.rs`、型は `ui/panels/feedback/src/api.ts`（`createIpcFeedbackApi` が呼ぶ）。中身は `mm_improve::Improver` の公開 API で、JSON は camelCase。失敗は `Err(String)`（作業ツリーが見つからず自動の改善を使えないときも）。

| コマンド | 引数 | 成功 |
|---|---|---|
| `feedback_status` | なし | `ImproveStatus`（`settings`、`running`、`run`、`pending`、`waiting`、`last`） |
| `feedback_list` | `{ filter }`（`all` / `pending` / `done`。省くと `all`） | `FeedbackItem[]`（新しい順、返事つき） |
| `feedback_get` | `{ id }` | `FeedbackItem` |
| `feedback_submit` | `{ feedback: { text, targets, context } }` | `FeedbackItem`。アプリが `context.app`（開いているプロジェクト）を添え、回を起こす |
| `feedback_settings` | `{ patch: { enabled?, model?, effort?, budgetUsd?, wallMinutes?, perfThreshold? } }` | `ImproveStatus` |
| `feedback_stop` | なし | `ImproveStatus`。OFF にし、動いている回を木ごと止める。要望は未対応に残る |
| `feedback_capture` | なし | ウィンドウのスクリーンショット（PNG）のパス。指した部品の `screenshot` に入れる |
| `feedback_report_error` | `{ error: { message, stack?, where? } }` | `null`。画面の例外を自動の起票に回す（出どころはアプリが `webview` に決める） |

Tauri のイベント `mm:feedback-changed`（中身なし）が、起票、設定の変更、回の始めと終わりに届く。

## Mixer のコマンド

Mixer パネル（`ui/panels/mixer`）が使う Tauri のコマンドとイベント。型は `ui/panels/mixer/src/api.ts`（`createIpcMixerApi` が呼ぶ）。失敗は `Err(String)`。

| コマンド | 引数 | 成功 |
|---|---|---|
| `mixer_state` | なし | いま開いているコンポジションの `MixerState`。`composition`、`mixer`（`ops.ts` の `CompMixer`、既定は `null`）、`tracks[]`（`layer`、`kind`、`label`、`audio`（`LayerAudio`、既定は `null`）、`animated`）、`roles`（バスの選択肢） |
| `apply_ops` | `{ ops: Op[] }` | `()`。Mixer が送る操作は 2 つで、どちらも変えた後の全体を置く。`setLayerAudio`（レイヤーの `LayerAudio` 全体）と `setMixer`（コンポジションの `CompMixer` 全体）。1 回の操作が取り消し 1 回で、フェーダーはドラッグを離したときに 1 回だけ送る |

音量とパンにキーフレームのあるトラック（`animated`）は、フェーダーで固定の値に置くとキーフレームを消すので、Mixer は触らせない（ミュート、ソロ、バスは触れる）。

メーターは Tauri のイベント `mm:mixer-meters`（`MIXER_METERS_EVENT`）で、再生位置の通知と同じ間隔で `MixerMeters`（`t`、`playing`、`tracks[]`（`layer`、`peak_db`）、`master`（`peak_db` の L と R、`true_peak_db`、`short_term_lufs`、`integrated_lufs`））を受ける。無音と測れない値は `null`。受けた値は state に入れず、パネルが rAF で棒の DOM を直接書き換える（ピークホールドは UI 側で 1.5 秒）。`playing: false` を受けたら棒は少しずつ下がる。

## Review のコマンド

Review パネル（`ui/panels/review`）が使う Tauri のコマンド。型は `ui/panels/review/src/api.ts`（`createIpcReviewApi` が呼ぶ）。失敗は `Err(String)`。提案（`Proposal`）は 1 つのトランザクション（操作の並び）で、保留中はプロジェクトのデータに入っていない。

| コマンド | 引数 | 成功 |
|---|---|---|
| `review_list` | なし | `Proposal[]`（新しい順） |
| `review_approve` | `{ id, ops }`（`ops` は承認する操作の index。`null` は全部） | `{ proposal, change }`。承認した操作が普通のトランザクションとして適用され、取り消せる |
| `review_reject` | `{ id }` | `Proposal`。データは変わらない |
| `review_reopen` | `{ id }` | `Proposal`。却下したものを保留に戻す |
| `review_undo` | `{ id }` | `Proposal`。承認で適用したトランザクションが取り消しのスタックの一番上のときだけ取り消す |
| `review_preview` | `{ id, ops }` | `null`。Program に提案を適用したフレームを出す。`id: null` で元のフレームに戻す |
| `review_comment_focus` | `{ id }`（`null` で外す） | `null`。選んだコメントの描き込みを、再生位置がその時刻か範囲にある間だけ Program に重ねる |
| `review_comments` | なし | `Comment[]`（書いた順。モバイルが書いたものも入る。型は `ui/sdk/src/comments.ts`） |
| `review_comment_reply` | `{ id, text }` | `Comment`。人の操作経路（`gui`）の返信 |
| `review_comment_state` | `{ id, value }`（`open`、`done`、`wontfix`） | `Comment`。`fresh` は決められず `Err` |

`Proposal` の `applied` は適用したトランザクションが今も取り消されていないこと、`undoable` はそれが取り消しのスタックの一番上にあること、`conflict` は保留中で提案の後の編集と触る所が重なる ID（あれば承認できない）。

Tauri のイベント `mm:proposals-changed`（`{ id, state }`）が提案の追加・決定のたびに届く。取り消しで `applied` と `undoable` が変わるので、`mm:doc-changed` でも一覧を読み直す。コメントの追加・返信・状態の変化（モバイルや Claude が書いたものも）は `mm:comments-changed`（`{ id, state }`）で届く。コメントの時刻を押したときの再生位置の移動は `transport`（`{ action: "seek", value: 秒 }`）。

## Transition のコマンド

Transition パネル（`ui/panels/transition`）が使う Tauri のコマンド。ハンドラーは `apps/movie-maker/src/win/transition_ipc.rs` と `pack_ipc.rs`、型は `ui/panels/transition/src/api.ts`（`createIpcTransitionApi` が呼ぶ）。`dir` はパックのフォルダー（既定 `packs/vlog`）。失敗は `Err(String)`。

| コマンド | 引数 | 成功 |
|---|---|---|
| `transition_cuts` | `{ dir }` | 開いているコンポジションの sequence の `{ sequence, cuts: [{ id, start, duration, transition }] }`（秒）。`transition` は `{ kind, duration, params }` か `null`。`kind` は `dir` のパックのコードと一致した名前で、一致しなければ `null`。`params` はノードの数のパラメータ（`p0`..`p7`、`rewind`） |
| `pack_transitions` | `{ dir }` | パックの `transitions/` の名前（名前の順） |
| `pack_apply` | `{ dir, name: "transition", args: { cut, kind, dur, seed? } }`、巻き戻しは `{ dir, name: "rewind", args: { cut, dur, speed? } }` | 置く・差し替える。`setTransition` 1 回で、取り消し 1 回で戻る。パネルは今の `p0`（`seed`）と `rewind`（`speed`）を引き継ぐ |
| `apply_op` | `{ op: { op: "setTransition", cut, transition: null } }` | 外す |

## Inspector のコマンド

Inspector パネル（`ui/panels/inspector`）が使う Tauri のコマンド。ハンドラーは `apps/movie-maker/src/win/inspector_ipc.rs`、呼ぶのは `ui/panels/inspector/src/kindsApi.ts`（`createIpcKindLookup`）。MCP の `kinds` と同じ `Core::kinds` を通る。失敗は `Err(String)`。登録の無い種類（`composition` など）と、書き手として開いていないプロジェクト（JSON）には失敗を返し、Inspector はその種類をプロジェクトにある値だけで出す。Inspector が引くのは種類か値が変わったソースとエフェクトだけで、定義のある種類の値の変更は 250 ms 止まってから引く（`LOOKUP_DEBOUNCE_MS`）。

| コマンド | 引数 | 成功 |
|---|---|---|
| `inspector_kind` | `{ kind, params }` | 種類の一覧の 1 件 `{ id, version, traits, inputs, params: { params: [{ name, ty, default }] } }`。`params` はレイヤーの今の値（名前から `ParamValue`）で、パラメータの組ごとの定義（`params_for`）で一覧が変わる種類（`ofx` のプラグインのパラメータなど）に渡す。`ty` は `{ type: "number", min, max }`、`{ type: "choice", options }`、`{ type: "bool" }` など |
