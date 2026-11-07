//! モバイルから即時に適用してよい操作。`docs/design/12-mobile.md` の表と同じ内容で、
//! 人の待ち受け（`mm-serve`）が `POST /m/api/ops` で断る・通すの判定に使う。
//! 表に操作を足すときは、この一覧と設計書を同じ PR で変える。

/// Story で切る・並べる操作と目印に限る。
pub const MOBILE_OPS: &[&str] = &[
    // 話題と選別
    "addTopic",
    "removeTopic",
    "addSelection",
    "removeSelection",
    "rate",
    "tag",
    // 文字起こし
    "deleteWords",
    "fixTranscript",
    "cutWords",
    // 並べる
    "insertTopics",
    // 目印
    "addMarker",
    "removeMarker",
];

/// 操作の名前（JSON の `op` の値）がモバイルから即時に適用してよいものか。
pub fn is_mobile_op(name: &str) -> bool {
    MOBILE_OPS.contains(&name)
}
