# レビューと自動の改善

## レビューと Claude の対応

- コメントは `project.mmdb` の `comments` の表（[保存の形](09-ingest-storage.md#表)）。時刻か範囲、描き込み、返信、状態（`open` → `fresh` → `done`、`wontfix`）。`mm export` で `versions/review.json` に出る。
- Claude はコメントを読んで操作で対応し、対応 1 回を 1 つのトランザクションにして、そのコメントに結びつける。返事は UI に出る。
- durun-dashboard とは HTTP の API でつなぐ（要望の起票、対応の状態、Claude の対応の起動）。画面は埋め込まず、dashboard からは movie-maker の URL を開く。dashboard の XP 風のスキンの中では、movie-maker は「開いているウィンドウ」の 1 つとしてタスクバーに出る〔推。結合の設計は別に書く〕。

## 自動の改善

durun-dashboard の `server/improve.ts` の型を移す。dashboard とつなぐまでは、movie-maker が自分で持つ（`mm-improve`）。

- 受け付け: `Feedback`。アプリの panic と例外、画面の例外、予算を超えた遅い処理、ホストのクラッシュ回数も自動で起票する（同じものは 1 件にまとめ、未対応の上限と 1 時間あたりの上限。[計装の「継続的改善とのつなぎ」](17-instrumentation.md#継続的改善とのつなぎ)）。要望には詳しくの trace のパスと `mm trace query` の例が添わるので、回はそれを読んで直す。
- 起動: 要望を保存したとき、設定を変えたとき、アプリを起動したときに、動いている回がなく、ON で、未対応の要望があれば `claude -p` を起動して、たまった要望をまとめて実装する。
- 区切り: Stop フックで回を終え、済んだ要望を決める。使用制限で止まったら、明けたときに続きから再開する。
- 囲い: 回は別の worktree で動かし、`cargo check`、`cargo nextest run`（変えたクレートと依存の逆引き）、WebView の型の検査が通ったら本線へ取り込む。
- 計測の比較: 回の前に `mm perf run all` を取り、取り込みの前に同じシナリオを取って `mm perf compare` で比べる。閾値（項目ごと、既定 5%〔推〕）を超えて遅くなった項目があれば取り込まず、返事に項目と数を書く（[計装](17-instrumentation.md#継続的改善とのつなぎ)、[テストの「性能のテストと回帰のゲート」](11-testing.md#性能のテストと回帰のゲート)）。回の直後に起きたエラーと遅い処理は、その回の疑いとして結びつける。回の中ではフィクスチャのプロジェクトに向け、実際のプロジェクトは権限の外に置き、前後で `mm export` の決定的な JSON のハッシュを比べる（`project.mmdb` のバイト列は WAL のチェックポイントで変わるので比べない）。回の予算と、すぐ止める手段。回ごとに 1 コミット。レンダラー、操作の層、スキーマの移行は人の承認が要る領域にして PR までで止める。費用のかかる操作は回から呼べない（回の `claude -p` に回の印 `MM_IMPROVE_RUN` を渡し、`gen.ai` の承認と生成はこれを見て断る。[ノードの「gen.ai の作り」](04-nodes.md#genai-の作り)）。
- 反映の速さ: Rust のコンパイルが待ち時間になる。3 段で縮める。(1) WebView 側（Story、Review、Inspector）は esbuild のバンドルし直しと画面のリロードだけで、Rust を触らない。(2) Rust の関数の中身だけの変更は、Dioxus の subsecond（`dx serve --hotpatch`。ジャンプテーブルで関数を差し替え、構造体の形を変えるとクラッシュする [8]）で、動いているアプリに適用する。どこまで縮むかは段階 0 で測る。(3) 構造体やトレイトを変えたら、ビルドして差し替える。入力中と書き出し中は差し替えず、更新のマークを出す。
- 反映の隔離: 自動の回が書いてよい領域と人の承認が要る領域は、ディレクトリで分ける（[開発のしやすさの「自動の改善の領域」](14-dev-structure.md#自動の改善の領域)）。

### mm-improve の作り

durun-dashboard の `improve.ts` と `errors.ts` から採ったもの:

- 要望は 1 件 1 ファイル（`feedback/<時刻>-<乱数>.json`）。名前順が古い順で、回の `claude -p` はこのファイルを読む。済んだものは `feedback/done/` へ移し、返事（`replies`）を残す。
- 回は「たまった要望の全体を見て計画し、一度に仕上げられる量だけ直す」。最後の発言の「対応した要望:」の後に ID がある要望を済んだものとする。印を書かずに止まったら 2 回まで同じセッションに続きを頼み、それでも書かなければ待機する。
- 失敗したら待機して、同じ失敗を繰り返さない。新しい要望か、ON にし直すと待機を解く。OFF は動いている回を終えてから止まる。
- 使用制限で止まった回は区切らず、明ける時刻（会話ログの `apiErrorParams.rate_limit_info.resetsAt`）の 1 分後に「続けて」で同じセッションを続ける。
- 自動の起票の `ErrorRecord`、鍵、上限（[計装](17-instrumentation.md#継続的改善とのつなぎ)）。要望が済んだら、その記録の回数と最長を数え直す。
- 本線は専用のブランチ `local/movie-maker`（dashboard の `local/dashboard` と同じ型）。人の main のチェックアウトには並行の WIP があることが多く、そこへ自動で早送りすると巻き込むため。無ければ `main` から切る。`mm improve --mainline`（と `--upstream`）で変えられる。PR までで止めるときは、回のコミットを `origin/<upstream>`（既定 `origin/main`）から切り直したブランチに cherry-pick してから PR を作り、取り込み先の先行分を載せない。cherry-pick が衝突したら PR を作らず、要望は未対応に残して返事にその旨を書く。

新しく設計したもの:

- 回は別の worktree（`<data_dir>/worktrees/<回の ID>`、ブランチ `fix/movie-maker-improve-<回の ID>`）で動く。区切ったら変更を本線から 1 コミットにまとめ、本線の先端に載せ直してから検査する。
- Stop フックは、標準入力の JSON を回のフォルダーの `stop.json` に書くだけ（`cat >`）。回を動かしていたプロセスが落ちても、次に起きたときにフックの残した最後の発言で区切れる。
- 判定は 3 つ。検査（cargo check、変えたクレートと依存の逆引きの nextest、`ui/` を変えたら WebView の型、`mm perf compare`）で落ちたら取り込まず、要望は未対応に残して理由を返事に書く（ブランチは残す）。areas.toml の `[improve]` の外か `hold` に触れたか、`mm export` のハッシュが変わったら、PR までで止める。全部通れば本線へ早送りで取り込む。areas.toml は回が書き換えても効かないよう、本線の版で読む。
- 回を 1 つに限るのは OS のファイルの錠（`run.lock`）。アプリと `mm improve` が同じ `data_dir` を指しても、回は 1 つだけ動く。
- 回の予算は `claude -p --max-budget-usd` と時間の上限（既定 5 ドル、60 分。超えたら木ごと止める）。すぐ止める手段は `Improver::stop`（OFF にし、`claude -p` を木ごと止め、worktree を消す。要望は未対応に残る）。
- `claude -p`、検査、PR の作成は差し替えられる層（`Agent`、`Checks`、`Forge`）。試験は決まった差分を書く偽物で通しを確かめる。

置き場所（既定は `%LOCALAPPDATA%/movie-maker/improve`、`MM_IMPROVE_DIR` で変える）:

| もの | 中身 |
|---|---|
| `feedback/*.json`、`feedback/done/*.json` | 未対応の要望 / 済んだ要望（`FeedbackItem`） |
| `settings.json` | ON / OFF、モデル、effort、予算、時間の上限、`mm perf compare` の閾値 |
| `state.json` | 動いている回、待機の理由、最後に取り込んだ回、最後の回の記録 |
| `errors.json` | 自動の起票の記録（`ErrorRecord`）と直近 1 時間の起票の時刻 |
| `runs/<回の ID>/` | `baseline.json`（性能の結果と書き出しのハッシュ）、`stop.json`、`agent.log`、`report.json` |

Feedback パネルとアプリが呼ぶ API（`mm_improve::Improver`。アプリは `Improver::new(...).with_background()` を 1 つ持ち、起動時に `kick` を呼ぶ）:

| 呼び出し | 引数と戻り値 | 使う所 |
|---|---|---|
| `submit(NewFeedback)` | `{ text, targets, context }` → `FeedbackItem` | パネルの送信。保存したら回を起こす |
| `list(ListFilter)` | `all` / `pending` / `done` → `FeedbackItem[]`（新しい順、返事つき） | パネルの一覧 |
| `get(id)` | → `FeedbackItem` | 1 件の詳しく |
| `status()` | → `ImproveStatus { settings, running, run, pending, waiting, last }` | パネルの見出し |
| `update_settings(SettingsPatch)` | `{ enabled?, model?, effort?, budgetUsd?, wallMinutes?, perfThreshold? }` → `ImproveStatus` | ON / OFF と予算 |
| `stop()` | → `ImproveStatus` | すぐ止めるボタン |
| `record(ErrorInput)` | `{ source, message, stack?, where?, span?, ms?, budgetMs?, load?, trace? }` → 起票した ID | panic と例外、画面の例外、予算を超えたスパン |

JSON は camelCase。IPC のコマンドの名前とパネルの API はパネルの担当が決める。durun-dashboard とつなぐ HTTP は、同じ呼び出しを `mm-serve` の外（アプリ）で載せる〔推〕。

CLI は `mm improve status / submit / list / show / set / run / stop / areas`。`run` は回を 1 つ前で回し、`areas` は作業ツリーの変更を areas.toml に突き合わせる（共有のファイル、`--area` の外、自動の改善の取り込みの判定）。


---

出典の番号（[1] など）は [references.md](references.md) を見る。
