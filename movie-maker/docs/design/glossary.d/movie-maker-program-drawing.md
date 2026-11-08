## 再生と Timeline

| 用語 | 定義 | 識別子 |
|---|---|---|
| 描き込み | コメントに付けた線、矩形、点。座標はフレームの左上を (0, 0)、右下を (1, 1) に正規化する。デスクトップは Program の映像の矩形に重ね、サーフェスにだけ描く（フレームのキャッシュと書き出しには入らない） | `Comment.drawing`、`Shape`（mm-store）、`DrawingShape`（mm-ui-layout）、`DrawingLayer` |
| 時刻のコメント / 範囲のコメント | `at` を持つコメント / `range`（`in` から `out` まで。`out` は含まない）を持つコメント。描き込みは Program に映るフレームで比べ、時刻のコメントは止めている間だけ、範囲のコメントは再生中も出す | `Comment.at`、`Comment.range`、`review::drawing::shows_at` |
| 選んだコメント（Review の） | Review で時刻を押したコメント。その位置ではその描き込みだけを Program に重ねる。選択はパネルの中にだけあり、パネルを開いたときと閉じたときに外す | `review_comment_focus`、`ReviewApi.focusComment`、`DrawingSlot::focus` |
| 映像の矩形 | Program の領域の中で、縦横比を保って映像が映る矩形。レターボックスの余白を除く | `fit_rect`、`frame_point` |
| 表示の倍率 | Windows の DPI を 96 で割った値。1 論理ピクセルあたりの物理ピクセルの数で、サーフェスの UI（Timeline、描き込み）を論理ピクセルで揃えるのに掛ける | `surface::scale_factor`、`Ui::set_scale_factor`、`Scale`（mm-ui-layout） |

## 置き換えた語

### 段階 9（Program の描き込み）

| 元の語 | 文脈 | 正式な用語 |
|---|---|---|
| 印 | 描き込みの点 / Review で選んだカード | 点 / 選んだコメント |
| 控え | 描画スレッドが最後に `Ui` に渡した描き込み | 最後に渡したコメント |
| 写す | 0〜1 の座標を映像の矩形の点へ | 変換する |
| 落とす | 描き込みを線分の並びに | 分ける |
| `Mark`、`MarkKind`、`MarkSegment`、`MarkRenderer`、`mark_segments`、`palette::MARK`、`marks.rs` | 描き込みの型と関数。Source の `bins.marks`（イン点・アウト点）や `addMarker`（マーカー）と紛れる | `DrawingShape`、`DrawingShapeKind`、`DrawingSegment`、`DrawingRenderer`、`drawing_segments`、`palette::DRAWING`、`drawing.rs` |
