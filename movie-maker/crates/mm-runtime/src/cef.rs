//! CEF のホストで描く実装（`web.cef` の種類と、文字の `cef`）。プロセスの中で 1 組を共有する。
//! 並行の書き出しはワーカーごとに組み立てるので、組み立てごとに作ると CEF のホスト（1 つ数百 MB）がワーカーの数だけ増える。
//! GPU のコンテキストもプロセスで共有する（`GpuContext::shared`）ので、共有テクスチャの経路のホストも使い回せる。

use std::sync::{Arc, Mutex, PoisonError, Weak};

use mm_nodes_cef::WebCefKind;
use mm_text_cef::CefRasterizer;

/// 組み立てが持っている `web.cef` の種類。どの組み立ても持っていなければ空。
static WEB_CEF: Mutex<Weak<WebCefKind>> = Mutex::new(Weak::new());
/// 組み立てが持っている文字の `cef` のレンダラー。
static TEXT_CEF: Mutex<Weak<CefRasterizer>> = Mutex::new(Weak::new());

/// 誰かが持っていればそれを、無ければ `make` で作って返す。最後の持ち主が手放すとホストも止まる。
fn shared<T>(slot: &Mutex<Weak<T>>, make: impl FnOnce() -> Option<T>) -> Option<Arc<T>> {
    let mut slot = slot.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some(alive) = slot.upgrade() {
        return Some(alive);
    }
    let made = Arc::new(make()?);
    *slot = Arc::downgrade(&made);
    Some(made)
}

/// `web.cef` の種類。CEF の配布物とホストの実行ファイルがあればそれで描き、無ければモックの矩形。
pub(crate) fn web_cef() -> Arc<WebCefKind> {
    let made = shared(&WEB_CEF, || {
        Some(match mm_host_cef::find_host() {
            Some(exe) => WebCefKind::with_host(Arc::new(mm_host_cef::CefLauncher), exe),
            None => {
                tracing::info!("CEF のホストが無いので web.cef はモックの矩形で描く");
                WebCefKind::mock()
            }
        })
    });
    made.unwrap_or_else(|| Arc::new(WebCefKind::mock()))
}

/// 文字の `cef` のレンダラー。ホストの実行ファイルが無ければ `None`。
pub(crate) fn text_cef() -> Option<Arc<CefRasterizer>> {
    shared(&TEXT_CEF, || {
        let exe = mm_host_cef::find_host()?;
        Some(CefRasterizer::new(Arc::new(mm_host_cef::CefLauncher), exe))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 持ち主がいる間は同じものを返し_いなくなったら作り直す() {
        let slot: Mutex<Weak<String>> = Mutex::new(Weak::new());
        let a = shared(&slot, || Some("a".to_string())).unwrap();
        let b = shared(&slot, || Some("b".to_string())).unwrap();
        assert!(Arc::ptr_eq(&a, &b), "持ち主がいれば作らない");
        drop((a, b));
        let c = shared(&slot, || Some("c".to_string())).unwrap();
        assert_eq!(*c, "c");
        assert!(shared(&Mutex::new(Weak::<String>::new()), || None).is_none());
    }

    #[test]
    fn 組み立てごとの_web_cef_は同じ種類を使う() {
        let a = web_cef();
        let b = web_cef();
        assert!(Arc::ptr_eq(&a, &b));
    }
}
