//! 素材を開いている途中のレイヤーの描き直し。プレビューの描画は長い録画の素材を開き終わるのを待たず、そのレイヤーを
//! 抜いて描く（`open_wait_ms`）。抜いたフレームは描かなかったレイヤーの数と一緒に渡し、描画スレッドがビューアー
//! （Program、Source）に印を出す。
//! 止まっている間は開き終わったかを見張り、開き終わったら今のフレームを描き直す。

use std::time::Duration;

use mm_runtime::RuntimeConfig;

use super::{runtime_config, Player};
use crate::win::state::FrameImage;

/// 1 フレームで素材を開くのを待つ上限（ミリ秒）。画像や短い素材は数 ms〜十数 ms で開けるので待ち、PTS の索引を作る長い
/// 録画（4 GB で十数秒）だけをバックグラウンドのスレッドに任せる。
const OPEN_WAIT_MS: u64 = 100;

/// 描かなかったレイヤーの素材が開き終わったかを見る間隔。
const OPENED_POLL: Duration = Duration::from_millis(50);

/// プレビューの描画の設定。素材を開くのは [`OPEN_WAIT_MS`] まで待ち、超えたらバックグラウンドのスレッドで開いて、
/// そのレイヤーを抜いて描く。書き出しと配信は [`runtime_config`] のまま、開き終わるまで待つ。
pub(super) fn preview_config() -> RuntimeConfig {
    RuntimeConfig {
        open_wait_ms: Some(OPEN_WAIT_MS),
        ..runtime_config()
    }
}

impl Player {
    /// 止まっている間に、描かなかったレイヤーの素材が開き終わったかを見る間隔。描かなかったレイヤーが無ければ `None`。
    pub(super) fn opened_poll(&self) -> Option<Duration> {
        (self.engine.pending_layers() > 0).then_some(OPENED_POLL)
    }

    /// 描かなかったレイヤーの素材が、直前に描き始めた後に開き終わっていたら、今のフレームを描き直す。
    pub(super) fn redraw_opened(&mut self) {
        if self.engine.pending_layers() == 0 || self.engine.sources_opened() == self.opened_seen {
            return;
        }
        if let Some(index) = self.last_frame {
            self.render_frame(index);
        }
    }

    /// 描いたフレームを、描かなかったレイヤーの数と一緒に描画スレッドへ渡す。数が 0 でなければビューアーに印が出る。
    pub(super) fn put_frame(&self, index: i64, image: FrameImage) {
        let pending = self.engine.pending_layers();
        self.shared
            .frames
            .put(index as f64 / self.fps_f, image, pending);
    }
}
