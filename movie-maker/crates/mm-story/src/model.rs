//! Story の読み出しが返す型。画面に依らず、JSON にそのまま出せる。

use mm_doc::{Selection, Topic};
use mm_types::RationalTime;
use serde::Serialize;

/// 一覧に出す素材 1 つ。
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct StoryAsset {
    pub id: String,
    pub name: String,
    pub duration: f64,
    pub has_transcript: bool,
}

/// シーケンス 1 つの概要。
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct StorySequence {
    pub id: String,
    pub cuts: usize,
    pub duration: f64,
}

/// Story を開いたときに要る一覧。
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Overview {
    pub assets: Vec<StoryAsset>,
    pub sequences: Vec<StorySequence>,
    /// いま開いているコンポジションの、いちばん下の sequence のレイヤーが置く sequence。無ければ `None`。
    /// Story の Delete（`cutWords`）が切る先。
    pub open_sequence: Option<String>,
}

/// 文字起こしの 1 単語。時刻は素材の時間軸。
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct StoryWord {
    pub id: String,
    pub segment: u32,
    pub start: RationalTime,
    pub end: RationalTime,
    /// 取り込んだままの文字。
    pub text: String,
    /// 直した文字。
    pub fixed: Option<String>,
    pub deleted: bool,
}

/// 素材の文字起こしと、その素材の話題・選別。
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct StoryTranscript {
    pub asset: String,
    pub words: Vec<StoryWord>,
    pub topics: Vec<Topic>,
    pub selections: Vec<Selection>,
}

/// 単語の範囲の時刻（素材の時間軸）。
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct WordRange {
    pub start: RationalTime,
    pub end: RationalTime,
}

/// 検索の 1 件。
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct StoryHit {
    pub asset: String,
    /// 語が始まる単語。
    pub word: String,
    pub start: RationalTime,
    /// 語が終わる単語の終わり。
    pub end: RationalTime,
    pub context: String,
}
