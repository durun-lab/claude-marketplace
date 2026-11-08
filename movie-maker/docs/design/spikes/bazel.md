# 試作: Bazel（rules_rust）でビルドする（途中）

worktree の場所が違っても、自前のクレートを含めたビルドとテストの結果を共有できるかを確かめる試作。cargo のビルドが正で、この試作は cargo のファイルを変えない。
**途中で止めた（WIP）。** 測れたのは「何もない状態から」だけ。残りは末尾の「残りの作業」。

## ここまでの結論

- **コンソールのウィンドウは出なかった。** Bash のツール（前景と `run_in_background` の両方）から bazelisk を起動し、Bazel のサーバーの起動、crate_universe の取得、263 個のローカルのアクション（rustc、build script、cl.exe）、7 個のテストの実行まで、表に出たコンソールの窓は 0 個だった。
- Bazel のサーバーは `DETACHED_PROCESS` で起動され、アクションは `CREATE_NO_WINDOW` で作られる（[process.cc](https://github.com/bazelbuild/bazel/blob/master/src/main/native/windows/process.cc)、[blaze_util_windows.cc](https://github.com/bazelbuild/bazel/blob/master/src/main/cpp/blaze_util_windows.cc)）。孫のプロセス（rustc → link.exe、build script → cl.exe）は窓の無いコンソールを継承する。前の試作で窓が出た原因は再現できなかった（起動のしかたの違いと見ている）。
- **何もない状態からは cargo より遅い（47 秒対 19 秒）。** 差のほとんど（約 26 秒）は rules_rust が自分の道具（process_wrapper、build script の runner、`cargo_toml_variable_extractor` とその依存の url / icu）をソースからビルドする時間。道具は disk cache に入るので、2 回目以降の worktree では払わない（未測定）。

## 窓の確かめ方

`spikes/bazel/watch-console.ps1` が `SetWinEventHook` で `EVENT_OBJECT_SHOW` と `EVENT_SYSTEM_FOREGROUND` を拾い、クラスが `ConsoleWindowClass`、`CASCADIA_HOSTING_WINDOW_CLASS`、`PseudoConsoleWindow` の窓を数える。10 秒に 6 個以上出たら Bazel の java を止める。最小化した `cmd` を 1 個出して、拾えることを先に確かめた。

```sh
pwsh -NoProfile -File spikes/bazel/watch-console.ps1 -Log w.log -StopFile stop -MaxSeconds 3600
```

## 数字（mm-types、mm-doc、mm-ops、mm-store とそのテスト。--jobs 16 / -j 16）

| 場面 | Bazel | cargo |
|---|---|---|
| 何もない状態からビルド（テストのバイナリまで） | 47.4 秒（disk cache なし、`bazel clean` の後。外部のリポジトリは取得済み） | 18.6 秒（新しい target） |
| テストの実行（7 つのテストの対象） | 9.3 秒（`--nocache_test_results`） | 16.6 秒 |
| 別の worktree から同じコミット | 28.9 秒（前の試作の disk cache を使い、自前の 10 個だけローカル。253 個がヒット） | 未測定 |
| 1 ファイル変えたとき | 未測定 | 未測定 |
| テストの結果のキャッシュ | 変えていない対象は `(cached) PASSED` | なし |

Bazel の何もない状態の 47 秒の内訳（クリティカルパス）: rules_rust の道具 約 26 秒 → proc-macro2 / syn / serde_derive_internals / schemars_derive（exec の設定で opt-level 2）約 12 秒 → mm-types から mm-store 約 7 秒。

## 先行事例

- rules_rust は Windows を公式にはサポートしない（「ほとんどは動くはずだが CI のテストを多数無効にしている」）。<https://bazelbuild.github.io/rules_rust/>、[#838](https://github.com/bazelbuild/rules_rust/issues/838)
- crate_universe の `from_cargo` は splicing で symlink を作るので、開発者モードが切だと失敗する（[#2019](https://github.com/bazelbuild/rules_rust/issues/2019)、open）。この試作は `from_specs` で直接の依存を並べ、版は Cargo.lock の写し（`Cargo.bazel.lock`）で固定した。`--enable_runfiles` なしの完全な対応も未完（[#1156](https://github.com/bazelbuild/rules_rust/issues/1156)）。
- MAX_PATH（[#1120](https://github.com/bazelbuild/rules_rust/issues/1120)、[#4391](https://github.com/bazelbuild/rules_rust/issues/4391)）は `--output_user_root=E:/dev/bazel/o` で短くして避けた。依存が多いと rustc の `-L` で PATH が 32767 文字を超える（[#3767](https://github.com/bazelbuild/rules_rust/issues/3767)）。全部を移すと当たりうる。
- bindgen は Windows で失敗の報告がある（[#2009](https://github.com/bazelbuild/rules_rust/issues/2009)）。ffmpeg-sys 系は未試行。
- disk cache は同じマシンの複数の checkout で共有できる（<https://bazel.build/remote/caching>、<https://fuchsia.dev/fuchsia-src/development/build/bazel_disk_cache>）。ただし build script が絶対パス入りの出力を書くと、別の場所の clone でキーが変わる（[#4201](https://github.com/bazelbuild/rules_rust/issues/4201)）。この試作の範囲では前の試作（別の worktree）の disk cache が 253 個ヒットした。
- 大きな Rust の workspace を移した事例: [Scaling Rust builds with Bazel](https://mmapped.blog/posts/17-scaling-rust-builds-with-bazel)（BUILD を Cargo.toml から約 9 割自動生成、rust-analyzer のために Cargo のファイルを残す）。数字つきの比較は見つからない。

## 詰まった所

- github.com の releases のダウンロードが 504 / タイムアウトになる。rules_cc などを `gh api` で落として repository cache に入れた（`BAZEL_HTTP_RULES_URLS_AS_DEFAULT_CANONICAL_ID=0` を `user.bazelrc` に）。依存の版を repository cache にあるもの（rules_cc 0.2.28、platforms 1.1.0）に揃えた。
- ブランチ `spike/movie-maker-bazel` が前の試作の worktree で使用中のため、手元のブランチは `spike/movie-maker-bazel-v2` にし、リモートの `spike/movie-maker-bazel` へ push した。

## 残りの作業

1. 別の worktree から同じコミット（disk cache を共有、新しい output base）と、cargo の枠（25 秒相当）の比較。
2. 1 ファイル変えたとき（mm-types の葉の変更と mm-store の変更）。
3. libwebp-sys（`@crates//:webp`）のビルド。libsqlite3-sys（bundled）は通った。
4. exec の設定の依存（proc-macro）を opt-level 0 にする（cargo の build-override と同じ）と、何もない状態がどれだけ縮むか。
5. 全部を移すときの見積もりと推奨。
