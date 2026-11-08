## 書き出しと納品

| 用語 | 定義 | 識別子 |
|---|---|---|
| 描画の工程 | 1 つのレンダラーの書き出しで、`Engine` を持ってフレームを描き、リードバックを先行させて受け取る側。フレームを 1 つ描くごとに `Engine` を呼び出し側に渡せる | `stages::Draw`、`Renderer::export_frame`、`export_run_interleaved` |
| エンコードの工程 | 1 つのレンダラーの書き出しで、`Engine` を持たない別のスレッドで、受け取ったフレームをエンコーダーに送る側 | `stages::encode_stage` |
| 受け渡しのキュー | 描画の工程とエンコードの工程の間の、長さに上限のあるフレームのキュー。満ちたら描画の工程が待つ。Deliver のキュー（書き出しの）とは別 | `stages::QUEUE_FRAMES` |
| 背圧 | 受け渡しのキューが満ちたら上流（描画の工程）を待たせ、溜まるフレームの数を抑えること（GStreamer の `queue` の `max-size-buffers`） | `stages::QUEUE_FRAMES` |
| 終端（受け渡しの） | 描画の工程が区間の全部を渡し終えたことを伝える受け渡しの要素（GStreamer の EOS）。これが来ずに受け渡しのキューが閉じたら、描画の工程が止まった | `stages::Handoff::End` |
| フレームの間の割り込み | 描画の工程がフレームを 1 つ描くごとに `Engine` を呼び出し側に渡し、配信の急ぎの仕事（静止画のフレーム、`GET /frame`、キャッシュの破棄）を済ませること | `export_run_interleaved` の `between`、`Worker::run_urgent` |
