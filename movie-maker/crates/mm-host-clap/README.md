# mm-host-clap

CLAP の音のプラグイン（`.clap`）を別プロセスで読み込み、音のブロックを処理するホストと、レイヤーの効果として挿す種類 `audio.clap`（`mm-node-api` の実装）。
段階 8 では、列挙（descriptor とパラメータの一覧）、ステレオ 48 kHz の効果の処理、パラメータのキーフレーム、クラッシュと時間切れからの回復までを持つ。

## 分け方

| 部分 | 場所 | 役目 |
|---|---|---|
| 種類 `audio.clap`（`ClapEffectKind`） | `src/kind.rs` | 効果のインスタンスごとにホストを 1 つ持つ。続きでない区間ではプラグインを `reset` する |
| 1 つのインスタンス（`ClapEffect`） | `src/effect.rs` | ホストを起こし、共有メモリで往復させる。パラメータは差だけをイベントにする |
| 共有メモリとイベント | `src/shm.rs`、`src/shm/imp.rs` | 並び（`Block`）と、Windows の名前つきのファイルマッピングと自動で戻るイベント |
| ランチャー（`ClapLauncher`、`describe_bundle`、`find_host`） | `src/launcher.rs` | `mm-host-clap serve` を `mm_host_pipe::PipeHost` で起こし、`describe` を 1 ファイル 1 プロセスで実行する |
| 記述と要求の形（`BundleDesc`、`PluginDesc`、`ParamDesc`、`PrepareParams`） | `src/desc.rs` | ホストとアプリの間の JSON |
| ホストの実行ファイル | `src/host/` | `clack-host` でプラグインを読み込み、音のスレッドで処理する |
| 公式のヘッダー | `vendor/clap/` | CLAP の `include/`（MIT、タグ `1.2.6`）。テストのプラグインを作るためだけに使う |
| テストのプラグイン | `tests/plugins/mm_test.c` | `build.rs` が `MmTest.clap` に作る |

## 先行事例と決めたこと

| 先行事例 | 形 | 採ったもの |
|---|---|---|
| Bitwig Studio のプラグインの隔離 | プラグインを本体と別のプロセスで動かし、クラッシュしたプラグインだけを止めて知らせる。隔離の単位（全部まとめて、メーカーごと、1 つずつ）を選べる〔推〕 | インスタンスごとに 1 プロセス。クラッシュはそのブロックの失敗にして、次のブロックで起こし直す |
| REAPER のブリッジ（別プロセスで動かす設定） | 音を共有メモリで渡し、同じブロックの中で往復させる〔推〕 | 共有メモリで同じブロックの中で往復させ、ブロックの遅れを足さない |
| `free-audio/clap-host`（公式の参考のホスト）、`clack` の cpal の例 | 同じプロセスで、主スレッドがインスタンス、音のスレッドが `process` を持つ | ホストのプロセスの中は同じ形（主スレッドでインスタンスと活性化、音のスレッドで処理） |
| `mm-host-ofx`、`mm-host-cef` | 別プロセス、制御は標準入出力の JSON の 1 行ずつ、ジョブオブジェクト、`describe` は 1 ファイル 1 プロセス | `mm_host_pipe::PipeHost` と同じ形の `describe`、クラッシュのダイアログを出さない、標準出力の付け替え |
| `DspMixer` のダッキング（`mm-audio-dsp`） | 区間が前の区間の続きならミキサーを持ち越し、離れていればシークとして作り直す | 効果も同じ。続きでなければ `reset` |

設計書の方針（README の 7「クラッシュしうるものは別のプロセスに置く」、04-nodes の `out_of_process`、06-engine の「共有メモリで音を往復させる」、07-standards の CLAP の行）に従った。

新しく決めたこと:

- 音のブロックはパイプを通さない。制御（`Ping`、`Prepare`、`Shutdown`）だけを `mm-host-api` の JSON で送り、音とパラメータは共有メモリに書いて要求のイベントを立てる。`mm-host-api` は変えていない。
- パラメータはコンポジションの時間軸の 0 から 64 フレーム（`AUDIO_PARAM_STEP`）ごとの格子でレンダラーが評価し、プラグインが持つ値との差だけを、その位置の CLAP のパラメータのイベントで渡す。
- 1 つのホストのプロセスで `Prepare` は 1 回だけ。別のプラグインに替えるときはホストを起こし直す（`ClapEffectKind` が `plugin` か `plugin_id` の変化で作り直す）。
- 共有ライブラリは解放しない（`mm-host-ofx` と同じ理由）。プロセスごと捨てる。

## 遅れと性能

- 受け渡しは同じブロックの中の往復で、ブロックの遅れを足さない。`process_audio` は返るまでに処理の済んだ音を受け取る。
- 往復の時間（`tests/it/perf.rs`、gain のプラグイン、2000 回、手元の Windows 11）:

| フレーム数 | デバッグの p50 / p99 | リリースの p50 / p99 | ブロックの長さ |
|---|---|---|---|
| 128 | 28 µs / 63 µs | 12 µs / 27 µs | 2.67 ms |
| 512 | 58 µs / 114 µs | 14 µs / 32 µs | 10.7 ms |
| 4096 | 313 µs / 452 µs | 33 µs / 85 µs | 85.3 ms |

- パイプの JSON にしなかった理由: 512 フレームのステレオは JSON で 10 KB を超え、エンコードと読む側のスレッドの受け渡しが毎ブロックに乗る。共有メモリなら写しは f32 の配列の写しだけ。
- プラグインが申告する遅れ（`clap.latency`）は補わない（下の「段階 8 の後に回すもの」）。

## 決定性

- 同じ入力と同じパラメータなら、ブロックの分け方（書き出しの 1 回、再生の短い区間の連続、共有メモリに入る 4096 フレームごとの分割）に依らず同じサンプルになる。`tests/it/process.rs` と `tests/it/kind.rs`、`mm-render` の `tests/it/audio_effects.rs` が確かめる。
- 位置の情報（`steady_time`、トランスポート）はプラグインに渡さない。始めた位置で結果が変わらないようにするため。
- 書き出しでも `clap.render` の offline のモードにしない。プラグインが offline で処理を変えると、再生と書き出しの音がずれるため。
- 区間が前の区間の続きでなければ（シーク）`reset` する。再生を途中から始めると、その直前から鳴り続けるはずの残り（遅延、リバーブの尾）は入らない。0 から再生した音と書き出しの音は一致する。
- プラグインがクラッシュするか期限を過ぎたら、そのブロックは効果をかけずに続け（レンダラーがエラーを記録する）、次のブロックでホストを起こし直す。状態は失われ、パラメータは次のブロックの先頭で全部送り直す。

## 受け渡しの規約

- `ClapEffect::start` はアプリ側で共有メモリ（`Local\mm-clap-<pid>-<n>-mem`）とイベント（`-req`、`-done`）を作り、`mm-host-clap serve` を起こして `Prepare` を送る。
- `Prepare` の `params` は `PrepareParams`（`{"path": "<.clap>", "plugin": "<id>", "sample_rate": 48000, "link": "Local\\mm-clap-..."}`。`plugin` は省ける）。ホストはプラグインを読み込み、ステレオの入力と出力（主のポートが 2 ch ずつ）を確かめ、最小 1、最大 4096 フレームで活性にし、記述（`PluginDesc` の JSON）を共有メモリに書いて `Prepared` を返す。
- 1 回の往復: アプリが入力（チャンネルごとに分けた f32）、パラメータのイベント（位置の昇順、最大 4096）、フレーム数、要求の番号を書いて要求のイベントを立てる。ホストは処理して出力と完了の番号を書き、完了のイベントを立てる。アプリは完了のイベントとホストのプロセスの終わりを同時に待つ（`WaitForMultipleObjects`）。
- `RenderFrame` には `Error` を返す。
- 種類 `audio.clap` のパラメータは `plugin`（`.clap` のファイル）、`plugin_id`（空ならファイルの最初のもの）、`param.<id>`（CLAP のパラメータの `id` を 10 進で。plain value で、範囲の外の値は範囲に収める）。`plugin` が空なら何もしない。
- 画像には何もしない（`is_identity` が真）。

## テスト

```bash
cargo nextest run -p mm-host-clap
```

`build.rs` がテストのプラグイン `MmTest.clap` を `OUT_DIR/plugins/` に作る（C コンパイラーが要る。Windows は MSVC の `cl`）。

| プラグイン | 確かめること |
|---|---|
| `jp.durun.mm.test.gain` | 列挙とパラメータの一覧、gain の値、パラメータの効く位置、範囲に収めること、分け方に依らない結果、クラッシュ（`crash`）と時間切れ（`sleep_ms`）からの回復、`printf` が応答に混ざらないこと |
| `jp.durun.mm.test.delay` | ブロックの切れ目をまたぐ遅延、`reset`、シークで状態を消すこと、書き出しと再生の一致 |

手元のプラグインを列挙するには、`mm-host-clap describe <.clap のファイル>` を実行する。既定の置き場所は Windows で `C:\Program Files\Common Files\CLAP` と `%LOCALAPPDATA%\Programs\Common\CLAP`。

## 段階 8 の後に回すもの

- VST3（`vst3` クレートと SDK）。共有メモリの受け渡し（`shm.rs`）はそのとき共通実装へ移す。
- プラグインの遅れの補正（`clap.latency`）と尾（`clap.tail`）。今は遅れのあるプラグインの音がその分だけ遅れて出る。
- プラグインの状態の保存（`clap.state`）、GUI（`clap.gui`）、値の表示の文字（`value_to_text`）。
- バスへの挿し込み（`CompMixer` のバスの効果）と、モノラルとサラウンドの配置。
- 置き場所の走査と describe の結果のキャッシュ、Inspector でプラグインを選ぶ画面。
- 使わなくなったインスタンスのホストの片付け（今は組み立てを捨てるまで残る）。同じ入れ子を 2 か所に置くと、中のレイヤーの効果が同じインスタンスを分け合い、互いに `reset` し合う。
- Windows のほかの環境の共有メモリ（今は `Unsupported`）。
