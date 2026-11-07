//! レイヤーの時刻から Lottie のフレームを決める式。

use std::ops::Range;

use mm_types::{ParamValue, Params};

/// 再生の速さの既定（1 倍）。
pub(crate) const DEFAULT_SPEED: f64 = 1.0;

/// 再生のしかた。パラメータ `speed`、`loop`、`start_frame` から作る。
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Timing {
    /// 再生の速さ。0 なら `start_frame` で止まる。
    pub speed: f64,
    /// 最後のフレームの次に先頭へ戻るか。偽なら最後のフレームで止まる。
    pub looping: bool,
    /// レイヤーの先頭で見せるフレーム（アニメーションの先頭 `ip` からの数）。
    pub start_frame: f64,
}

impl Timing {
    /// パラメータから。有限でない数は既定値にする（フレームが NaN になると何も描かれない）。
    pub(crate) fn of(params: &Params) -> Self {
        let finite = |name: &str, fallback: f64| {
            let v = params.number_or(name, fallback);
            if v.is_finite() {
                v
            } else {
                fallback
            }
        };
        Self {
            speed: finite("speed", DEFAULT_SPEED).max(0.0),
            looping: params
                .get("loop")
                .and_then(ParamValue::as_bool)
                .unwrap_or(true),
            start_frame: finite("start_frame", 0.0),
        }
    }

    /// レイヤーの時刻 `seconds` で見せるフレーム。`frames` はアニメーションの `ip..op`、`frame_rate` はその fps。
    /// 範囲が空か fps が正でなければ先頭のフレーム。
    pub(crate) fn frame_at(&self, seconds: f64, frames: &Range<f64>, frame_rate: f64) -> f64 {
        let len = frames.end - frames.start;
        let elapsed = self.start_frame + seconds * frame_rate * self.speed;
        if !(len > 0.0 && frame_rate > 0.0 && elapsed.is_finite()) {
            return frames.start;
        }
        let offset = if self.looping {
            elapsed.rem_euclid(len)
        } else {
            // `op` はアニメーションの外（最後のフレームの次）なので、止めるのはその 1 つ前。
            elapsed.clamp(0.0, (len - 1.0).max(0.0))
        };
        frames.start + offset
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FRAMES: Range<f64> = 10.0..40.0;
    const FPS: f64 = 30.0;

    fn timing(speed: f64, looping: bool, start_frame: f64) -> Timing {
        Timing {
            speed,
            looping,
            start_frame,
        }
    }

    #[test]
    fn 先頭は_ip_から数え_速さを掛ける() {
        let t = timing(1.0, true, 0.0);
        assert_eq!(t.frame_at(0.0, &FRAMES, FPS), 10.0);
        assert_eq!(t.frame_at(0.5, &FRAMES, FPS), 25.0);
        assert_eq!(timing(2.0, true, 0.0).frame_at(0.25, &FRAMES, FPS), 25.0);
        assert_eq!(timing(1.0, true, 5.0).frame_at(0.0, &FRAMES, FPS), 15.0);
    }

    #[test]
    fn ループは先頭へ戻り_負の時刻も範囲に入る() {
        let t = timing(1.0, true, 0.0);
        assert_eq!(t.frame_at(1.0, &FRAMES, FPS), 10.0, "30 フレームで 1 周");
        assert_eq!(t.frame_at(1.1, &FRAMES, FPS), 13.0);
        assert_eq!(t.frame_at(-0.1, &FRAMES, FPS), 37.0);
    }

    #[test]
    fn ループしなければ最後のフレームで止まる() {
        let t = timing(1.0, false, 0.0);
        assert_eq!(t.frame_at(5.0, &FRAMES, FPS), 39.0);
        assert_eq!(t.frame_at(-1.0, &FRAMES, FPS), 10.0);
    }

    #[test]
    fn 速さ_0_は開始のフレームで止まる() {
        let t = timing(0.0, true, 7.0);
        assert_eq!(t.frame_at(3.0, &FRAMES, FPS), 17.0);
    }

    #[test]
    fn 壊れた範囲と有限でない数は先頭にする() {
        let t = timing(1.0, true, 0.0);
        assert_eq!(t.frame_at(1.0, &(5.0..5.0), FPS), 5.0);
        assert_eq!(t.frame_at(1.0, &FRAMES, 0.0), 10.0);
        assert_eq!(t.frame_at(f64::NAN, &FRAMES, FPS), 10.0);
        let mut params = Params::new();
        params
            .set("speed", ParamValue::Number(f64::NAN))
            .set("start_frame", ParamValue::Number(f64::INFINITY));
        assert_eq!(Timing::of(&params), timing(DEFAULT_SPEED, true, 0.0));
    }
}
