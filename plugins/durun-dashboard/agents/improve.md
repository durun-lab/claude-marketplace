---
name: improve
description: durun-dashboard のワークフロー「改善要望の対応」の指示。ダッシュボードに届いた改善要望を 1 つのウェーブとして実装するときに使う
model: opus
effort: medium
---

全体を見て作業計画を立て、1 つのウェーブとして実装してください。
- 要望のファイルの text が要望で、targets は人間が画面上で指した要素です。selector のクラス名から、描画しているコンポーネントを探せます。context は送ったときの画面の状態です。context.snapshot は大きいので、必要なところだけ読んでください。
- まず全体を読み、似た作業や同じ場所を触る要望をまとめて、このウェーブで対応する要望と順番を決めてください。一度に仕上げられる量にとどめ、残りは次のウェーブに回してください。
- ダッシュボードは起動済みで、local/dashboard のコードが変わると画面とサーバーに自動で反映されます。pnpm start は実行しないでください。
- ダッシュボードは、このセッションの作業ディレクトリ（手元専用のブランチ local/dashboard の durun-dashboard）で動いています。ここには並行するセッションの書きかけがあるので、このチェックアウトでは実装せず、書きかけにも触らないでください。
- 実装を始める前に git fetch origin main を実行し、git worktree add <リポジトリのルート>/.worktrees/improve-<日時> -b fix/dashboard-improve-<日時> origin/main で worktree を作ってください。worktree の durun-dashboard で pnpm install を実行してから、そこで実装します。Bash の作業ディレクトリはコールをまたいで保たれないので、毎回 cd <絶対パス> && を付けるか git -C を使ってください。
- まとまりごとに worktree で pnpm check と pnpm test を通し、1 つのコミットにしてください。コードレビューは依頼せず、そのまま次のまとまりへ進んでください。
- main は直接 push できません。すべてのまとまりを終えたら、ブランチを push -u し、PR を作って gh pr merge --auto でマージキューに載せてください。
- 続けて、作業ディレクトリで local/dashboard にいることを確かめ、git merge --no-edit fix/dashboard-improve-<日時> でこのウェーブを取り込んでください。衝突したら両方の意図を残して解消してください。local/dashboard は push しないでください。取り込んだら git worktree remove で worktree を片付けてください。
- 判断に迷う点は ask_human で人間に聞いてください。
- 終えたら、まとまりごとに何をどう変えたかを 2、3 行で書いてください。
- 最後に「対応した要望:」の行を置き、続けて対応を終えた要望のファイル名を 1 行に 1 つずつ書いて止まってください。残りの要望は、次のウェーブで計画し直します。
