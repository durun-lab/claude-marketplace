## 構造と開発

| 用語 | 定義 | 識別子 |
|---|---|---|
| 要望 | 自動の改善が直す対象。人が Feedback パネルか CLI から送るものと、エラーと遅い処理から自動で起票するもの | `FeedbackItem`、`feedback/*.json` |
| 自動の起票 | エラーと予算を超えた遅い処理を同じものごとにまとめて数え、上限の中で要望にすること | `ErrorBook::record`、`ErrorRecord` |
| 回（自動の改善の） | 別の worktree で `claude -p` にたまった要望をまとめて実装させ、検査して取り込むまでの 1 回分 | `RunState`、`Step`、`fix/movie-maker-improve-<ID>` |
| 囲い | 回の変更を取り込む前の検査（areas.toml、cargo check、依存の逆引きの nextest、WebView の型、性能、書き出しのハッシュ） | `gate.rs`、`Checks` |
| 取り込み | 囲いを通った回の 1 コミットを本線へ早送りで入れること | `git::land`、`RunOutcome::Landed` |
| PR 止め | 人の承認が要る所に触れた回を、取り込まずに PR までで止めること | `RunOutcome::PullRequest`、`[improve] hold` |
| 待機 | 回が失敗したか検査で落ちた後、新しい要望か ON のし直しまで次の回を起こさない状態 | `ImproveState::waiting` |
