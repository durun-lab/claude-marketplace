## モバイル版

| 用語 | 定義 | 識別子 |
|---|---|---|
| 端末の ID | 端末の登録ごとに新しく引く 8 バイトの乱数（16 進）。トランザクションと提案の発行元、提案を決めた者に残し、モバイル版の取り消しでどの端末の操作かを見分ける。端末の名前と違い、登録を外した後に同じ名前で登録し直しても同じ値にならない | `DeviceInfo::id`、`Actor::device`、`actor_device` |
| 提案を決めた者 | 提案を承認か却下した発行元。保留に戻すと消える | `StoredProposal::decided_by`、`decided_kind`、`decided_name`、`decided_device` |
