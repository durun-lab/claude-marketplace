//! Story の読み出し。素材の文字起こし（単語の列、話題、選別）、単語の範囲の時刻、全文検索を、
//! 画面に依らない形で組み立てる。アプリの Story パネルと、モバイル向けの HTTP が同じ型を返す。
//!
//! 単語の本文は `project.mmdb` の `Store::transcript_words` があればそれを、無ければ取り込みの付属ファイル
//! （`cache/ingest/<hash>.transcript.json`）を読む。消した単語・直した文字は、呼び出し側が渡す
//! プロジェクトのデータの `transcript_edits` を重ねる。
//!
//! 読み出しは [`StoryReader`] が持つ。プロジェクトのデータは呼び出しごとに渡す（アプリは再生スレッドの最新、
//! HTTP は保存済みの版を渡す）。操作の適用は持たず、`mm-ops` の操作を使う。モバイルから即時に適用してよい
//! 操作の一覧は [`MOBILE_OPS`]。

mod mobile_ops;
mod model;
mod reader;
mod words;

pub use mobile_ops::{is_mobile_op, MOBILE_OPS};
pub use model::{
    Overview, StoryAsset, StoryHit, StorySequence, StoryTranscript, StoryWord, WordRange,
};
pub use reader::{layer_sequence, StoryReader, QUERY_MAX_CHARS};
pub use words::range_end;
