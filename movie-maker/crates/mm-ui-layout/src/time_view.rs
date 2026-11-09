//! Timeline の表示の範囲。時刻（秒）と画面の x の変換、拡大縮小、移動。

/// 縮小の下限の 1 秒あたりのピクセル数。これより細かく縮めるのは、全体が収まるまで（[`TimeView::zoom_floor`]）。
pub const MIN_PX_PER_SEC: f64 = 2.0;
/// 1 秒あたりの最大のピクセル数。60 fps の 1 フレームが 40 px 以上になる。
pub const MAX_PX_PER_SEC: f64 = 2400.0;
/// 全体表示の 1 秒あたりのピクセル数の下限。目盛りのいちばん粗い間隔（1 時間）でも時刻の文字が重ならない（72 px / 3600 秒）。
pub const MIN_FIT_PX_PER_SEC: f64 = 0.02;

/// 表示の範囲。左端の時刻と倍率で決まる。x は Timeline の矩形の左端を 0 とする。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TimeView {
    /// 左端の時刻（秒）。
    pub origin: f64,
    /// 1 秒のピクセル数。
    pub px_per_sec: f64,
}

impl TimeView {
    /// `duration` 秒が `width` px にちょうど収まる表示（左右に `margin` px の余白）。
    /// 長いコンポジションも収まるよう、[`MIN_PX_PER_SEC`] より細かく [`MIN_FIT_PX_PER_SEC`] まで縮める。
    pub fn fit(duration: f64, width: f32, margin: f32) -> Self {
        let usable = (f64::from(width) - 2.0 * f64::from(margin)).max(1.0);
        let px = (usable / duration.max(1e-6)).clamp(MIN_FIT_PX_PER_SEC, MAX_PX_PER_SEC);
        Self {
            origin: -f64::from(margin) / px,
            px_per_sec: px,
        }
    }

    /// 縮小の下限。全体表示の倍率と [`MIN_PX_PER_SEC`] の小さい方で、長いコンポジションも全体が収まるまで縮められる。
    pub fn zoom_floor(duration: f64, width: f32, margin: f32) -> f64 {
        Self::fit(duration, width, margin)
            .px_per_sec
            .min(MIN_PX_PER_SEC)
    }

    /// 時刻 → x。
    pub fn x_of(&self, t: f64) -> f32 {
        ((t - self.origin) * self.px_per_sec) as f32
    }

    /// x → 時刻。
    pub fn t_of(&self, x: f32) -> f64 {
        f64::from(x) / self.px_per_sec + self.origin
    }

    /// 幅 `width` px で見える時刻の範囲（始め、終わり）。
    pub fn visible(&self, width: f32) -> (f64, f64) {
        (self.origin, self.t_of(width))
    }

    /// `anchor_x` の下の時刻を動かさずに `factor` 倍へ拡大する。倍率は縮小の下限 `floor`（[`Self::zoom_floor`]）から
    /// [`MAX_PX_PER_SEC`] に収める。今の倍率が下限より細かいとき（狭いウィンドウで保存した表示）は、縮めて拡大しないよう今の倍率で止める。
    pub fn zoom_at(&self, anchor_x: f32, factor: f64, floor: f64) -> Self {
        let lowest = floor.min(self.px_per_sec).min(MAX_PX_PER_SEC);
        let px = (self.px_per_sec * factor).clamp(lowest, MAX_PX_PER_SEC);
        let anchor_t = self.t_of(anchor_x);
        Self {
            origin: anchor_t - f64::from(anchor_x) / px,
            px_per_sec: px,
        }
    }

    /// 右へ `dx` px 動かした表示（つまみを右へ引くと内容も右へ動く）。
    pub fn pan_px(&self, dx: f32) -> Self {
        Self {
            origin: self.origin - f64::from(dx) / self.px_per_sec,
            ..*self
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn x_と時刻は往復する() {
        let v = TimeView {
            origin: 1.5,
            px_per_sec: 80.0,
        };
        for t in [0.0, 1.5, 7.25] {
            assert!((v.t_of(v.x_of(t)) - t).abs() < 1e-4);
        }
    }

    #[test]
    fn 全体表示は余白つきで幅に収まる() {
        let v = TimeView::fit(10.0, 1000.0, 20.0);
        assert!((v.x_of(0.0) - 20.0).abs() < 1e-3);
        assert!((v.x_of(10.0) - 980.0).abs() < 1e-3);
    }

    #[test]
    fn 長いコンポジションの全体表示は_min_px_per_sec_より細かくしても幅に収める() {
        // ep03（30 fps で 17349 フレーム）を、表示の倍率 1.25 の行の領域（788 論理ピクセル）に。
        let duration = 17349.0 / 30.0;
        let v = TimeView::fit(duration, 788.0, 16.0);
        assert!(v.px_per_sec < MIN_PX_PER_SEC, "{}", v.px_per_sec);
        assert!((v.x_of(0.0) - 16.0).abs() < 1e-3);
        assert!((v.x_of(duration) - 772.0).abs() < 1e-3);
        let longest = TimeView::fit(1e9, 788.0, 16.0);
        assert_eq!(longest.px_per_sec, MIN_FIT_PX_PER_SEC);
    }

    #[test]
    fn 縮小の下限は全体表示と_min_px_per_sec_の小さい方() {
        let long = TimeView::zoom_floor(578.3, 788.0, 16.0);
        assert!((long - 756.0 / 578.3).abs() < 1e-9, "{long}");
        assert_eq!(TimeView::zoom_floor(10.0, 788.0, 16.0), MIN_PX_PER_SEC);
    }

    #[test]
    fn 拡大はカーソルの下の時刻を動かさない() {
        let v = TimeView::fit(10.0, 1000.0, 0.0);
        let t = v.t_of(300.0);
        let z = v.zoom_at(300.0, 4.0, MIN_PX_PER_SEC);
        assert!((z.t_of(300.0) - t).abs() < 1e-6);
        assert!((z.px_per_sec - v.px_per_sec * 4.0).abs() < 1e-9);
    }

    #[test]
    fn 倍率は上下限に収まり_下限より細かい表示は縮めても拡大しない() {
        let v = TimeView::fit(10.0, 1000.0, 0.0);
        let floor = MIN_PX_PER_SEC;
        assert_eq!(v.zoom_at(0.0, 1e9, floor).px_per_sec, MAX_PX_PER_SEC);
        assert_eq!(v.zoom_at(0.0, 1e-9, floor).px_per_sec, MIN_PX_PER_SEC);
        let wide = TimeView {
            origin: 0.0,
            px_per_sec: 1.0,
        };
        assert_eq!(wide.zoom_at(0.0, 0.5, floor).px_per_sec, 1.0);
        assert_eq!(wide.zoom_at(0.0, 1.5, floor).px_per_sec, 1.5);
    }

    #[test]
    fn 右へ引くと内容が右へ動く() {
        let v = TimeView::fit(10.0, 1000.0, 0.0);
        let p = v.pan_px(50.0);
        assert!((p.x_of(0.0) - (v.x_of(0.0) + 50.0)).abs() < 1e-3);
    }
}
