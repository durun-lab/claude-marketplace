# mm-host-ofx

OpenFX（OFX）のプラグイン（`.ofx` の DLL）を別プロセスで読み込み、1 フレームを描いて返すホスト（`mm-host-api` の実装）。
段階 6 で骨組みと 1 本の通し（列挙、パラメータの一覧、CPU の画像での 1 フレーム）を持ち、段階 8 でノードの種類 `ofx`（[mm-nodes-ofx](../mm-nodes-ofx/README.md)）から使う。

## 分け方

| 部分 | 場所 | 役目 |
|---|---|---|
| アプリ側のランチャー（`OfxLauncher`、`PipeHost`、`describe_bundle`） | `src/launcher.rs` | `mm-host-ofx serve` と `mm-host-ofx describe` を子プロセスで起こす |
| 実行ファイルの探し方（`find_host`）とテストのプラグインの置き場所（`TEST_PLUGINS`） | `src/locate.rs` | 組み立てが種類を登録するかを決める。`MM_OFX_HOST` か、アプリの実行ファイルの隣か親のフォルダー |
| 記述と要求の形（`BundleDesc`、`PluginDesc`、`ParamDesc`、`PrepareParams`、`RenderParams`） | `src/desc.rs`、`src/lib.rs` | ホストとアプリの間の JSON |
| プラグインの置き場所とバンドルの形 | `src/scan.rs` | `OFX_PLUGIN_PATH` と OS の既定のフォルダー |
| ホストの実行ファイル | `src/host/` | OpenFX の C API のホスト（Property、Image Effect、Parameter、Memory、MultiThread、Message の Suite） |
| 可変長引数のブリッジ | `csrc/shim.c` | Rust の安定版では可変長の関数を定義できないので、`paramGetValue` などを C で受けて Rust へ渡す |
| 公式のヘッダー | `vendor/openfx/` | OpenFX の `include/`（BSD-3-Clause、コミット `b2d2eea`）。Rust の構造体の大きさをこれとテストで突き合わせる |
| テストのプラグイン | `tests/plugins/` | `build.rs` が公式のヘッダーでバンドルの形に作る |

## 先行事例と決めたこと

| 先行事例 | 形 | 採ったもの |
|---|---|---|
| OpenFX の公式の HostSupport（C++） | 同じプロセス。describe の結果をキャッシュのファイルに書き、次の起動で DLL を読まない | describe の結果を JSON の記述（`BundleDesc`）にする。キャッシュは種類の側（mm-nodes-ofx） |
| Natron（GPLv2、HostSupport を使う） | 同じプロセス。プラグインのクラッシュで Natron ごとクラッシュする | 何も流用しない（GPL が及ぶ） |
| Nuke、DaVinci Resolve | 描画は本体のプロセス〔推〕。Resolve はインストール先に `OFXLoader.exe` を持ち、プラグインの読み込みを本体の外で行う〔推〕 | 読み込みを本体の外に置く |
| 音のプラグインのホスト（REAPER、Bitwig、Ardour の `ardour-vst-scanner`） | 走査を 1 プラグインずつ別プロセスで行い、走査中のクラッシュでホストを巻き込まない | `describe` を 1 バンドル 1 プロセスにする |
| `mm-host-cef` | 別プロセス。標準入出力の JSON の 1 行ずつ、期限を過ぎた応答の読み捨て | 同じプロトコルと同じランチャーの形 |

設計書の方針（README の 7「クラッシュしうるものは別のプロセスに置く」、04-nodes の `out_of_process`、07-standards の OpenFX の行）に従い、別プロセスにした。

新しく決めたこと:

- パラメータの値は時刻ごとに持たない。アプリが時刻で評価した値（`mm_types::Params`）を `RenderFrame` ごとに渡し、プラグインがどの時刻で読んでもその値を返す。キーフレームは 0 個に見える。
- プラグインの `printf` が応答の JSON に混ざらないよう、起動直後に標準出力を複製して応答に使い、元の標準出力は標準エラーへ付け替える。
- クラッシュのダイアログ（Windows のエラー報告）を出さない。プロセスがすぐ終わり、アプリには `HostCrashed` で届く。
- 共有ライブラリは解放しない。プラグインが残したスレッドや終了時の処理が、解放後にクラッシュすることがある。プロセスごと捨てる。

## 受け渡しの規約

- `OfxLauncher::spawn` は `mm-host-ofx serve` を起こし、環境変数 `MM_HOST_SPEC` に `HostSpec` の JSON を渡す。制御は標準入出力の JSON の 1 行ずつ。
- `Prepare` の `params` は `PrepareParams`（`{"bundle": "<名前>.ofx.bundle", "plugin": "<id>", "frame_rate": 30}`。`plugin` と `frame_rate` は省ける）。プラグインを load、describe し、インスタンスを作る。`bundle` はここでは確かめない。任意のパスの DLL を読み込むので、アプリの側（mm-nodes-ofx）が OpenFX の置き場所とプロジェクトの素材のバンドルだけを渡す。
- `RenderFrame` の `params` は `RenderParams`（`{"source": <SharedFrame の Cpu>, "values": <Params>}`。どちらも省ける）。`source` が無ければ透明を入力にする。時刻はフレームレートでフレームの番号に直す。
- `RenderParams` に `shared`（`{"name": "Local\mm-ofx-<pid>-<番号>", "has_source": true}`）があれば、画像は JSON に入れない。アプリが作った名前つきの共有メモリ（`mm_host_pipe::shm`）の `[入力 | 結果]`（どちらも `frame_bytes` バイトの RGBA f32、リトルエンディアン、上の行から）から入力を読み、結果を書き、応答の `SharedFrame::Cpu` の `data` を空にする。開いた区画は名前ごとにキャッシュし、フレームごとに開き直さない。
- `frame_rate` は正の有限の数だけを受ける。`RenderFrame` の `size` は幅と高さが 1 以上、1 辺 16384 以下、画素の数が 8K UHD 以下だけを受け、それ以外は `Error` を返す。
- 応答は CPU の画像（`SharedFrame::Cpu`、RGBA f32、premultiplied、線形）。プラグインが書いた NaN と Inf は 0 にする（JSON に書けないため）。`client_pid` があっても共有テクスチャは返さない（段階 8）。
- Windows ではプラグインの DLL を `LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR | LOAD_LIBRARY_SEARCH_DEFAULT_DIRS` で読み込み、依存の DLL をバンドルのフォルダーとシステムのフォルダーから探す。
- MultiThread の `multiThread` は、プラグインが頼む数に依らず CPU の数までのスレッドで番号を分け合う。スレッドを起こせなければ呼んだスレッドで実行する。
- 文脈は Filter を優先し、無ければ Generator。画素の深さはプラグインが対応するもののうち Float、Short、Byte の順に選び、全部のクリップをそろえる。整数の深さは 0〜1 に切って量子化するだけで、伝達関数は掛けない。
- `describe_bundle` は `mm-host-ofx describe <バンドル>` を実行し、`BundleDesc` の JSON を 1 行で受け取る。読めなかったプラグインは `skipped` に理由つきで入る。
- `PluginDesc::param_schema` は、隠しのパラメータと値を持たないもの（グループ、ページ、ボタン）を除いて `ParamSchema` にする。整数の型も `Number` になる。

## テスト

```bash
cargo nextest run -p mm-host-ofx
```

`build.rs` が 3 つのテストのプラグインを `OUT_DIR/plugins/` に作る（C コンパイラーが要る。Windows は MSVC の `cl`）。

| プラグイン | 元 | 確かめること |
|---|---|---|
| `Invert.ofx.bundle` | OpenFX の公式のサンプル `Examples/Invert/invert.cpp`（BSD-3-Clause、そのまま） | 列挙、8 ビットの 1 フレーム |
| `MmTestGain.ofx.bundle` | `tests/plugins/gain.c` | パラメータの 4 つの型の一覧と値、float の 1 フレーム、可変長の `paramGetValueAtTime` と `message`、`printf`、時間切れ（`delay_ms`）、クラッシュ（`crash`）と再起動、有限でない値（`nonfinite`）、`mm-host-api` の契約、共有メモリの受け渡し |
| `MmTestGainV2.ofx.bundle` | `tests/plugins/gain.c`（`MM_GAIN_SCALE=2.0`） | 識別子が同じで gain が 2 倍。共有ライブラリの差し替え（mm-nodes-ofx のテスト） |

手元のプラグインを列挙するには、`OFX_PLUGIN_PATH` か既定のフォルダー（Windows は `C:\Program Files\Common Files\OFX\Plugins`）に置き、`mm-host-ofx describe <バンドル>` を実行する。

## 段階 8 に回すもの

- GPU の受け渡し（共有テクスチャ、OpenFX の GPU Rendering Suite）。今は CPU の画像を共有メモリで往復する（1080p で release 26 ms、mm-nodes-ofx の「速さ」）。
- GetClipPreferences、IsIdentity、GetRegionOfDefinition と GetRegionsOfInterest、時間方向のクリップの読み、General と Transition の文脈、マスクの入力。
- InstanceChanged（パラメータを変えたときの通知）、オーバーレイ（Interact）、描画の取り消し（`abort`）。
- 色の管理（OpenFX 1.5 の Colour Management）、Half の深さ、RGB と Alpha だけの成分。
