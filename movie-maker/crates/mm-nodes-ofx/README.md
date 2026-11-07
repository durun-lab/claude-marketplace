# mm-nodes-ofx

`ofx` のノードの種類。OpenFX のプラグインで、入力のレイヤーの画像を描き直す（[04-nodes.md](../../docs/design/04-nodes.md) の `ofx`）。
描くのは別プロセスのホスト（[mm-host-ofx](../mm-host-ofx/README.md)）で、画像は CPU の画像（RGBA f32、premultiplied、線形）で往復する。

## パラメータ

| 名前 | 型 | 既定 | 意味 |
|---|---|---|---|
| `bundle` | 文字 | 空 | OpenFX の置き場所からの相対の名前（`Vendor/Foo.ofx.bundle`）か、置き場所の中の絶対パス |
| `asset` | 素材 | 空 | プロジェクトの素材のバンドル。素材のパスはバンドルのフォルダーか、その中のファイル（`Contents/Win64/Foo.ofx`） |
| `plugin` | 文字 | 空 | 効果の識別子（`pluginIdentifier`）。空ならバンドルの最初の効果 |
| 効果のパラメータ | describe の結果 | プラグインの既定 | `NodeKind::params_for` が上の 3 つの後ろに足す（隠しのもの、グループ、ページ、ボタンは出さない） |

- `bundle` と `asset` は片方だけで指す。どちらも空なら入力をそのまま返す（`is_identity`）。
- 効果のパラメータの値は、レンダラーがキーフレームを評価した値を `RenderFrame` ごとに渡す。プラグインが持たない名前の値は渡さない（効果を選び直した後に残った値で描けなくならない）。
- 入力 0（`source`）が無ければ、透明を入力にして描く（Generator の文脈のプラグインもこの形で使える）。

## 先行事例と新規の設計

| 先行事例 | 採ったもの |
|---|---|
| OpenFX の公式の HostSupport（describe の結果をキャッシュし、バンドルの更新時刻で無効にする） | describe の結果をバンドルごとに覚え、`Contents/<アーキテクチャ>/` のファイルの名前、大きさ、更新時刻が変わったら引き直す |
| OpenFX の仕様の「Installation Location」（`OFX_PLUGIN_PATH` と OS の既定のフォルダー） | バンドルを探してよい置き場所はこの 2 つ（組み立てが `mm_host_ofx::scan::plugin_dirs()` を渡す） |
| `mm-nodes-cef`（ホストの持ち方） | 読み込んだホストを上限まで置き、クラッシュしたら 1 回だけ起こし直し、時間切れが 2 回続いたら止める。毒の入ったロックはホストを全部捨ててから使う |

新規に設計したもの:

- 置き場所の決まり（`src/locate.rs`）。プロジェクトのデータは他人が書いたものでもあり、ホストは渡されたバンドルの DLL をそのまま読み込むので、種類の側で OpenFX の置き場所の中とプロジェクトの素材のバンドルだけに絞る。相対の名前の `..` とドライブを断り、実体のパス（シンボリックリンクとジャンクションを解いたもの）が置き場所の中かを確かめてからホストに渡す。
- describe の失敗も覚える。壊れたプラグインを毎フレーム読み込み直さない。プラグインを差し替えると印が変わり、引き直す。
- 実装は実装に依存しないので、種類は `mm-host-ofx` を直に呼ばない。ホストの実行ファイル、ランチャー、describe の関数（`Describe`）は組み立て（`mm-runtime` の `ofx.rs`）が渡す。実行ファイルが見つからなければ（`mm_host_ofx::find_host()`）種類を登録しない。
- ホストはプラグインごとに 1 つ起こし、別のプラグインを同じホストに読み直させない。ホストは共有ライブラリを解放しないため。

## 速さ

1920x1080、Gain（`gain.c`）、ホストの起動と読み込みの後の 1 フレーム（`tests/it/render.rs` の `フル_hd_の_1_フレームの時間を測る`）。

| ビルド | p50 |
|---|---|
| release | 1.6 s |
| dev | 5.1 s |

ほとんどは JSON の往復（`SharedFrame::Cpu` の 830 万個の f32）。release で、送る側の `Value` 化 0.24 s と文字列化 0.34 s、受ける側の読み 0.72 s を測った。プレビューには遅く、書き出しとキャッシュに向く。

## 試す

```bash
cargo nextest run -p mm-host-ofx -p mm-nodes-ofx
```

`-p mm-host-ofx` を一緒に付けるのは、ホストの実行ファイル（`target/<profile>/mm-host-ofx`）を作らせるため。
見つからなければ試験はスキップする（`MM_REQUIRE_OFX=1` で失敗にする。`MM_OFX_HOST` で実行ファイルを指せる）。
期待画像は `fixtures/golden/cpu/ofx-gain.png`（更新は `node.rs` の冒頭）。

## 残したもの

- 画像の受け渡しの速さ。CPU の画像は共有メモリで渡す（07-standards の OpenFX の行）。応答の形（`SharedFrame`）に共有メモリの形が要るので、`mm-host-api` の RFC から。GPU の受け渡し（共有テクスチャ、GPU Rendering Suite）はその後。
- IsIdentity、GetRegionOfDefinition と GetRegionsOfInterest、GetClipPreferences をホストに問わない。今は出力の全面を毎回描く。
- Transition と General の文脈、2 つ以上の入力、マスクの入力、時間方向のクリップの読み（今は入力 0 の今の時刻だけ）。
- 描く倍率（`kOfxImageEffectPropRenderScale`）を渡さない。プレビューの縮小した大きさでは、半径のような画素の単位のパラメータの見え方が書き出しと違う。
- フレームレート。種類はコンポジションのフレームレートを知らないので、時刻をフレームの番号に直すのは 30 fps の既定のまま。
- describe の結果はプロセスの中だけで覚える（起動のたびに 1 回 describe する）。ディスクのキャッシュは `cache/` の担当と決める。
- `params_for` は素材のバンドル（`asset`）を引けず、自前の 3 つだけを返す（`params_for` に素材のパスを引く手段が無い）。
- レンダラーのフレームのキャッシュのキーにプラグインの印が入らない。プラグインを差し替えても、キャッシュに残った前の画素を返すことがある。
- InstanceChanged、オーバーレイ（Interact）、描画の取り消し、色の管理（OpenFX 1.5）は mm-host-ofx の残り。
