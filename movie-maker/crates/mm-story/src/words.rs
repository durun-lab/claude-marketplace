//! 単語の並びの組み立てと、範囲・検索の純粋な計算。

use std::collections::HashSet;

use mm_ingest_api::Transcript;
use mm_store::word_id;
use mm_types::RationalTime;

use crate::model::{StoryHit, StoryWord};

/// 検索結果の文脈に出す文字数。
const CONTEXT_CHARS: usize = 24;

/// 取り込んだままの 1 単語（修正を重ねる前）。
pub(crate) struct BaseWord {
    pub id: String,
    pub segment: u32,
    pub start: RationalTime,
    pub end: RationalTime,
    pub text: String,
}

/// 単語 `b` で終わる範囲の終わり。単語の終わりを、次の単語の始まりまでに詰める
/// （単語の時刻は隣と重なることがあり、隣り合う話題を並べたときに同じ音が 2 回入らないようにする）。
pub fn range_end(words: &[StoryWord], b: usize) -> RationalTime {
    match words.get(b + 1) {
        Some(next) if next.start < words[b].end && next.start > words[b].start => next.start,
        _ => words[b].end,
    }
}

/// 付属ファイルの文字起こしを単語の並びにする。単語の時刻の無い区間は区間を 1 つの単語にする。
/// 単語の ID は `Store::put_transcript` と同じ規則（同じ開始の時刻には `_2`、`_3` を付ける）。
pub(crate) fn sidecar_words(content_hash: &str, t: &Transcript) -> Vec<BaseWord> {
    let mut raw = Vec::new();
    for (segment, seg) in t.segments.iter().enumerate() {
        if seg.words.is_empty() {
            raw.push((segment as u32, seg.start, seg.end, seg.text.clone()));
        }
        raw.extend(
            seg.words
                .iter()
                .map(|w| (segment as u32, w.start, w.end, w.text.clone())),
        );
    }
    let mut seen = HashSet::new();
    raw.into_iter()
        .map(|(segment, start, end, text)| {
            let mut nth = 1;
            while !seen.insert(word_id(content_hash, start, nth)) {
                nth += 1;
            }
            BaseWord {
                id: word_id(content_hash, start, nth).to_string(),
                segment,
                start,
                end,
                text,
            }
        })
        .collect()
}

/// 消していない単語の文字を並べた中から `q` を探す。語が始まる単語ごとに 1 件で、最大 `limit` 件。
pub(crate) fn find_in(asset: &str, words: &[StoryWord], q: &str, limit: usize) -> Vec<StoryHit> {
    let shown: Vec<(usize, Vec<char>)> = words
        .iter()
        .enumerate()
        .filter(|(_, w)| !w.deleted)
        .map(|(i, w)| (i, w.fixed.as_deref().unwrap_or(&w.text).chars().collect()))
        .collect();
    let flat: Vec<char> = shown.iter().flat_map(|(_, c)| c.iter().copied()).collect();
    // 平らな文字列の位置から、その文字がある単語の番号（shown の添字）へ。
    let owner: Vec<usize> = shown
        .iter()
        .enumerate()
        .flat_map(|(k, (_, c))| std::iter::repeat_n(k, c.len()))
        .collect();
    let needle: Vec<char> = q.chars().collect();
    let mut hits = Vec::new();
    let mut last_start_word = None;
    for at in 0..flat.len().saturating_sub(needle.len() - 1) {
        if hits.len() >= limit {
            break;
        }
        if flat[at..at + needle.len()] != needle[..] || last_start_word == Some(owner[at]) {
            continue;
        }
        last_start_word = Some(owner[at]);
        let first = &words[shown[owner[at]].0];
        let last = &words[shown[owner[at + needle.len() - 1]].0];
        hits.push(StoryHit {
            asset: asset.to_owned(),
            word: first.id.clone(),
            start: first.start,
            end: last.end,
            context: flat[at..].iter().take(CONTEXT_CHARS).collect(),
        });
    }
    hits
}
