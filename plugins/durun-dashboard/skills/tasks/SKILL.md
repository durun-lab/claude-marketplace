---
name: tasks
description: 手順が 3 つ以上ある作業や、PR・JIRA の課題を進めるときに、durun-dashboard のタスクを登録して進み具合を人間に見せる
---

人間はダッシュボードでタスクの進み具合を見ています。タスクは durun-dashboard の MCP のツールで扱います。

- 取りかかる前に `create_tasks` で登録する。subject は 30 文字以内。人間が開いて読む説明は description に書く。
- 取りかかるときに `update_task` で `in_progress`、終えたら `completed` にする。要らなくなったら `deleted`。
- PR・Issue・JIRA の課題そのものを進めるときは、`ticket` にその URL かキーを渡したタスクを作り、手順は `parent` でその下に置く。
- 順に進める手順は `blocked_by` で前の手順を待たせる。同じ呼び出しのタスクは `#1` のように何番目かで指す。
- 子セッションに任せるタスクは、`start_session` の `task` に id を渡す。起動済みの子セッションには、`update_task` の `session` で後から任せる。任されたタスクは子セッションのルートのタスクになり、親のタスクと同じ 1 つのタスクなので、終えたら `completed` にすると親でも完了になる。親セッションの `list_tasks` には子セッションの手順も `任せたタスクの id/子の id` で並び、`update_task` でその id を変えられる。`create_tasks` の `parent` に任せたタスクかその id を渡すと、子セッションに登録する。
- 一覧は `list_tasks` で読んだときだけ返る（既定はいちばん外の階層の未完了だけ）。`status`・`ticket`・`depth`・`fields` で絞り、`jq` のクエリで自由に絞れる。登録と更新の返事は短いので、覚えておく必要はない。
