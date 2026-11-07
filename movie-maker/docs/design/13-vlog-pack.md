# vlog をこの上で作る

DTM_Vlog の作り方は、movie-maker の上の「パック」として持つ。基盤は vlog を知らない。

| DTM_Vlog の今 | movie-maker での形 |
|---|---|
| `pool.yaml` の候補 | `bins` の `selection` |
| `edit.yaml` のシーケンス | 本編のコンポジションの `sequence` |
| 字幕、テロップ、図解（`overlay.mjs` と 7_mix） | 字幕は `text`。ステッカーと図解は `web.cef` のテンプレート。カットに `attachTo` |
| トランジション（`tools/transitions/fx/*.js`、WebGL2） | WGSL に書き直し、パックの `transitions/*.wgsl` に置く。`transition` のマクロが `transition.wgsl` の種類で `setTransition` する。マット、重ねる絵、映像の動きを 1 つのシェーダーで合成する仕組みは `transitions/_prelude.wgsl` |
| 巻き戻し（`design/templates/rewind.html`） | `transitions/rewind.wgsl` の絵と、レンダラーの `rewind`（出ていくカットを逆向きに描く）。`rewind` のマクロで置く |
| ショート | 本編の `sequence` を参照した縦のコンポジションと、そのレンダーターゲット |
| 編集の好み | パックのマクロと、ミキサーのダッキング |
| ディレクション UI の書き込み | `Review` のコメント |
| `seams.py`、`listen.py` | 対応のトランザクションに付く数値の点検 |

ep04 から、この形で作れるかを試す。

`web.cef` を使わずに作れる分（色と書体の決まり、文字のスタイル、エピソードのテンプレート、マクロ、ダッキングの設定）は `packs/vlog` にあり、読み込みと検査と展開は `crates/mm-pack`、使う経路は `mm pack` とアプリの `pack_list`・`pack_apply`。中身と、段階 6 に回すものの一覧は [packs/vlog/README.md](../../packs/vlog/README.md)。

`web.cef` の部品は `packs/vlog/web/*.html` に置き、マクロの `web` 手順がレイヤーにする（テーマの色と書体はページの `data.theme` で渡す）。部品の一覧は同じ README の「web の部品」、書き方は「web の部品の書き方」。時刻で中身が変わる部品（歌詞、図解の cues、読む窓、Claude の吹き出し）は `animated: true` で、レイヤーの始まりからの秒 `t` と `data` だけで画面を決める。

## ショートに置く

パックの位置と幅は本編の画面（`theme.json` の `canvas`、1920x1080）の画素で書く。同じマクロを大きさの違うコンポジション（ショート、1080x1920）に置いても、部品が画面の外へ出たり形が伸びたりしないようにする。

### 先行事例

| 事例 | 大きさの違う画面での位置の決め方 | 採ったこと |
|---|---|---|
| CSS の `right`・`bottom`、`min()` | 箱の右・下の縁からの距離で置き、幅を `min(固定, 割合%)` で縮める | 基準点に近い縁からの距離を保つ。狭い画面で折り返しの幅を割合で縮める |
| Premiere の Essential Graphics の「レスポンシブデザイン（位置）」、After Effects の Essential Graphics の位置の固定 | レイヤーを画面の縁に固定し、シーケンスの大きさが変わっても縁からの距離を保つ | 同上（固定する縁は文字のスタイルの `anchor` で決まる） |
| CSS の `object-fit: cover`、Premiere の「フレームサイズに合わせる（塗りつぶし）」 | 縦横比を保って画面を覆うように広げ、はみ出た所を切る | トランジションの基準の画面を縦横同じ倍率で写す |
| Premiere の書き出し | シーケンスの画面を座標の基準にし、出力の大きさへ拡大・縮小する | `--sequence` の書き出しは見せ方を書いたコンポジションの大きさで描く（[06-engine](06-engine.md)） |

### 新しい設計

- 展開は置く先のコンポジションの大きさをプロジェクトから引く（`mm_pack::expand_for`。`mm pack apply` とアプリの `pack_apply` が使う）。プロジェクトを渡さない `expand`（`mm pack expand`）と、同じトランザクションで足すコンポジションは本編の画面の大きさで置く。
- 文字のスタイル: 位置は `anchor` と同じ割合の画面の点からのずれを保つ（`anchor` が 1 なら右・下の縁から、0.5 なら中央から、0 なら左・上の縁からの距離）。折り返しの幅は、本編の画面より狭いと画面の幅に対する割合を保つ。字の大きさと余白は画素のまま。
- web の部品: `data.theme.canvas` に置くコンポジションの画面の大きさを渡す。`side=right` の部品はその幅から右の距離を決める。引数の座標（`x`、`y`）は置くコンポジションの画素で、既定の値は本編の画面の値。
- トランジション: 基準の 1920x1080 の中央を、画面を覆うように縦横同じ倍率で写す（`_prelude.wgsl`）。ショートでは左右が切れ、円は円のまま、どの進み具合でも画面の全部を覆う。

## 書体

パックの書体を、OS に入っていない PC でも、プロジェクトを別の PC に移しても、同じ画素で描く。`web.cef`（ページの CSS）と `text`（cosmic-text と swash、glyphon）の両方が同じファイルを使う。書体の本体（3 書体で 25 MB）はリポジトリに入れず、初めて使うときに配布元から取る。

### 先行事例

| 事例 | 書体の持ち方 | 採ったこと |
|---|---|---|
| After Effects、Premiere、DaVinci Resolve | OS に入った書体を名前で引く。プロジェクトは書体を持たず、無い PC では置き換えて警告する（AE の「ファイルを収集」も書体は集めない） | 無ければ落ちずに置き換える |
| Unity、Godot | 書体のファイルをプロジェクトの資産として取り込み、資産として配る | 書体を素材（`assets`）にする |
| Google Fonts の配布元（`google/fonts`） | OFL の書体を Git のリポジトリで配り、`METADATA.pb` が family の名前を持つ。CSS API はファイルの中の名前によらずその名前で `@font-face` を宣言する（M PLUS Rounded 1c のファイルの中の名前は `Rounded Mplus 1c`） | 配布元のコミットを固定した URL から取る。family の名前を外で決める |
| Homebrew の formula、npm の lockfile（`@fontsource` の書体の包みなど） | 取る先の URL か版と、中身のハッシュ（`sha256`、`integrity`）を並べ、取ったものを照らしてから使う | 目録に URL と BLAKE3 を書き、合わなければ使わない |
| Typst のパッケージ、MiKTeX | 文書が使うものを初めて使うときに取り、ユーザーのキャッシュに置く（Typst は `{cache}/typst/packages`）。2 回目からは取らない | 適用するときに足りない書体だけを取り、キャッシュに置く |
| Remotion | `public/` に書体を置き、`staticFile` の URL を `@font-face` で宣言し、読み込みを待ってから描く（`delayRender`） | `@font-face` で宣言し、読み込みを待つ |
| CEF の scheme handler | 決めた scheme と名前の要求に、アプリが答える（`CefRegisterSchemeHandlerFactory`） | 書体のファイルをホストが配る |

### 新しい設計

- 目録: パックは `fonts/fonts.json` に書体ごとの family の名前、ファイル名、配布元の URL（`google/fonts` の固定のコミット）、BLAKE3 を書く。`fonts/<family>/` にはライセンスの文（`OFL.txt`）だけを置く。family の名前はテーマ（`theme.json` の `fonts`）と CSS が書く名前で、プロジェクトのフォルダーの名前になる。`mm pack check` は本体を取らずに、テーマの書体が目録にあるか、URL が HTTPS か、family ごとにライセンスの文があるかを見る。
- 取得: `mm pack fetch-fonts <パック>` が目録の書体を取り、BLAKE3 を照らして、ユーザーのキャッシュ（Windows は `%LOCALAPPDATA%\movie-maker\fonts\<BLAKE3>\<ファイル>`、`MM_FONT_CACHE` で変えられる）に置く。`mm pack apply` も足りない書体を取る。取るのは OS の `curl` で、HTTPS だけを許す。ハッシュが違えば置かず、配布元に届かなければ残りは取りにいかずに進む。キャッシュの中身も使う前に照らす。
- 取り込み: パックを適用すると、キャッシュの書体とパックのライセンスの文をプロジェクトの `fonts/<family>/` へ写し、まだ無い書体を素材として足す（`addAsset`。場所は `fonts/<family>/<ファイル>` の相対パス、`contentHash` は目録の BLAKE3）。プロジェクトは書体を持つので、フォルダーごと移せば書体も付いていき、移した先で取り直さない。スキーマは変えない（拡張子 `ttf`、`otf`、`ttc` の素材を書体として扱う）。
- 渡し方: レンダラーがフレームごとにプロジェクトの書体の素材を集め、組が変わったときだけ、文字の実装（`TextShaper::use_fonts`、`TextRasterizer::use_fonts`）とノードの種類（`NodeKind::use_fonts`）に渡す。組の指紋（`contentHash` と family の名前）をフレームのキャッシュのキーに入れる。
- `text`: fontdb に読み込み、フォルダーの名前を family の名前として足す。同じ family の OS の書体は外す（PC ごとに入っている版で画素が変わらないように）。`text` の種類に `font`（family の名前）を足し、パックの文字のスタイルは `font` に書体の役（`body` など）を書く。
- `web.cef`: ページの HTML に書体を埋めない（3 書体で 25 MB あり、HTML はレイヤーごとにコピーされる）。ホストは `Prepare` の `fonts` から `@font-face` をページに差し込み、ファイルを `https://mm-font.invalid/<contentHash>` の URL で配る（scheme handler）。CSP の `font-src` と、ホストの要求の決まり（`policy`）は、この起点だけを足して許す。Chromium は使う family と太さのファイルだけを取りに来る。ホストは時刻を反映したあと書体の読み込みを待ち、待ったときは時刻をもう一度反映する（文字の幅を測って枠を組む吹き出しが、置き換えの書体の幅のまま描かないように）。
- 無いとき: 取れなかった書体（オフライン、ハッシュの違い）は取り込まず、プロジェクトの書体のファイルが読めなければ飛ばして、その family は OS の書体で描く。適用も描画も止めない。あとで `mm pack fetch-fonts` で取り、適用し直せば書体が入る。
