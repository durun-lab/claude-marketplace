## 構造と開発

| 用語 | 定義 | 識別子 |
|---|---|---|
| 右クリックのメニュー | サーフェスの右クリックかメニューキーで、当たった所の種類に合わせて出すネイティブのポップアップのメニュー。項目と並びはメニューバーと同じ形（コマンドの表と `menu.rs` の並び）で、選んだ項目はメニューバーと同じ `on_menu_event` で受ける | `context_menu.rs`、`menu::context_groups`、`Forward::ContextMenu` |
| 右クリックのメニューの項目の ID | コマンドの ID に `context:` を付けたもの。メニューバーの項目の ID と重ならず、選んだ項目を割る時刻（Split Clip Here の）で実行する | `context_menu::item_id`、`context_menu::command_of`、`commands::run_in_context` |
| メニューキー | 右クリックのメニューを開くキー（Shift+F10 とアプリケーションキー）。OS が `WM_CONTEXTMENU`（lParam は -1）を送り、選んでいるものの位置でメニューを出す | `WM_CONTEXTMENU`、`PointerEvent::ContextMenuKey` |

## 再生と Timeline

| 用語 | 定義 | 識別子 |
|---|---|---|
| 当たった所 | ポインターの下の部品を当たり判定で決めたもの（カット、帯、見出しのボタン、キーフレームの行のひし形、スクロールバーなど）。レイヤーとカットの添字を含む | `mm_ui_layout::Hit`、`hit_test` |
| 当たった所の種類 | 当たった所を、右クリックのメニューの種類ごとにまとめたもの（カット、レイヤーの帯、見出し、キーフレームの行、ひし形、目盛り、行の無い所）。スクロールバー、ズームバー、角は持たない | `mm_ui_layout::HitKind`、`HitKind::of` |
| 割る時刻（Split Clip Here の） | 割れるカット（隙間でも端でもない所）を右クリックした点の時刻。Blade と同じく、フレームにそろえ、Snapping が入なら再生位置に吸い付ける。出したメニューの項目を選んだときだけ使い、次のメニューで置き直す | `Action::ContextMenu::time`、`ContextSlot::take_time` |
| Move Layer Up / Move Layer Down | 選んだレイヤーを Timeline の 1 行上（前）/ 1 行下（後ろ）へ動かすコマンド。Resolve の見出しの右クリックの Move Track Up と Move Track Down の Track を Layer にした名前 | `layer.move_up`、`layer.move_down`、`LayerCommand::MoveUp`、`LayerCommand::MoveDown` |

## 置き換えた語

### 右クリックのメニュー（D3）

| 元の語 | 文脈 | 正式な用語 |
|---|---|---|
| コンテキストメニュー | サーフェスの右クリックで出すメニュー | 右クリックのメニュー |
| ホバー先、押した所 | ポインターの下の所を分けた種類 | 当たった所の種類 |
| 当たった所（`Place` の意味で） | 当たった所を右クリックのメニューの種類ごとにまとめたもの。`Hit` の説明の「当たった所」と同じ語で、2 つの意味になる | 当たった所の種類 |
| `Place`、`pointed.rs` | 当たった所の種類の型とファイル。文字の置き場所の `place` と `TextPlacement`、メニューバーの位置の `menu::place_of` と紛れる | `HitKind`、`hit_kind.rs` |
| 空き（右クリックの） | 行の無い所。シーケンスの隙間を描く「空き」（`CutBar::gap`、隙間の色）と紛れる | 行の無い所 |
| 入口 | 当たった所を UI のスレッドへ渡す経路（Z5 の `HintSlot::current()` と D3 の `Action::ContextMenu`）と、コマンドを呼び出す所（WebView の表、Tasks の項目、Keyboard Shortcuts のウィンドウ、使い方の記録の「右クリック」）。用語集の「入口」は呼び出し箇所などに置き換えた語 | 経路（当たった所を渡す）、呼び出し箇所 |
| 腕（`WM_CONTEXTMENU` の） | `surface.rs` の振り分けの `match` の 1 つ。用語集の「腕」は分岐に置き換えた語 | 分岐 |
| 右クリックの時刻 | Split Clip Here の割る時刻。割れない所の右クリックでは持たない | 割る時刻（Split Clip Here の） |
| 重ねの順 | `moveLayer` の位置で決まるレイヤーの前後。見出しのドラッグと `header_drag.rs` の「重なりの順」と言い方が違う | 重なりの順 |
| `Shown` | 出したメニューの記録。何の記録かが名前に無い | `ShownMenu` |
| Bring Forward、Send Backward（`layer.bring_forward`、`layer.send_backward`） | 重なりの順を 1 つ前後へ動かすコマンド。Resolve の見出しの右クリックの名前（Move Track Up、Move Track Down）と合わない | Move Layer Up、Move Layer Down（`layer.move_up`、`layer.move_down`） |
