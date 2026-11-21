# 試作: Bazel（rules_rust）でビルドする

worktree の場所が違っても、自前のクレートを含めたビルドとテストの結果を共有できるかを確かめる試作。cargo のビルドが正で、この試作は cargo のファイルを変えない。
範囲は mm-types、mm-doc、mm-ops、mm-store とそのテスト、libwebp-sys の確かめ（`bazel/webp-probe`）。

## 推奨: 今は移らない

- **得られるもの**: 別の worktree で自前のクレートもテストの結果も当たる（1〜2 秒、テストは `(cached) PASSED`）。編集してテストまでの往復は、変わったテストだけ走るので cargo より 3〜7 秒速い。
- **払うもの**: 何もない状態のビルドは cargo より 7 秒遅い。cargo の借りた枠がすでに worktree の間の作り直しを 5 秒に抑えている。速くなるのは主にテストの結果のキャッシュで、時間のかかるテスト（GPU、FFmpeg、CEF）ほど移すのが難しい（下の「移すときの見積もり」）。
- Cargo.toml と BUILD の二重の管理が続く（rust-analyzer、clippy、fmt は cargo のまま）。rules_rust は Windows を公式にはサポートしない。
- 見直す条件: 開発者モードを入れて `from_cargo` が使える（BUILD の依存を Cargo.toml から自動で出せる）、かつ、テストの実行が並行の開発の待ち時間の大半になったとき。そのときは純 Rust のクレートだけを Bazel でテストする「部分的に使う」から始める。

## 数字（--jobs 16 / -j 16。cargo は `cargo test --no-run` または `cargo test` に `-p` で同じ 4 つ）

| 場面 | Bazel | cargo |
|---|---|---|
| 何もない状態からビルド（テストのバイナリまで） | 42.8 秒（既定）→ **22.3 秒**（exec の設定を opt-level 0。下の「新規設計」） | 15.5 秒（新しい target） |
| 別の worktree から同じコミット（ビルド） | 1.3〜2.3 秒（263 個すべて disk cache に当たる。自前のクレートも当たる） | 5.2 秒（借りた枠。自前の 3 つを作り直す） |
| 別の worktree から同じコミット（テスト） | 0.4 秒（7 つとも `(cached) PASSED`） | 16.6 秒（毎回走る） |
| 新しい worktree の最初の取得（`bazel fetch`） | 11〜13 秒（1 回だけ。crate_universe の解決） | なし |
| 1 ファイル変えてビルド: mm-types（葉） | 6.1〜7.3 秒（11 個のアクション） | 5.5 秒（4 つ） |
| 1 ファイル変えてビルド: mm-store | 2.0〜2.3 秒 | 2.3〜3.2 秒 |
| 1 ファイル変えてテストまで: mm-types の関数の本体 | **14.3 秒**（6/7 を実行） | 21.2 秒（全部） |
| 1 ファイル変えてテストまで: mm-store | **10.5 秒** | 17.5 秒 |
| 1 ファイル変えてテストまで: mm-types に使われない `pub const` | **6.1 秒**（バイナリが同じなので 0/7） | 21.0 秒 |
| テストの実行（7 つのテストの対象、キャッシュなし） | 9.3 秒 | 16.6 秒 |
| libwebp-sys（build script と cc） | 10.1 秒で通り、リンクして符号化と復号が通る | 通る |
| 1 つの worktree の常駐 | Bazel のサーバー（java）0.6〜0.9 GB、output base 1〜1.4 GB | なし |

- 何もない状態の内訳（クリティカルパス、既定）: rules_rust の道具（exec の設定で opt-level 3）約 26 秒 → proc-macro2 / syn / serde_derive（opt-level 2）約 12 秒 → mm-types から mm-store 約 7 秒。
- proc-macro だけを opt-level 0 にしても 43.0 秒で縮まない。道具と build script を先に待つため。道具と build script を 0 にすると 28.4 秒、両方で 22.3 秒。
- cargo の今の設定では proc-macro も opt-level 2 で作っている。`profile.dev.package."*"` は `build-override` より優先される（[Cargo の Profiles](https://doc.rust-lang.org/cargo/reference/profiles.html#overrides)）。
- コンソールのウィンドウ: 今回の全部の実行（2 つの output base、約 1,500 個のローカルのアクション、テスト）で 0 個。

## 窓の確かめ方

`spikes/bazel/watch-console.ps1` が `SetWinEventHook` で `EVENT_OBJECT_SHOW` と `EVENT_SYSTEM_FOREGROUND` を拾い、クラスが `ConsoleWindowClass`、`CASCADIA_HOSTING_WINDOW_CLASS`、`PseudoConsoleWindow` の窓を数える。10 秒に 6 個以上出たら Bazel の java を止める。Bazel のサーバーは `DETACHED_PROCESS` で起動され、アクションは `CREATE_NO_WINDOW` で作られる（[process.cc](https://github.com/bazelbuild/bazel/blob/master/src/main/native/windows/process.cc)、[blaze_util_windows.cc](https://github.com/bazelbuild/bazel/blob/master/src/main/cpp/blaze_util_windows.cc)）。

```sh
pwsh -NoProfile -File spikes/bazel/watch-console.ps1 -Log w.log -StopFile stop -MaxSeconds 3600
```

## 先行事例

- rules_rust は Windows を公式にはサポートしない（「ほとんどは動くはずだが CI のテストを多数無効にしている」）。<https://bazelbuild.github.io/rules_rust/>、[#838](https://github.com/bazelbuild/rules_rust/issues/838)
- crate_universe の `from_cargo` は splicing で symlink を作るので、開発者モードが切だと失敗する（[#2019](https://github.com/bazelbuild/rules_rust/issues/2019)、open）。`--enable_runfiles` なしの完全な対応も未完（[#1156](https://github.com/bazelbuild/rules_rust/issues/1156)）。
- MAX_PATH（[#1120](https://github.com/bazelbuild/rules_rust/issues/1120)、[#4391](https://github.com/bazelbuild/rules_rust/issues/4391)）は短い `--output_user_root` で避ける。依存が多いと rustc の `-L` で PATH が 32767 文字を超える（[#3767](https://github.com/bazelbuild/rules_rust/issues/3767)）。
- bindgen: rules_rust_bindgen の既定の toolchain は libclang を llvm-project のソースからビルドする。用意済みの libclang を使う例は `examples/bindgen_toolchain`（[rust_bindgen](https://bazelbuild.github.io/rules_rust/rust_bindgen.html)）。Windows での失敗の報告がある（[#2009](https://github.com/bazelbuild/rules_rust/issues/2009)）。
- disk cache は同じマシンの複数の checkout で共有できる（<https://bazel.build/remote/caching>、<https://fuchsia.dev/fuchsia-src/development/build/bazel_disk_cache>）。build script が絶対パス入りの出力を書くと、別の場所の clone でキーが変わる（[#4201](https://github.com/bazelbuild/rules_rust/issues/4201)）。この範囲では当たらなかった。
- 大きな Rust の workspace を移した事例: [Scaling Rust builds with Bazel](https://mmapped.blog/posts/17-scaling-rust-builds-with-bazel)（BUILD を Cargo.toml から約 9 割自動生成し、rust-analyzer のために Cargo のファイルを残す）。cargo との数字つきの比較と、Tauri を Bazel でビルドした事例は見つからない。

## 新規設計（この試作で決めたこと）

- **依存は `from_specs`**: `from_cargo` が開発者モードなしで動かないため、直接の依存を MODULE.bazel に並べ、版は Cargo.lock の写し（`Cargo.bazel.lock`）で固定する。cargo-bazel が lock を範囲に刈り込んで書き戻すので、写しを渡す。
- **最適化の水準を cargo に合わせる**: 外部のクレートは `-Copt-level=2`（`profile.dev.package."*"` と同じ）。exec の設定で一度走るだけの道具、build script、proc-macro とその依存は 0（`.bazelrc` の `extra_exec_rustc_flags` と MODULE.bazel のクレートごとの注記）。注記は `*` より後に付くので後勝ちで 0 になる（`bazel aquery` で確かめた）。
- **キャッシュの置き場**: `--output_user_root`、`--repository_cache`、`--disk_cache` を `E:/dev/bazel` に置き、worktree の間で共有する。`--incompatible_strict_action_env` で PATH の違いをキーに入れない。
- github.com の releases が 504 になるため、取得できない依存を `gh api` で落として repository cache に入れる（`user.bazelrc` に `BAZEL_HTTP_RULES_URLS_AS_DEFAULT_CANONICAL_ID=0`）。

## 移すときの見積もり（エージェントの作業量）

対象はクレート 54、アプリ 2、xtask、tools。直接の外部の依存 47、Cargo.lock の 731 個。

| 対象 | 難しさ | 作業 |
|---|---|---|
| 純 Rust の自前のクレート（約 40） | 易 | BUILD を Cargo.toml から生成するスクリプト 1 本と、MODULE.bazel の spec の生成。担当 1 人で 1 回 |
| mm-doc のような OUT_DIR に書く build script | 易 | `cargo_build_script` に `data` を足す（済み） |
| libsqlite3-sys、libwebp-sys（cc） | 易 | 注記なしで通った |
| windows（0.62、大きな features） | 中 | ビルドは通る見込み。依存が増えると PATH の長さ（#3767）に当たりうる |
| mm-host-clap、mm-host-ofx（試験の plugin の DLL を cc で作る build script） | 中 | `cc_binary(linkshared)` を作り、試験の `data` で渡す形に書き直す |
| GPU のテスト（nextest の 1 テスト 1 プロセス） | 中 | rules_rust に nextest の対応は無い。`tags = ["exclusive"]` と `--test-threads=1` で並列のデバイス作成を避ける |
| mm-color-ocio（cc の C のブリッジ + bindgen、`OCIO_ROOT`） | 難 | `cc_library` と `rust_bindgen` に分けるか、build script に `OCIO_ROOT` と `LIBCLANG_PATH` を渡す（密閉されない） |
| ffmpeg-sys-next（build script の bindgen が FFmpeg のヘッダーを全部読む） | 難 | `FFMPEG_DIR`、`LIBCLANG_PATH`、1 MB のスタックの対策（`/STACK`）を注記で渡す。試験は FFmpeg の DLL を `data` に要る。libclang は用意済みのものを toolchain にする |
| cef-dll-sys（build script が CEF を落として cmake で libcef_dll_wrapper を作る） | 難 | `CEF_PATH` を渡して取得を止め、cmake と ninja を strict_action_env の外から渡す。実行時の libcef.dll と resources を `data` に並べる |
| Tauri（apps/movie-maker） | 最難 | tauri-build が tauri.conf.json、アイコン、ui の pnpm の成果物を読み、Windows のリソースを作る。先行事例なし |

全部を移すと、易と中で担当 2〜3 人、難の 4 つはそれぞれ 1 人以上で、試行錯誤の幅が大きい。

## 詰まった所

- github.com の releases のダウンロードが 504 / タイムアウトになる（上の「新規設計」の回避）。
- `.bazelignore` の `spikes` の下に BUILD を置くと、execroot にファイルが出ず rustc が読めない。試しの対象は `bazel/` に置いた。
- mm-ops の proptest `touches_が重ならない操作は別々に適用して順を入れ替えても同じ` が、この試作の元のコミット（4d8d3cec）で乱数によって落ちることがある（Bazel と cargo の両方で 1 回ずつ）。Bazel とは関係しない。
