# RFC 0120: ホストの接続の共通実装（mm-host-pipe）

状態: 採用

## 変える理由

- `mm-host-cef` と `mm-host-ofx` が、アプリ側の接続 `PipeHost`（子プロセスの起動、`Ping` の確かめ、1 行ずつの JSON の往復、期限切れの応答の読み捨て、`Shutdown` と kill）を 300 行ずつ同じ形で持っていた。
- 2 つはすでにずれていた。出力が閉じたとき、`mm-host-ofx` は止めて `is_alive` を偽にそろえるが、`mm-host-cef` はそろえない。`mm-host-cef` はホストの木をジョブに入れてアプリがクラッシュしたときも止めるが、`mm-host-ofx` は入れず、アプリがクラッシュするとホストが残る。
- 実装どうしは依存しない（依存の規則 3）ので、片方からもう片方を使えない。

先行事例は RFC 0007 と RFC 0015（実装から共通実装を切り出して名前のあるクレートにし、依存の規則 3 の例外に並べる）。

## 決めたこと

| クレート | 依存 | 内容 |
|---|---|---|
| `mm-host-pipe` | `mm-host-api`、`mm-types`、Windows のジョブオブジェクト | `PipeHost`（`HostProcess` の実装）、`PipeKind`（ホストごとの起こし方）、`StaleHandles`、ジョブ（`Job`、`kill_tree`）、`remove_work_dir`、`SPEC_ENV`、`SERVE_COMMAND` |

- ホストごとの違いは `PipeKind` の定数で渡す。名前（時間切れの `what` と一時フォルダーの名前）、起動の期限、一時フォルダーを渡す環境変数（`None` なら作らない）、読み捨てる応答の共有ハンドルの扱い、子のコマンドへ足す設定（`mm-host-cef` の `PATH`）。
- 読み捨てる応答の共有ハンドルは、`mm-host-cef` は閉じる（このプロセスへ複製されたもの）。`mm-host-ofx` は共有テクスチャを返さない約束なので閉じずに数える（複製されていない値を閉じると、このプロセスの別のハンドルを閉じる）。
- どのホストもジョブに入れる。出力が閉じたら止めて `is_alive` を偽にそろえる。
- `mm-host-cef` と `mm-host-ofx` は `CefLauncher::start`、`OfxLauncher::start` で具体の `PipeHost` を返す。`PipeHost`、`SPEC_ENV`、`SERVE_COMMAND` はこれまでどおり各ホストのクレートからも使える（`mm-host-pipe` の再公開）。
- `mm-host-pipe` は依存の規則 3 の共通実装に並べる。インターフェースと基礎にだけ依存し、wgpu と実装には依存しない。

## 影響

- API クレート（`mm-*-api`）は変えない。
- `mm-host-cef` の公開から `PipeHost::start(spec)` が無くなり、`CefLauncher::start(spec)` になる。`mm-host-ofx` も同じ。
- `mm-host-ofx` の `describe_bundle` も describe の子をジョブに入れ、出力の上限（16 MiB）と、出力が閉じてから終わるまでの期限（2 秒）を持つ。
