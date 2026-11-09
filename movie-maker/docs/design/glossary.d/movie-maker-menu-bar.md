## 構造と開発

| 用語 | 定義 | 識別子 |
|---|---|---|
| メニューバー | ウィンドウの上端の File、Edit、View、Playback、Timeline、Help。Tauri のメニュー（muda）で出し、項目の名前、キーの見た目、押せるかは UI のコマンドの表から作る | `menu.rs`、`menu::build` |
| メニューの並び | メニューごとのコマンドの ID の並び。区切りで分けたグループの並びとして持ち、表に無い ID は飛ばす。右クリックのメニューも同じ形 | `menu::BAR`、`menu::Groups` |
| グループ（メニューの） | メニューの並びの中の、区切りで分けた項目のまとまり。空のグループは区切りごと消える | `menu::lines` |
| アクセスキー | メニューの名前の下線の字（`&File` の F）。Alt と一緒に押すか、F10 でメニューバーに入った後に押すと、そのメニューを開く | `Menu::title` |
| ウィンドウのフック | UI のコマンドのうち、パネルとウィンドウ（Tauri の側）が実行するものを、Rust の表から渡す関数。メニューバーと一緒に、ウィンドウを作るときに置く | `commands::WindowHook`、`commands::Forward` |
| パネルのコマンドのイベント | パネルが持ち主のコマンドを、メニューとキーから Shell に実行させる Tauri のイベント。中身はコマンドの ID | `mm:panel-command`、`PANEL_COMMAND_EVENT`、`CommandsApi.onPanelCommand` |
