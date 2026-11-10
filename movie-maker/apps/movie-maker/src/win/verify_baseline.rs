//! `--play-test` の再生の前の数字。再生の区間だけを数えるために、再生の前と後で引く。

use std::sync::atomic::Ordering;

use crate::win::state::Shared;

/// 再生の前の数字。
pub struct Baseline {
    pub dropped: u64,
    pub rendered: u64,
    pub presented: u64,
    pub underruns: u64,
}

impl Baseline {
    pub fn take(shared: &Shared) -> Self {
        Self {
            dropped: shared.dropped_frames.load(Ordering::SeqCst),
            rendered: shared.rendered_frames.load(Ordering::SeqCst),
            presented: shared.frames_presented.load(Ordering::SeqCst),
            underruns: shared.clock.get().map_or(0, |c| c.underruns()),
        }
    }
}
