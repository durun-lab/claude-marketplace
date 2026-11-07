# movie-maker（アプリ）

Tauri のウィンドウに wgpu のサーフェス（Program と Timeline）と WebView2 を並べ、プロジェクトを開いて再生する。

```sh
# movie-maker/ で。UI（ui/dist/）を先に作る
(cd ui && pnpm install && pnpm build)
cargo run -p movie-maker -- [PROJECT]
```

## Rust の関数の差し替え（開発のときだけ）

アプリを起動したまま、Rust の関数の中身の変更を数秒で当てる。Dioxus の subsecond を使う（[試作](../../docs/design/spikes/subsecond.md)、[反映の速さ](../../docs/design/10-review-improve.md)の (2)）。
cargo の feature `hotpatch` のときだけ入り、既定と本番のビルドには入らない。

### 用意

`dx`（dioxus-cli）を、アプリが使う `dioxus-devtools` と同じ版で入れる。

```sh
cargo install dioxus-cli --version 0.7.10 --locked
dx --version   # dioxus 0.7.10
```

### 起動

```sh
# movie-maker/ で。リンカーは LLVM の lld-link（winget install LLVM.LLVM）
export CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_LINKER="C:/Program Files/LLVM/bin/lld-link.exe"
dx serve --hot-patch --platform desktop -p movie-maker --features hotpatch \
  --interactive false --open false --args "<PROJECT の絶対パス>"
```

- リンカーを差し替えるのは、`.cargo/config.toml` の `rust-lld.exe` を `dx` が `-flavor link` なしで呼び、`lld is a generic driver` で失敗するため。リンカーを変えると依存も作り直しになるので、`cargo build` とは別の `CARGO_TARGET_DIR` を使うとよい。
- 最初の 1 回は全体をビルドする（fat binary）。後は保存するたびに、変えたクレートとそれに依存するクレートだけを作り直してパッチを当てる。
- 保存から反映まで（2026-11-07、Windows 11、中央値）は次のとおり。差し替えの時間の大半は作り直すクレートのコンパイル。

  | 変えた所 | `CARGO_INCREMENTAL=0` | `CARGO_INCREMENTAL=1` |
  |---|---|---|
  | アプリのクレート（IPC のハンドラー） | 10.1 秒 | 5.0 秒 |
  | `mm-ops`（`mm-pack`、`mm-store`、`mm-serve` も作り直す） | 31.4 秒 | 8.8 秒 |
- 差し替わるのは `hotpatch::call`（`src/win/hotpatch.rs`）の中で呼ぶ関数だけ。入口は次の所に置いてある。
  - 操作の適用: `edit::submit_based`（WebView、キー、ドラッグの編集）と、JSON のプロジェクトの再生スレッドの `apply_edits`。ここから呼ぶ `mm-ops`、`mm-store`、`mm-serve` の関数が差し替わる。
  - 共通の IPC のハンドラー: `ipc.rs` の `sync_ack`、`transport`、`project_info`、`key_event`。パネルの IPC のハンドラー（`*_ipc.rs`）は、各パネルの担当が本体を `hotpatch::call` で囲むと差し替わる。
- 構造体の形やトレイトを変える変更は差し替えられない（subsecond の制約）。そのときは dx を止めて起動し直す。
- 差し替わるのは `dx` で起動したワークスペースのメンバーだけ。外のクレートの変更は起動し直す。
- `dx` 0.7.10 は、変えたクレートに依存するワークスペースのメンバーを、アプリが依存しないもの（`apps/mm` など）まで作り直そうとし、`Missing rustc args for replay: 'mm'` で失敗する。`mm-ops` のように `apps/mm` も使うクレートを差し替えるときは、ルートの `Cargo.toml` の `[workspace]` に `exclude = ["apps/mm"]` を一時的に足してから `dx` を起動する（コミットしない）。上流への報告の文案は[試作](../../docs/design/spikes/subsecond.md#上流への報告の文案)にある。

### 入力中と書き出し中

サーフェスでボタンを押している間（ドラッグの途中）と、書き出しのキューが動いている間に届いたパッチは当てずに持つ。
その間はウィンドウの題に「Rust の更新あり」の印を出し、両方が終わったときに当てて印を消す。持つのは最新の 1 つ（dx のパッチは起動したときのバイナリとの差分を全部含む）。

### Windows のリソースの二重

`dx` は自前の Windows のリソース（版の情報とアイコン）をリンクし、`tauri-build` のリソースと二重になって `duplicate resource` でリンクが失敗する。
feature `hotpatch` のとき、`build.rs` が `tauri-build` の書いたリソースを、comctl32 v6 のマニフェストだけの `.res` に置き換える（マニフェストが無いと起動時に `0xc0000139` で落ちる）。
アイコンと版の情報は `dx` の側のものになる。本番のビルド（feature なし）はそのまま。

### 閉じ方

`dx` を強制終了すると WebView2 のユーザーデータのフォルダが使用中のまま残り、次の起動でウィンドウが作れないことがある。アプリのウィンドウを閉じてから `dx` を止める。止まらないときは `taskkill /F /T /PID <dx の PID>` で子まで閉じる。
