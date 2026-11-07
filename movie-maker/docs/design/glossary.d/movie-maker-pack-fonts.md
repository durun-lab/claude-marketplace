## 書体

| 用語 | 定義 | 識別子 |
|---|---|---|
| 書体の素材 | プロジェクトの素材のうち、拡張子が `ttf`、`otf`、`ttc` のもの。描く側が読み込み、同じ family の OS の書体より優先する | `FontFile`、`use_fonts` |
| family のフォルダー | 書体を置く `fonts/<family>/`。フォルダーの名前を family の名前として登録し、テーマと CSS はこの名前で書く | `FONTS_DIR`、`declared_family` |
| 書体の起点 | CEF のホストが書体のファイルを配る URL の起点。ページの CSP とホストの要求の決まりは、この起点だけを足して許す | `FONT_ORIGIN` |
| 書体の目録 | パックの `fonts/fonts.json`。書体ごとの family の名前、ファイル名、配布元の URL（固定のコミット）、BLAKE3 を持つ。本体はリポジトリに入れない | `PackFont`、`mm pack fetch-fonts` |
| 書体のキャッシュ | 目録の書体を取って照らしたものを置く、ユーザーごとのフォルダー（`<キャッシュ>/<BLAKE3>/<ファイル>`）。適用するとここからプロジェクトに写す | `FontCache`、`MM_FONT_CACHE` |
