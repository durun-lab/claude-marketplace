//! Shy（After Effects のレイヤーの Shy）。Shy のフラグを立てたレイヤーの ID と、フラグを立てた行を Timeline から外すかどうか。
//! 表示だけの状態で、描画と書き出しは変わらない。操作ではないので取り消しのスタックに入らず、`.mm/view.json` に置く。

use std::collections::BTreeSet;
use std::sync::atomic::Ordering;

use super::ViewSlot;

/// Shy のフラグと、Shy の行を隠すかどうか。コンポジションごと。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Shy {
    /// Shy のフラグを立てたレイヤーの ID。
    pub layers: BTreeSet<String>,
    /// フラグを立てた行を Timeline から外す（View > Hide Shy Layers）。
    pub hide: bool,
}

impl Shy {
    /// Timeline から外すレイヤーか。
    pub fn hides(&self, id: &str) -> bool {
        self.hide && self.layers.contains(id)
    }
}

impl ViewSlot {
    /// 今の Shy の状態。
    pub fn shy(&self) -> Shy {
        self.inner.lock().expect("view").shy.clone()
    }

    /// Shy の状態が変わるたびに進む通し番号。描画スレッドが Timeline の行を作り直すきっかけにする。
    pub fn shy_serial(&self) -> u64 {
        self.shy_serial.load(Ordering::SeqCst)
    }

    /// レイヤー `id` の Shy のフラグを立てる・外す。
    pub fn toggle_shy(&self, id: &str) {
        self.change_shy(|shy| {
            if !shy.layers.remove(id) {
                shy.layers.insert(id.to_owned());
            }
        });
    }

    /// フラグを立てた行を隠す・見せる。
    pub fn toggle_hide_shy(&self) {
        self.change_shy(|shy| shy.hide = !shy.hide);
    }

    fn change_shy(&self, change: impl FnOnce(&mut Shy)) {
        {
            let mut inner = self.inner.lock().expect("view");
            change(&mut inner.shy);
            inner.dirty = true;
        }
        self.shy_serial.fetch_add(1, Ordering::SeqCst);
        self.wake();
    }
}
