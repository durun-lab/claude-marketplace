//! パネルを選ぶコマンド（View のパネルの項目）の表の行を作るマクロ。

/// パネルを選ぶコマンドの表の行。ID は `view.show_<パネルの id>`、持ち主はそのパネル。Shell はタブを選ぶだけで、
/// パネルの `requests`（Media の取り込みなど）は進めない。
macro_rules! show_panel {
    ($panel:literal, $name:literal, $description:expr) => {
        Spec {
            id: concat!("view.show_", $panel),
            name: $name,
            description: $description,
            owner: Owner::Panel($panel),
        }
    };
}
