# RFC 0139: VST3 のホスト（mm-host-vst3）と、音のプラグインのホストの共通部分（mm_host_pipe::audio）

状態: 提案

## 変える理由

- 段階 8 の VST3（07-standards の「VST3、CLAP」の行）を、CLAP（RFC 0133）と同じくレイヤーの音に挿せる効果として使えるようにする。
- `mm-host-clap` の共有メモリの並び、アプリ側の往復、種類の本体（インスタンスの管理、シークでの消去、ホストの起こし直し）、`describe` と要求の繰り返しは規格に依らない。実装どうしは依存しない（依存の規則 3）ので、VST3 のクレートから CLAP のクレートを使えない。RFC 0133 は「VST3 を足すときに共通実装へ移す」とした。

## 決めたこと

### インターフェース（`mm-*-api`）

変えない。音の効果は RFC 0133 の `NodeKind::process_audio`、`AudioEffectCx`、`AUDIO_PARAM_STEP` をそのまま使う。

### プロジェクトのデータ

変えない。効果は `Layer.effects` に種類 `audio.vst3` のノードとして置き、パラメータは `plugin`（`.vst3` のバンドル）、`plugin_id`（クラスの ID の 32 桁の 16 進）、`param.<ParamID>`（正規化した値 0..=1）に持つ。

### クレート

| クレート | 依存 | 内容 |
|---|---|---|
| `mm-host-pipe`（共通実装） | 足すもの: `mm-audio-api`、`mm-node-api`、`serde` | `audio` のモジュール。`PluginStandard`（規格ごとの名前）、`AudioEffect<S>`、`PluginEffectKind<S>`（`NodeKind` の実装）、共有メモリの並び（`block`）、記述の JSON（`desc`）、`describe_bundle`、`find_host`、ホストの実行ファイルの側（`serve`: サブコマンド、要求の繰り返し、共有メモリの要求の繰り返し） |
| `mm-host-clap` | `mm-host-pipe`、`mm-types`、`clack-host`、`clack-extensions` | `Clap`（`PluginStandard`）、`ClapEffect` と `ClapEffectKind` は型の別名。公開の名前と形は変わらない（使われていなかった `ClapLauncher` は消した） |
| `mm-host-vst3`（新規） | `mm-host-pipe`、`mm-types`、`libloading`、`vst3` | `Vst3`、`Vst3Effect`、`Vst3EffectKind`、バンドルの形、ホストの実行ファイル `mm-host-vst3`（`serve`、`describe`） |

- `mm-host-pipe` は依存の規則 3 の共通実装のままで、足す依存は API クレートと基礎だけ。
- 依存の規則 3 の「別プロセスのホストは `mm-host-pipe` に依存してよい」に `mm-host-vst3` を並べる（14-dev-structure の更新は統合の担当）。
- 07-standards は VST3 と CLAP を 1 つの `mm-host-audio` に置く案だった。段階 8 の「全部が別の実装のクレート」に合わせて実装のクレートは分け、共通の部分を `mm_host_pipe::audio` に置いた。
- `vst3` クレート 0.3（MIT / Apache-2.0）は生成済みのバインディングを同梱し、VST3 SDK（3.8.0 から MIT）を要らない。テストのプラグインも同じクレートで書く。
- `mm-runtime` は wgpu のレンダラーのレジストリに `audio.vst3` を組み立てごとに 1 つ登録する（`audio.clap` と同じ理由）。ホストの実行ファイルは `MM_VST3_HOST` か、実行ファイルと同じフォルダーの `mm-host-vst3`。

## 影響

- API クレートとプロジェクトのデータは変わらない。
- `mm-host-clap` の共有メモリの先頭のマジックが `MMCL` から `MMAU` に変わる。アプリとホストは同じビルドで作るので、互いのバージョンがずれることは無い。
- `mm-host-pipe` を使う `mm-host-cef`、`mm-host-ofx`、`mm-nodes-ofx` のビルドに `mm-node-api` が加わる（すでに依存の木にある）。
- `mm-runtime` の wgpu のレジストリの種類が 1 つ増える（`assemble_real` に名前の確かめを 1 行足した）。
