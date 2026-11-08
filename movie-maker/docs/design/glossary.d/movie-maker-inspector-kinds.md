## 再生と Timeline

| 用語 | 定義 | 識別子 |
|---|---|---|
| 既定値の行（Inspector の） | プロジェクトに値が無く、パラメータの定義の既定値を出している入力欄の行。文字を薄くし、触ると `setParam` で値を書く | `FieldRow::isDefault`、`data-default` |
| パラメータの定義の引き直し | 種類か値が変わったソースとエフェクトだけについて、`inspector_kind` でパラメータの組ごとの定義を引き直すこと。定義のある種類の値の変更は止まってから 1 回だけ引く | `useLayerKinds`、`LOOKUP_DEBOUNCE_MS` |
