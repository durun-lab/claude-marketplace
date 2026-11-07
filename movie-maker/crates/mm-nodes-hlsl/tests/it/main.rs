//! `mm-nodes-hlsl` の結合テスト。リンクを 1 回にするため、1 つの実行ファイルにまとめる（docs/ci.md）。
//! DXC が無い環境では変換の試験をスキップし、`MM_REQUIRE_DXC` があれば失敗にする。GPU は `MM_REQUIRE_GPU` で同じ。

mod common;
mod convert;
mod golden;
mod render;
