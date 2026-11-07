# mm-host-vst3

VST3 の音のプラグイン（`.vst3` のバンドル）を別プロセスで読み込み、音のブロックを処理するホストと、レイヤーの効果として挿す種類 `audio.vst3`（`mm-node-api` の実装）。
段階 8 では、列挙（Audio Module Class とパラメータの一覧）、ステレオ 48 kHz の効果の処理、パラメータのキーフレーム、クラッシュと時間切れからの回復までを持つ。
受け渡しと種類の本体は `mm-host-clap` と共通の `mm_host_pipe::audio` で、このクレートは VST3 の名前、バンドルの形、ホストの実行ファイルを持つ。

## 分け方

| 部分 | 場所 | 役目 |
|---|---|---|
| 規格の名前（`Vst3`）、`Vst3Effect`、`Vst3EffectKind` | `src/lib.rs` | `mm_host_pipe::audio` の `AudioEffect` と `PluginEffectKind` に VST3 の名前（種類、実行ファイル、時間切れの `what`）を渡す |
| バンドルの形 | `src/bundle.rs` | バンドルの中のモジュール（`Contents/x86_64-win/<名前>.vst3`）、既定の置き場所、クラスの ID の 16 進 |
| モジュールとファクトリー | `src/host/module.rs` | `InitDll`、`GetPluginFactory`、クラスの一覧（`IPluginFactory2`、無ければ `IPluginFactory`）、インスタンスの作成 |
| インスタンス | `src/host/plugin.rs` | 処理と編集の部品の作成と接続、バスとパラメータの記述、活性化、状態の消去、バンドルの列挙 |
| 処理 | `src/host/process.rs` | 共有メモリのブロックを `ProcessData` にして `process` を呼ぶ |
| ホストの部品 | `src/host/context.rs` | `IHostApplication`、`IComponentHandler`、`IParameterChanges`、`IEventList` |
| `serve` | `src/host/serve.rs` | 音のスレッドを 1 つ起こし、読み込みから処理までをそのスレッドで行う |
| テストのプラグイン | `tests/plugins/mm_vst3_test.rs` | `cargo test` が例（cdylib）として作る。gain と 1 サンプルの遅延 |

## バインディングの選び方

| 候補 | 形 | ライセンス | 判断 |
|---|---|---|---|
| `vst3` 0.3（coupler-rs） | C++ のヘッダーから `com-scrape` で生成したバインディングを同梱（0.3.0 から SDK が要らない）。COM の口を Rust の型で実装できる（`Class`、`ComWrapper`） | MIT / Apache-2.0 | 採った。07-standards の指定と同じで、テストのプラグインも同じクレートで書ける |
| `vst3-sys`（RustAudio） | 手で書いたバインディングと COM の補助（`vst3-com`） | GPL-3.0 | 採らない。crates.io に無く（git だけ）、2020 年ごろから止まっている |
| VST3 SDK（C++）を `cc` で作る | 公式の参考の実装（`hosting/`、`vstsdk`） | MIT（3.8.0 から） | 採らない。CMake と C++ のビルドが要り、ホストの本体を C++ で書くことになる |

VST3 SDK は 3.8.0（2025 年 10 月）から MIT になった。このクレートは SDK のファイルを持たず、ビルドの時に取りにも行かない。テストのプラグインも `vst3` クレートで書くので、`MM_REQUIRE_VST3` のようなスキップの仕組みは要らない。

## 先行事例と決めたこと

| 先行事例 | 形 | 採ったもの |
|---|---|---|
| `mm-host-clap`（RFC 0133） | 別プロセス、制御はパイプの JSON、音は共有メモリで同じ呼び出しの中で往復、インスタンスごとに 1 プロセス、シークで `reset` | 全部。共通の部分は `mm_host_pipe::audio` に移して両方が使う |
| VST3 SDK の参考のホスト（`audiohost`、`validator`） | 処理の部品を作って初期化し、編集の部品が別のクラスなら作って `IConnectionPoint` で互いにつなぐ。配置、バスの活性、`setupProcessing`、`setActive`、`setProcessing` の順 | 同じ手順 |
| REAPER、Bitwig のプラグインの隔離 | 別プロセスで動かし、落ちたプラグインだけを止める〔推〕 | `mm-host-clap` と同じ |
| VST3 のホストのオートメーション（Cubase など） | パラメータを正規化した値（0..1）で書き、`IParameterChanges` のサンプルの位置つきの点で渡す〔推〕 | パラメータは正規化した値で持ち、格子の点を `IParamValueQueue` の点にする |

新しく決めたこと:

- 種類 `audio.vst3` のパラメータ `param.<id>` は VST3 の `ParamID` の正規化した値（0..=1）。CLAP は plain value だが、VST3 は `IParameterChanges` が正規化した値を運び、plain value への変換（`normalizedParamToPlain`）は編集の部品の仕事で処理のスレッドで呼べないため。範囲の外の値は 0..=1 に収める。
- `plugin_id` はクラスの ID の 32 桁の 16 進。並びはプラグインのソースの `FUID (a, b, c, d)` と同じで、`moduleinfo.json` の `CID` と同じ文字になる。空ならバンドルの最初の Audio Module Class。
- 状態の消去（シーク）は `setProcessing(false)`、`setActive(false)`、`setActive(true)`、`setProcessing(true)`。VST3 には CLAP の `reset` に当たる口が無い。SDK の文書は `setProcessing` を「遅延やリバーブの中身を消してよい所」とし、`setActive` は確保し直しを含む入れ直しなので、両方を呼べばどちらで消すプラグインも消える。
- ホストのプロセスの中では、読み込みから処理までを 1 つのスレッドで呼ぶ。VST3 は初期化を主のスレッド、処理を音のスレッドと決めているが、編集の画面を開かないこのホストでは、プラグインから見える呼び出しの順を 1 本にする方が単純で、プラグインが 2 つのスレッドの取り合いで止まらない。
- 位置の情報（`processContext`）は、テンポ 120、4/4 拍子、止まっている状態、時刻 0 を毎回渡す。空にすると落ちるプラグインがあり、時刻を進めると始めた位置で結果が変わるため。
- 主でないバス（サイドチェインなど）は止め、無音の入力と捨てる出力を渡す。ノートのイベントは渡さない（空の `IEventList`）。
- 共有ライブラリは解放しない。`describe` で作ったインスタンスも片付けずにプロセスごと捨てる（片付けの呼び出しで落ちるプラグインがあるため）。

## 遅れと性能

- 受け渡しは同じブロックの中の往復で、ブロックの遅れを足さない。
- 往復の時間（`tests/it/perf.rs`、gain のプラグイン、2000 回、手元の Windows 11）:

| フレーム数 | デバッグの p50 / p99 | リリースの p50 / p99 | ブロックの長さ |
|---|---|---|---|
| 128 | 36 µs / 68 µs | 12 µs / 32 µs | 2.67 ms |
| 512 | 75 µs / 134 µs | 14 µs / 35 µs | 10.7 ms |
| 4096 | 496 µs / 2.6 ms | 19 µs / 57 µs | 85.3 ms |

- CLAP より数 µs 遅いのは、毎回 `IParameterChanges` を詰め直し、テストのプラグインがパラメータの点を並べ直すため。
- プラグインが申告する遅れ（`getLatencySamples`）は補わない（下の「段階 8 の後に回すもの」）。

## 決定性

- 同じ入力と同じパラメータなら、ブロックの分け方（書き出しの 1 回、再生の短い区間の連続、共有メモリに入る 4096 フレームごとの分割）に依らず同じサンプルになる。`tests/it/process.rs` と `tests/it/kind.rs` が確かめる。
- 書き出しでも `kOffline` のモードにしない（`kRealtime` のまま）。プラグインが offline で処理を変えると、再生と書き出しの音がずれるため。
- 区間が前の区間の続きでなければ状態を消す。0 から再生した音と書き出しの音は一致する。
- プラグインがクラッシュするか期限を過ぎたら、そのブロックは効果をかけずに続け（レンダラーがエラーを記録する）、次のブロックでホストを起こし直す。状態は失われ、パラメータは次のブロックの先頭で全部送り直す。

## 受け渡しの規約

`mm-host-clap` の README の「受け渡しの規約」と同じ（共有メモリの名前の頭は `Local\mm-vst3-`、時間切れの `what` は `host_vst3.process`）。違いは次のとおり。

- `Prepare` の `path` は `.vst3` のバンドル（フォルダー）か 1 つのファイル、`plugin` はクラスの ID の 16 進。
- ホストは主のバスをステレオに配置できるか確かめ、32 ビットの浮動小数、最大 4096 フレーム、`kRealtime` で活性にする。

## テスト

```bash
cargo nextest run -p mm-host-vst3
```

`cargo test`（nextest も同じ）が例 `mm_vst3_test` を DLL に作り、`tests/it/` が試験のプロセスごとにバンドルの形（`MmTest.vst3/Contents/x86_64-win/MmTest.vst3`）に写して読む。`--test it` だけを作ると例が無く、試験は理由つきで落ちる。

| プラグイン | 形 | 確かめること |
|---|---|---|
| `MM Test Gain`（`4D4D544553544741494E000000000001`） | 処理と編集を 1 つの部品が兼ねる | 列挙とパラメータの一覧、gain の値、パラメータの効く位置、範囲に収めること、分け方に依らない結果、クラッシュ（`crash`）と時間切れ（`sleep`）からの回復、`println!` が応答に混ざらないこと |
| `MM Test Delay`（`4D4D5445535444454C41590000000002`） | 編集が別のクラス | 別の編集のクラスの作成、ブロックの切れ目をまたぐ遅延、状態の消去、シークで状態を消すこと、書き出しと再生の一致 |

手元のプラグインを列挙するには、`mm-host-vst3 describe <.vst3 のバンドル>` を実行する。既定の置き場所は Windows で `C:\Program Files\Common Files\VST3` と `%LOCALAPPDATA%\Programs\Common\VST3`（`default_dirs`、`scan_dir`）。

## 段階 8 の後に回すもの

- プラグインの遅れの補正（`getLatencySamples`）と尾（`getTailSamples`）。
- プラグインの状態の保存（`IComponent::getState`、編集の部品への `setComponentState`）、編集の画面（`IPlugView`）、値の表示の文字（`getParamStringByValue`）、単位の名前（`IUnitInfo`）。
- ノートのイベント（楽器）、モノラルとサラウンドの配置、バスへの挿し込み。
- 64 ビットの浮動小数での処理（`kSample64`）。
- macOS と Linux のモジュールの入口（`bundleEntry`、`ModuleEntry`）。今は Windows の `InitDll` だけを呼ぶ。
- 置き場所の走査と describe の結果のキャッシュ、Inspector でプラグインを選ぶ画面、使わなくなったインスタンスのホストの片付け（`mm-host-clap` と同じ）。
