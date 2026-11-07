//! 合成モード（`Layer.blend`）を GPU の固定の合成で描く。
//!
//! 画像は premultiplied で、式は W3C Compositing and Blending Level 1 の premultiplied の形
//! `co = cs·(1 − αb) + cb·(1 − αs) + αs·αb·B(Cb, Cs)`。下の色を読まずに固定の合成で正しく描ける
//! `add`（線形の加算）、`screen`、`multiply` を描く。`multiply` は `cs·cb + cb·(1 − αs)` と `cs·(1 − αb)` の 2 回に分け、
//! 1 回目はアルファを書かずに下のアルファ（αb）を 2 回目まで残す。ほかのモードは下の色を読む別のパスが要るので
//! `normal` で重ね、レンダラーの誤りに残す。

use mm_doc::BlendMode;

/// 1 回の描画の合成の仕方。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BlendPass {
    /// source-over。
    Over,
    /// 色を足し、アルファは source-over。
    Add,
    /// `cs + cb·(1 − cs)`。
    Screen,
    /// `cs·cb + cb·(1 − αs)`。アルファは書かない。
    MultiplyCore,
    /// `cs·(1 − αb)` を足し、アルファは source-over。
    MultiplyRest,
}

impl BlendPass {
    /// 全部のパス。パイプラインを作る順。
    pub(crate) const ALL: [Self; 5] = [
        Self::Over,
        Self::Add,
        Self::Screen,
        Self::MultiplyCore,
        Self::MultiplyRest,
    ];

    /// `ALL` の中の位置。
    pub(crate) fn index(self) -> usize {
        Self::ALL
            .iter()
            .position(|p| *p == self)
            .expect("ALL は全部のパスを持つ")
    }

    /// GPU の合成の設定。
    pub(crate) fn state(self) -> wgpu::BlendState {
        use wgpu::BlendFactor as F;
        let over = component(F::One, F::OneMinusSrcAlpha);
        let (color, alpha) = match self {
            Self::Over => (over, over),
            Self::Add => (component(F::One, F::One), over),
            Self::Screen => (component(F::One, F::OneMinusSrc), over),
            Self::MultiplyCore => (
                component(F::Dst, F::OneMinusSrcAlpha),
                component(F::Zero, F::One),
            ),
            Self::MultiplyRest => (component(F::OneMinusDstAlpha, F::One), over),
        };
        wgpu::BlendState { color, alpha }
    }
}

fn component(src: wgpu::BlendFactor, dst: wgpu::BlendFactor) -> wgpu::BlendComponent {
    wgpu::BlendComponent {
        src_factor: src,
        dst_factor: dst,
        operation: wgpu::BlendOperation::Add,
    }
}

/// 合成モードを描くパスの並び。描けないモードは `None`（呼んだ側が `normal` で重ねて誤りに残す）。
pub(crate) fn passes(mode: BlendMode) -> Option<&'static [BlendPass]> {
    match mode {
        BlendMode::Normal => Some(&[BlendPass::Over]),
        BlendMode::Add => Some(&[BlendPass::Add]),
        BlendMode::Screen => Some(&[BlendPass::Screen]),
        BlendMode::Multiply => Some(&[BlendPass::MultiplyCore, BlendPass::MultiplyRest]),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// premultiplied の 1 チャンネルとアルファに、固定の合成を CPU で掛ける。
    fn apply(state: wgpu::BlendState, (cs, a_s): (f32, f32), (cd, ad): (f32, f32)) -> (f32, f32) {
        use wgpu::BlendFactor as F;
        let factor = |f: F, c_src: f32| match f {
            F::Zero => 0.0,
            F::One => 1.0,
            F::Src => c_src,
            F::OneMinusSrc => 1.0 - c_src,
            F::OneMinusSrcAlpha => 1.0 - a_s,
            F::Dst => cd,
            F::OneMinusDstAlpha => 1.0 - ad,
            other => panic!("使っていない係数 {other:?}"),
        };
        let c = cs * factor(state.color.src_factor, cs) + cd * factor(state.color.dst_factor, cs);
        let a =
            a_s * factor(state.alpha.src_factor, a_s) + ad * factor(state.alpha.dst_factor, a_s);
        (c, a)
    }

    fn draw(mode: BlendMode, src: (f32, f32), dst: (f32, f32)) -> (f32, f32) {
        passes(mode)
            .unwrap()
            .iter()
            .fold(dst, |d, p| apply(p.state(), src, d))
    }

    /// W3C の premultiplied の式。`b` は straight の色の上の合成の関数。
    fn w3c(b: fn(f32, f32) -> f32, (cs, a_s): (f32, f32), (cd, ad): (f32, f32)) -> (f32, f32) {
        let straight = |c: f32, a: f32| if a > 0.0 { c / a } else { 0.0 };
        let mixed = b(straight(cd, ad), straight(cs, a_s));
        let c = cs * (1.0 - ad) + cd * (1.0 - a_s) + a_s * ad * mixed;
        (c, a_s + ad * (1.0 - a_s))
    }

    #[test]
    fn 乗算とスクリーンは半透明の下地でも_w3c_の式と一致する() {
        let cases = [
            ((0.3, 0.6), (0.2, 0.5)),
            ((0.8, 1.0), (0.4, 1.0)),
            ((0.5, 0.5), (0.0, 0.0)),
        ];
        for (src, dst) in cases {
            let close =
                |a: (f32, f32), b: (f32, f32)| (a.0 - b.0).abs() < 1e-6 && (a.1 - b.1).abs() < 1e-6;
            let mul = draw(BlendMode::Multiply, src, dst);
            assert!(
                close(mul, w3c(|b, s| b * s, src, dst)),
                "multiply {src:?} {dst:?}: {mul:?}"
            );
            let scr = draw(BlendMode::Screen, src, dst);
            assert!(
                close(scr, w3c(|b, s| b + s - b * s, src, dst)),
                "screen {src:?} {dst:?}: {scr:?}"
            );
        }
    }

    #[test]
    fn 加算は色を足しアルファは重ねる() {
        assert_eq!(draw(BlendMode::Add, (0.25, 0.5), (0.5, 1.0)), (0.75, 1.0));
        assert_eq!(draw(BlendMode::Normal, (0.25, 0.5), (0.5, 1.0)), (0.5, 1.0));
    }

    #[test]
    fn 下の色を読むモードは描けないと返す() {
        assert!(passes(BlendMode::Overlay).is_none());
        assert!(passes(BlendMode::Difference).is_none());
    }
}
