## 構造と開発

| 用語 | 定義 | 識別子 |
|---|---|---|
| 右クリックのメニュー | サーフェスの右クリックかメニューキーで、当たった所に合わせて出すネイティブのポップアップのメニュー。項目と並びはメニューバーと同じ形（コマンドの表と `menu.rs` の並び）で、選んだ項目はメニューバーと同じ `on_menu_event` で受ける | `context_menu.rs`、`menu::context_groups`、`Forward::ContextMenu` |
| 当たった所 | ポインターの下の所を粗く分けたもの（カット、レイヤーの帯、見出し、キーフレームの行、ひし形、目盛り、空き）。右クリックのメニューの種類を選ぶ。描画スレッドの当たり判定（`Hit`）から作る | `mm_ui_layout::Place`、`Place::of` |
| 右クリックのメニューの項目の ID | コマンドの ID に `context:` を付けたもの。メニューバーの項目の ID と重ならず、選んだ項目を右クリックの時刻で実行する | `context_menu::item_id`、`context_menu::command_of`、`commands::run_in_context` |
| メニューキー | 右クリックのメニューを開くキー（Shift+F10 とアプリケーションキー）。OS が `WM_CONTEXTMENU`（lParam は -1）を送り、選んでいるものの位置でメニューを出す | `WM_CONTEXTMENU`、`PointerEvent::ContextMenuKey` |
| 右クリックの時刻 | 割れるカット（隙間でも端でもない所）を右クリックした点の時刻。Blade と同じく、フレームにそろえ、Snapping が入なら再生位置に吸い付ける。Split Clip Here の割る時刻で、出したメニューの項目を選んだときだけ使い、次のメニューで置き直す | `Action::ContextMenu::time`、`ContextSlot::take_time` |

## 再生と Timeline

| 用語 | 定義 | 識別子 |
|---|---|---|
| Move Layer Up / Move Layer Down | 選んだレイヤーを Timeline の 1 行上（前）/ 1 行下（後ろ）へ動かすコマンド。Resolve の見出しの右クリックの Move Track Up と Move Track Down の Track を Layer にした名前 | `layer.move_up`、`layer.move_down`、`LayerCommand::MoveUp`、`LayerCommand::MoveDown` |

## 置き換えた語

### 右クリックのメニュー（D3）

| 元の語 | 文脈 | 正式な用語 |
|---|---|---|
| コンテキストメニュー | サーフェスの右クリックで出すメニュー | 右クリックのメニュー |
| ホバー先、押した所 | ポインターの下の所を分けた種類 | 当たった所 |
| Bring Forward、Send Backward（`layer.bring_forward`、`layer.send_backward`） | 重ねの順を 1 つ前後へ動かすコマンド。Resolve の見出しの右クリックの名前（Move Track Up、Move Track Down）と合わない | Move Layer Up、Move Layer Down（`layer.move_up`、`layer.move_down`） |
