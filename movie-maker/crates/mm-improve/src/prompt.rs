//! 回に頼む文と、最後の発言から済んだ要望と返事を読む所。

use crate::store::{FeedbackItem, FeedbackKind, FeedbackStore, Outcome};

/// 最後の発言で、済んだ要望の並びの前に置く完了のマーカー。
pub const DONE_MARK: &str = "対応した要望:";
/// 1 件の要望の本文を頼む文に載せる上限（文字）。全文はファイルにある。
const ITEM_CHARS: usize = 2000;

/// 完了のマーカーを書かずにターンを終えたときに続きを頼む文。
pub const NUDGE_PROMPT: &str = "回の途中でターンが終わりました。バックグラウンドに移したコマンドがあれば、その結果を読んで作業を続けてください。ターンを終えると回も終わるので、テストなどの長いコマンドは終わるまで待ってください。回を終えたら、最後に「対応した要望:」の行と、対応を終えた要望を 1 行に 1 つずつ「- <ID>: <返事>」の形で書いて止まってください。";

/// 使用制限が明けたときに続きを頼む文。止まったことは伝えず、人が続きを頼むときと同じ短い文にする。
pub const CONTINUE_PROMPT: &str = "続けて";

/// 頼む文に添える回の事情。
pub struct PromptContext<'a> {
    /// 書いて取り込んでよい所（areas.toml の `[improve] merge`）。
    pub merge: &'a [String],
    /// 要望のファイルの置き場所。
    pub store: &'a FeedbackStore,
}

fn item_block(item: &FeedbackItem, store: &FeedbackStore) -> String {
    let text: String = item.text.chars().take(ITEM_CHARS).collect();
    let mut s = format!(
        "### {}（{}）\nファイル: {}\n{text}\n",
        item.id,
        kind_label(item),
        store.path_of(item).display()
    );
    for r in &item.replies {
        if let Outcome::Rejected { reason } = &r.outcome {
            s.push_str(&format!(
                "- 前の回 {} は取り込めなかった: {reason}\n",
                r.run_id
            ));
        }
    }
    s
}

fn kind_label(item: &FeedbackItem) -> &'static str {
    match item.kind {
        FeedbackKind::Feedback => "人からの要望",
        FeedbackKind::Error => "エラーからの自動の起票",
        FeedbackKind::Slow => "遅い処理からの自動の起票",
    }
}

/// 回に頼む文。
pub fn prompt_for(items: &[FeedbackItem], cx: &PromptContext<'_>) -> String {
    let blocks: Vec<String> = items.iter().map(|i| item_block(i, cx.store)).collect();
    let merge = if cx.merge.is_empty() {
        "（なし）".to_owned()
    } else {
        cx.merge.join("、")
    };
    format!(
        "[movie-maker の自動の改善] 要望が {n} 件たまっています。全体を見て作業の計画を立て、1 つの回として実装してください。\n\
\n\
## 要望（古い順）\n\
{blocks}\n\
## 回の決まり\n\
- 作業フォルダーはこの worktree です。ほかのフォルダーのファイルを変えないでください。要望のファイルは読むだけにしてください。\n\
- まず全体を読み、似た作業や同じ場所を触る要望をまとめて、この回で対応する要望と順番を決めてください。一度に仕上げられる量にとどめ、残りは次の回に回してください。\n\
- 書いて取り込める所は次のとおりです: {merge}。これ以外のファイルを変えると、取り込まずに PR までで止まり、人の承認を待ちます。\n\
- 確かめはフィクスチャ（fixtures/ のプロジェクト）で行ってください。人の実際のプロジェクトを開かないでください。費用のかかる操作（gen.ai など）を呼ばないでください。\n\
- 変えたクレートで cargo check と cargo nextest run を通してください。画面を変えたら WebView の型の検査も通してください。\n\
- コミット、push、PR の作成はしないでください。回の終わりに、変更を 1 つのコミットにまとめて検査します。\n\
- 終えたら、何をどう変えたかを 2、3 行で書いてください。最後に「{DONE_MARK}」の行を置き、続けて対応を終えた要望を 1 行に 1 つずつ「- <ID>: <その要望への返事を 1、2 行>」の形で書いて止まってください。直さなくてよい要望も、理由を返事に書けば対応を終えたものにできます。",
        n = items.len(),
        blocks = blocks.join("\n"),
    )
}

/// 最後の発言から、済んだ要望と返事を読む。完了のマーカーより後の行に ID がある要望を済んだものとみなす。
/// 返事は ID の後ろの文。無ければ完了のマーカーより前の文（何をしたか）を返事にする。
pub fn parse_done(message: &str, ids: &[String]) -> Vec<(String, String)> {
    let Some(at) = message.rfind(DONE_MARK) else {
        return Vec::new();
    };
    let summary = message[..at].trim();
    let tail = &message[at + DONE_MARK.len()..];
    let mut out = Vec::new();
    for id in ids {
        let Some(line) = tail.lines().find(|l| l.contains(id.as_str())) else {
            continue;
        };
        let after = line.split_once(id.as_str()).map(|(_, a)| a).unwrap_or("");
        // ID を囲む閉じの「`」だけを外す。返事の先頭がコードの「`」で始まることがある。
        let after = after.strip_prefix('`').unwrap_or(after);
        let reply = after.trim_start_matches(|c: char| c == ':' || c == '：' || c.is_whitespace());
        let reply = if reply.trim().is_empty() {
            summary
        } else {
            reply.trim()
        };
        out.push((id.clone(), reply.to_owned()));
    }
    out
}

/// 回のコミットのメッセージ。
pub fn commit_message(run_id: &str, done: &[(String, String)]) -> String {
    let mut s = format!(
        "fix(movie-maker): 自動の改善の回 {run_id}（要望 {} 件）\n\n",
        done.len()
    );
    for (id, reply) in done {
        let first = reply.lines().next().unwrap_or("");
        s.push_str(&format!("- {id}: {first}\n"));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_done_reads_replies_after_mark() {
        let ids = ["a-1", "b-2", "c-3", "d-4"].map(str::to_owned).to_vec();
        let msg = "ボタンの文言を直した。\n\n対応した要望:\n- a-1: 文言を直しました\n- `b-2`\n- `d-4`: `src/a.txt` を直した\n";
        let done = parse_done(msg, &ids);
        assert_eq!(done.len(), 3);
        assert_eq!(done[0], ("a-1".into(), "文言を直しました".into()));
        assert_eq!(done[1], ("b-2".into(), "ボタンの文言を直した。".into()));
        assert_eq!(done[2], ("d-4".into(), "`src/a.txt` を直した".into()));
    }

    #[test]
    fn parse_done_without_mark_is_empty() {
        assert!(parse_done("a-1 を直した", &["a-1".to_owned()]).is_empty());
    }
}
