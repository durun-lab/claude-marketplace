//! パネルとウィンドウのコマンドを Tauri のウィンドウへ渡すフック。

use std::sync::OnceLock;

use mm_ui_layout::Place;

use super::WindowAction;

/// ウィンドウのフックへ渡すもの。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Forward {
    /// パネルが持ち主のコマンドの ID。WebView の Shell が持ち主のパネルのタブを開き、パネルを選ぶコマンドのほかは
    /// パネルの `requests` も進める。
    Panel(&'static str),
    Window(WindowAction),
    /// WebView で押した、キーの表に無い Alt+英字の字（小文字）。ウィンドウがその字をアクセスキーとしてメニューを開く。
    MenuKey(char),
    /// サーフェスの右クリックのメニューを、当たった所と点（サーフェスのクライアントの物理ピクセル）で出す。
    ContextMenu(Place, (i32, i32)),
}

/// ウィンドウのフックの中身。
type ForwardFn = Box<dyn Fn(Forward) + Send + Sync>;

/// パネルとウィンドウのコマンドを、Tauri の `AppHandle` と `Window` で実行するフック。メニューバーと同じく、
/// ウィンドウを作るときに設定する。
#[derive(Default)]
pub struct WindowHook(OnceLock<ForwardFn>);

impl WindowHook {
    /// 渡す先を設定する。2 回目からは無視する。
    pub fn set(&self, forward: impl Fn(Forward) + Send + Sync + 'static) {
        let _ = self.0.set(Box::new(forward));
    }

    /// 設定した先へ渡す。ウィンドウを作る前は渡す先が無いので拒否する。
    pub fn send(&self, forward: Forward) -> Result<(), String> {
        let f = self
            .0
            .get()
            .ok_or_else(|| "The window isn't ready yet".to_owned())?;
        f(forward);
        Ok(())
    }
}
