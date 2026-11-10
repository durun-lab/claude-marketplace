//! 再生位置をフレームの数と秒の数で動かす（←、→、Shift+←、Shift+→）。

use super::Player;
use crate::win::timebase;

impl Player {
    /// 今のフレームから `n` フレーム動かす（再生中なら止める）。
    pub(super) fn step(&mut self, n: i64) {
        let here = self.pause_position();
        let idx = timebase::frame_index(here, self.fps_f) + n;
        self.seek_to(idx as f64 / self.fps_f);
    }

    /// 今のフレームから `n` 秒動かす（再生中なら止める）。1 秒は fps を丸めたフレームの数（タイムコードの 1 秒）。
    pub(super) fn step_seconds(&mut self, n: i64) {
        self.step(n * (self.fps_f.round() as i64).max(1));
    }
}
