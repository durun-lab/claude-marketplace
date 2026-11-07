//! 自動の起票。panic、例外、ホストのクラッシュ、予算を超えた遅い処理を集めて要望にする（durun-dashboard の `errors.ts` の型）。
//! 大量の記録で人の要望が埋もれないよう、同じものは 1 件にまとめて数え、未対応はエラーと遅い処理でそれぞれ少数に抑える。

use std::collections::{BTreeMap, HashSet};
use std::path::Path;

use mm_types::Result;
use serde::{Deserialize, Serialize};

use crate::files::{read_or_default, write_json};
use crate::store::{FeedbackKind, FeedbackStore, NewFeedback};
use crate::time::human;

/// 未対応の自動の要望の上限。エラーと遅い処理で別々に数える。
pub const MAX_PENDING: usize = 3;
/// 同じものの起票の上限。直しても再発し続けるもので、同じ要望を繰り返し起票しない。
pub const MAX_FILINGS: u32 = 2;
/// 1 時間あたりの自動の起票の上限〔推〕。
pub const MAX_FILINGS_PER_HOUR: usize = 6;
const HOUR_MS: u64 = 3_600_000;
/// 取り込みからこの時間のうちに初めて起きたものは、その回の疑いとして結びつける〔推〕。
pub const SUSPECT_WINDOW_MS: u64 = 30 * 60 * 1000;
const MAX_RECORDS: usize = 100;
const MAX_MESSAGE_CHARS: usize = 500;
const STACK_LINES: usize = 12;

const ERROR_REQUEST: &str = "movie-maker で想定外のエラーが起きています。原因を調べて直してください。直さなくてよいエラーなら、理由を書いて止まってください。";
const SLOW_REQUEST: &str = "movie-maker で、予算より時間のかかった処理があります。詳しくの trace で時間のかかっているところを調べて速くしてください。予算が合っていなければ予算を直す提案を書き、直さなくてよいなら理由を書いて止まってください。";

/// 起きた所。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ErrorSource {
    /// アプリ（Tauri の Rust 側）。
    App,
    /// `mm` コマンド。
    Cli,
    /// アプリの WebView の画面。
    Webview,
    /// CEF のホスト。
    HostCef,
    /// OFX のホスト。
    HostOfx,
    /// 音のホスト。
    HostAudio,
}

impl ErrorSource {
    fn label(self) -> &'static str {
        match self {
            Self::App => "アプリ",
            Self::Cli => "mm コマンド",
            Self::Webview => "画面",
            Self::HostCef => "CEF のホスト",
            Self::HostOfx => "OFX のホスト",
            Self::HostAudio => "音のホスト",
        }
    }
}

/// 最長だったときの負荷。マシンが混んでいたせいで遅かったのかを、要望を読んですぐ見分けるために添える。
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Load {
    /// CPU の使用率（%）か load average。
    pub cpu: f64,
    /// GPU の使用率（%）。
    pub gpu: Option<f64>,
}

/// 記録する 1 回。
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ErrorInput {
    /// 起きた所。
    pub source: ErrorSource,
    /// メッセージ。数字と ID は同じものをまとめるキーでは伏せる。
    pub message: String,
    /// スタック。
    #[serde(default)]
    pub stack: Option<String>,
    /// 場所（モジュールやパネルの名前）。
    #[serde(default, rename = "where")]
    pub where_: Option<String>,
    /// スパンの名前（`render.frame` など）。
    #[serde(default)]
    pub span: Option<String>,
    /// 遅い処理ならかかった時間（ミリ秒）。エラーなら `None`。
    #[serde(default)]
    pub ms: Option<f64>,
    /// 予算（ミリ秒）。
    #[serde(default)]
    pub budget_ms: Option<f64>,
    /// そのときの負荷。
    #[serde(default)]
    pub load: Option<Load>,
    /// 詳しくの trace のパス。
    #[serde(default)]
    pub trace: Option<String>,
}

impl ErrorInput {
    /// エラーの 1 回。
    pub fn error(source: ErrorSource, message: impl Into<String>) -> Self {
        Self {
            source,
            message: message.into(),
            stack: None,
            where_: None,
            span: None,
            ms: None,
            budget_ms: None,
            load: None,
            trace: None,
        }
    }

    /// 遅い処理の 1 回。
    pub fn slow(source: ErrorSource, span: impl Into<String>, ms: f64, budget_ms: f64) -> Self {
        let span = span.into();
        Self {
            message: format!("{span} が予算 {budget_ms} ms を超えた"),
            span: Some(span),
            ms: Some(ms),
            budget_ms: Some(budget_ms),
            ..Self::error(source, "")
        }
    }
}

/// 同じものをまとめた記録（durun-dashboard の `ErrorRecord` の形）。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ErrorRecord {
    /// 起きた所。
    pub source: ErrorSource,
    /// 最後の回のメッセージ。
    pub message: String,
    /// 最後に分かったスタック。
    pub stack: Option<String>,
    /// 場所。
    #[serde(rename = "where")]
    pub where_: Option<String>,
    /// スパンの名前。
    pub span: Option<String>,
    /// 対応を終えてから数え直した回数。
    pub count: u32,
    /// 最初の回（ミリ秒）。
    pub first_at: u64,
    /// 最後の回（ミリ秒）。
    pub last_at: u64,
    /// 遅い処理の最長（ミリ秒）。エラーなら `None`。
    pub slowest_ms: Option<f64>,
    /// 最長だったときの負荷。
    pub slowest_load: Option<Load>,
    /// 予算（ミリ秒）。
    pub budget_ms: Option<f64>,
    /// 詳しくの trace のパス。
    pub trace: Option<String>,
    /// 起票した回数。
    pub filed: u32,
    /// 最後に起票した要望の ID。
    pub item: Option<String>,
    /// 疑いのある回の ID（取り込みの直後に初めて起きたもの）。
    pub suspect_run: Option<String>,
}

impl ErrorRecord {
    fn is_slow(&self) -> bool {
        self.budget_ms.is_some()
    }
}

/// `errors.json` の中身。
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ErrorBook {
    /// キーごとの記録。
    pub records: BTreeMap<String, ErrorRecord>,
    /// 自動の起票の時刻（直近 1 時間）。
    pub filed_at: Vec<u64>,
}

/// 直前に取り込んだ回。疑いを結びつけるのに使う。
#[derive(Clone, Debug)]
pub struct RecentLanding<'a> {
    /// 回の ID。
    pub run_id: &'a str,
    /// 取り込んだ時刻（ミリ秒）。
    pub at: u64,
}

/// 同じものをまとめるキー。場所 + スパンの名前 + メッセージから数字と ID を伏せたもの。
pub fn key_of(input: &ErrorInput) -> String {
    let src = serde_json::to_value(input.source)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_default();
    let where_ = input.where_.as_deref().unwrap_or("");
    let span = input.span.as_deref().unwrap_or("");
    format!("{src}|{where_}|{span}|{}", mask(&input.message))
}

/// 英数字の語のうち、数字を含むものと 8 桁以上の 16 進を `#` に伏せる。
fn mask(message: &str) -> String {
    let mut out = String::new();
    let mut word = String::new();
    let flush = |word: &mut String, out: &mut String| {
        let hexish = word.len() >= 8 && word.chars().all(|c| c.is_ascii_hexdigit());
        out.push_str(if word.chars().any(|c| c.is_ascii_digit()) || hexish {
            "#"
        } else {
            word
        });
        word.clear();
    };
    for c in message.chars() {
        if c.is_alphanumeric() || c == '_' {
            word.push(c);
        } else {
            flush(&mut word, &mut out);
            out.push(c);
        }
    }
    flush(&mut word, &mut out);
    out
}

impl ErrorBook {
    /// 読む。
    pub fn load(path: &Path) -> Result<Self> {
        read_or_default(path)
    }

    /// 書く。
    pub fn save(&self, path: &Path) -> Result<()> {
        write_json(path, self)
    }

    /// 記録して、起票の条件を満たせば要望にする。起票した要望の ID を返す。
    pub fn record(
        &mut self,
        input: &ErrorInput,
        store: &FeedbackStore,
        now: u64,
        landing: Option<RecentLanding<'_>>,
    ) -> Result<Option<String>> {
        let key = key_of(input);
        let pending: HashSet<String> = store.pending()?.into_iter().map(|i| i.id).collect();
        let slow = input.budget_ms.is_some();
        let open = self
            .records
            .values()
            .filter(|r| r.is_slow() == slow && r.item.as_ref().is_some_and(|i| pending.contains(i)))
            .count();
        self.filed_at.retain(|t| now.saturating_sub(*t) < HOUR_MS);
        let hour_full = self.filed_at.len() >= MAX_FILINGS_PER_HOUR;
        let r = update(&mut self.records, key, input, now, landing);
        let already = r.item.as_ref().is_some_and(|i| pending.contains(i));
        let filed = if !already && r.filed < MAX_FILINGS && open < MAX_PENDING && !hour_full {
            let kind = if slow {
                FeedbackKind::Slow
            } else {
                FeedbackKind::Error
            };
            let context = serde_json::json!({ "error": r });
            let item = store.add(
                kind,
                NewFeedback {
                    text: text_of(r),
                    targets: Vec::new(),
                    context,
                },
            )?;
            r.item = Some(item.id.clone());
            r.filed += 1;
            self.filed_at.push(now);
            Some(item.id)
        } else {
            None
        };
        self.prune();
        Ok(filed)
    }

    /// 要望が済んだので、その要望を起票した記録の回数と最長を数え直す。再発して起票し直す要望に、直す前の回を混ぜない。
    pub fn reset_item(&mut self, item: &str) {
        for r in self
            .records
            .values_mut()
            .filter(|r| r.item.as_deref() == Some(item))
        {
            r.count = 0;
            r.slowest_ms = None;
            r.slowest_load = None;
        }
    }

    fn prune(&mut self) {
        if self.records.len() <= MAX_RECORDS {
            return;
        }
        let mut keys: Vec<(u64, String)> = self
            .records
            .iter()
            .map(|(k, r)| (r.last_at, k.clone()))
            .collect();
        keys.sort();
        let drop = keys.len() - MAX_RECORDS;
        for (_, k) in keys.into_iter().take(drop) {
            self.records.remove(&k);
        }
    }
}

/// 記録を更新する。対応を終えた要望の記録は `reset_item` が回数を 0 に戻すので、次の回から数え直す。
fn update<'a>(
    records: &'a mut BTreeMap<String, ErrorRecord>,
    key: String,
    input: &ErrorInput,
    now: u64,
    landing: Option<RecentLanding<'_>>,
) -> &'a mut ErrorRecord {
    let message: String = input.message.chars().take(MAX_MESSAGE_CHARS).collect();
    let fresh = !records.contains_key(&key);
    let r = records.entry(key).or_insert_with(|| ErrorRecord {
        source: input.source,
        message: message.clone(),
        stack: None,
        where_: None,
        span: None,
        count: 0,
        first_at: now,
        last_at: now,
        slowest_ms: None,
        slowest_load: None,
        budget_ms: None,
        trace: None,
        filed: 0,
        item: None,
        suspect_run: None,
    });
    if r.count == 0 {
        r.first_at = now;
    }
    r.message = message;
    r.count += 1;
    r.last_at = now;
    r.stack = input.stack.clone().or(r.stack.take());
    r.where_ = input.where_.clone().or(r.where_.take());
    r.span = input.span.clone().or(r.span.take());
    r.budget_ms = input.budget_ms.or(r.budget_ms);
    r.trace = input.trace.clone().or(r.trace.take());
    if let Some(ms) = input.ms.filter(|ms| *ms >= r.slowest_ms.unwrap_or(0.0)) {
        r.slowest_ms = Some(ms);
        r.slowest_load = input.load;
    }
    if let Some(l) = landing.filter(|l| fresh && now.saturating_sub(l.at) < SUSPECT_WINDOW_MS) {
        r.suspect_run = Some(l.run_id.to_owned());
    }
    r
}

/// 要望の本文。回数、最長、負荷、trace のパスと `mm trace query` の例を添える。
fn text_of(r: &ErrorRecord) -> String {
    let request = if r.is_slow() {
        SLOW_REQUEST
    } else {
        ERROR_REQUEST
    };
    let mut lines = vec![request.to_owned()];
    lines.extend(what_lines(r));
    lines.extend(trace_lines(r));
    if let Some(run) = &r.suspect_run {
        lines.push(format!(
            "- 自動の改善の回 {run} を取り込んだ直後に初めて起きた。その回の疑いがある。"
        ));
    }
    if r.filed > 0 {
        let prev = r.item.as_deref().unwrap_or("?");
        lines.push(format!("- 前回の要望（{prev}）に対応した後も起きている。"));
    }
    if let Some(stack) = &r.stack {
        lines.push(String::new());
        lines.extend(stack.lines().take(STACK_LINES).map(str::to_owned));
    }
    lines.join("\n")
}

/// 起きた所、何が、回数と最長、最長だったときの負荷。
fn what_lines(r: &ErrorRecord) -> Vec<String> {
    let mut lines = Vec::new();
    let where_ = r
        .where_
        .as_ref()
        .map(|w| format!("（{w}）"))
        .unwrap_or_default();
    lines.push(format!("- 起きた所: {}{where_}", r.source.label()));
    if let Some(span) = &r.span {
        lines.push(format!("- スパン: {span}"));
    }
    let what = if r.is_slow() {
        "遅い処理"
    } else {
        "エラー"
    };
    lines.push(format!("- {what}: {}", r.message));
    let slowest = match (r.slowest_ms, r.budget_ms) {
        (Some(ms), Some(b)) => format!("最長 {ms:.1} ms（予算 {b} ms）、"),
        _ => String::new(),
    };
    lines.push(format!(
        "- 回数: {} 回（{slowest}最初 {}、最後 {}）",
        r.count,
        human(r.first_at),
        human(r.last_at)
    ));
    if let Some(l) = r.slowest_load {
        let gpu = l.gpu.map(|g| format!("、GPU {g:.0}%")).unwrap_or_default();
        lines.push(format!("- 最長だったときの負荷: CPU {:.1}{gpu}", l.cpu));
    }
    lines
}

fn trace_lines(r: &ErrorRecord) -> Vec<String> {
    let Some(trace) = &r.trace else {
        return Vec::new();
    };
    let filter = r
        .span
        .as_ref()
        .map(|s| format!(" WHERE name = '{}'", s.replace('\'', "''")))
        .unwrap_or_default();
    vec![
        format!("- 詳しくの trace: {trace}"),
        format!(
            "- 見るための例: mm trace query \"{trace}\" \"SELECT name, dur / 1e6 AS ms FROM slice{filter} ORDER BY dur DESC LIMIT 20\""
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mask_hides_numbers_and_ids() {
        assert_eq!(
            mask("frame 120 of clip c7f3a9e2 failed: abcdefab"),
            "frame # of clip # failed: #"
        );
    }

    #[test]
    fn key_ignores_numbers_but_not_span() {
        let a = ErrorInput::slow(ErrorSource::App, "render.frame", 30.0, 16.0);
        let mut b = ErrorInput::slow(ErrorSource::App, "render.frame", 40.0, 16.0);
        b.message = a.message.replace("16", "17");
        assert_eq!(key_of(&a), key_of(&b));
        let c = ErrorInput::slow(ErrorSource::App, "decode.frame", 30.0, 16.0);
        assert_ne!(key_of(&a), key_of(&c));
    }
}
