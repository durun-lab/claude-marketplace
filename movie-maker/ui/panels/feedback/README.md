# Feedback パネル

改善要望の受け付け（[08-ui.md](../../../docs/design/08-ui.md) のパネルの表、右の列の最後のタブ）。要望は自動の改善（`mm-improve`、[10-review-improve.md](../../../docs/design/10-review-improve.md#自動の改善)）がためて、回が直して返事を付ける。

## 画面

上から 3 つ。

- 設定と状態: 自動の改善の ON / OFF、回の予算（ドル）と時間の上限（分。どちらも 0 より大きい値だけ送り、予算は外せない）、すぐ止めるボタン。いまの状態を 1 行（動いている回、使用制限の明け、待機の理由、未対応の数の順に 1 つ）。
- 要望の入力: 文と、画面で指した部品。「Point at UI」で指すモードに入り、クリックした部品の識別子（短い CSS セレクター、パネルの名前、`aria-label` か中の文字）と位置（WebView の CSS ピクセルと `devicePixelRatio`）を添える。アプリがウィンドウ全体のスクリーンショットを撮り、そのパスも添える（撮った絵には指した部品の枠が入る）。Esc か「Cancel pointing」で抜ける。wgpu のサーフェス（Program と Timeline）は DOM でないので指せず、スクリーンショットにだけ写る。
- 一覧: 全部 / 未対応 / 済みで絞る。新しい順で、状態、本文、指した部品の数、回からの返事（結末とコミット、PR のリンク、落ちた理由）。

一覧の状態は 5 つ。保存された状態（`pending` / `done` / `held`）に、回の中（`ImproveStatus.run.items` に入っている）と差し戻し（未対応で、最後の返事が検査の落ち `rejected`）を足す。

| 状態 | 表示 | 決め方 |
|---|---|---|
| 未対応 | Pending | `pending` で、ほかに当たらない |
| 回の中 | In run | `pending` で、動いている回に渡した |
| 取り込み済み | Landed | `done`（コードを変えずに返事だけしたものも含む） |
| PR で止まった | Held at PR | `held` |
| 差し戻し | Sent back | `pending` で、最後の返事が `rejected` |

## アプリとのつなぎ

- コマンドは `ui/sdk/README.md` の「Feedback のコマンド」。アプリ側は `apps/movie-maker/src/win/feedback.rs`（`Improver` を 1 つ持つ窓口）と `feedback_ipc.rs`。
- 読み直しは、タブを開いたときと、Tauri のイベント `mm:feedback-changed`（起票、設定の変更、回の始めと終わり）。
- 画面の例外（`error` と `unhandledrejection`）は、Shell が API を作るとき（`panel.ts` の `createApi`）に見張りを始めるので、どのタブが開いていても自動の起票になる。1 回の起動で同じ文は 1 回、全部で 20 件まで送る。

## アプリが自動で起票するもの

| 出どころ | 拾い方 |
|---|---|
| アプリの panic | 起動時に入れる panic のフック。場所とバックトレースを添える |
| 画面の例外 | 上の見張り |
| 予算を超えた遅い処理 | 計装の `Layer`（`feedback/watch.rs`）。予算は [17-instrumentation.md](../../../docs/design/17-instrumentation.md#予算の表) のうち 1 回ごとに測れる行（`preview.render_frame` 16.7 ms、`render.gpu_frame` 6 ms、`decode.seek` 150 ms）。起動から 10 秒は数えない |
| ホストのクラッシュ | ホストとの接続（`mm-host-pipe`）が数えるカウンタ `<ホストの種類>.crash`（`host_cef.crash`、`host_ofx.crash`。名前は `mm_trace::names::host_crash_counter`）。応答の途中で出力が閉じた、応答が読めない、要求の合間に終わっていた、起動直後の Ping に答えない、を数え、こちらから止めたホストは数えない |

同じものは `mm-improve` が 1 件にまとめて回数と最長を数える。記録はファイルの読み書きなので、同じスパンかホストは 10 秒に 1 回だけ送る。

## 置き場所

`Improver` の `data_dir` は `mm improve` と同じ（`MM_IMPROVE_DIR`、既定は `%LOCALAPPDATA%/movie-maker/improve`）。回を動かす作業ツリーは、アプリをビルドした movie-maker（`MM_IMPROVE_REPO` で変える）。見つからなければ自動の改善は使えず、パネルは理由を出す。スクリーンショットは `data_dir/attachments/` に置く。

## 試験

- パネル: `src/*.test.ts(x)`。`fixtures/replied.json` は返事の付いた要望で、アプリの試験（`feedback/tests.rs`）が、アプリの返す JSON と形が揃っていることを確かめる。
- 段階 7 の終わりの条件の通し: `apps/movie-maker/src/win/feedback/tests.rs`。アプリと同じ窓口と裏の回し手で、偽の `claude -p` が直して返事が付くまで。実物の `claude -p` での通しは `--run-ignored only real_claude`（予算 1 ドル）。
- スクリーンショット: `pnpm build && PLAYWRIGHT_DIR=… node tools/screenshot.mjs feedback`。
