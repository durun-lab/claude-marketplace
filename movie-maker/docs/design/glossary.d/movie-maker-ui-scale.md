## 再生と Timeline

| 用語 | 定義 | 識別子 |
|---|---|---|
| 論理ピクセル | 表示の倍率に依らない UI の長さの単位。WebView の CSS の px と同じ大きさで、サーフェスの割り付けと Timeline の寸法と当たり判定はこれで持つ | `Scale::to_logical`、`ROW_H`、`HANDLE_PX` |
| 物理ピクセル | サーフェスのテクスチャとウィンドウのクライアント座標の画素。論理ピクセルに表示の倍率を掛けたもの。サーフェスと WebView の矩形、ポインターの入力、`Ui` の外とやりとりする座標はこれ | `Scale::rect_to_physical`、`split_rects`、`Ui::layout` |
