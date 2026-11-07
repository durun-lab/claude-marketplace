//! 部品を置くコンポジションの画面。
//!
//! パックの位置と幅はテーマの本編の画面（`canvas`）の画素で書く。大きさの違うコンポジション（ショート）に置くときは、
//! CSS の `right`・`bottom` や Premiere の Essential Graphics の「レスポンシブデザイン（位置）」と同じく、
//! 基準点に近い画面の縁からの距離を保って変換する。字の大きさと余白は画素のまま変えない（ショートは短い辺が
//! 本編と同じ 1080 なので、短い辺に対する字の大きさの割合も変わらない）。

use mm_doc::Project;
use mm_types::Id;

use crate::theme::Theme;

/// コンポジションの画面の大きさ（画素）を引く。
#[derive(Clone, Copy)]
pub(crate) struct Screens<'a> {
    theme: &'a Theme,
    project: Option<&'a Project>,
}

impl<'a> Screens<'a> {
    pub(crate) fn new(theme: &'a Theme, project: Option<&'a Project>) -> Self {
        Self { theme, project }
    }

    /// テーマの本編の画面（パックの位置を書いた画面）。
    pub(crate) fn canvas(&self) -> [f64; 2] {
        [
            f64::from(self.theme.canvas.width),
            f64::from(self.theme.canvas.height),
        ]
    }

    /// コンポジション `comp` の画面。プロジェクトに無いもの（プロジェクトを渡さない展開と、同じトランザクションで
    /// 足すコンポジション）はテーマの本編の画面にする。
    pub(crate) fn of(&self, comp: &str) -> [f64; 2] {
        self.project
            .and_then(|p| p.composition(&Id::new(comp)).ok())
            .map_or_else(
                || self.canvas(),
                |c| [f64::from(c.width), f64::from(c.height)],
            )
    }
}

/// 画面 `from` の画素で書いた位置を画面 `to` へ変換する。基準点 `anchor`（0..1）と同じ割合の画面の点からのずれを保つ
/// （1 なら右・下の縁から、0.5 なら中央から、0 なら左・上の縁からの距離）。
pub(crate) fn pin(position: [f64; 2], anchor: [f64; 2], from: [f64; 2], to: [f64; 2]) -> [f64; 2] {
    std::array::from_fn(|i| pin_axis(position[i], anchor[i], from[i], to[i]))
}

/// [`pin`] の 1 つの軸。`from` と `to` はその軸の画面の長さ。
pub(crate) fn pin_axis(position: f64, anchor: f64, from: f64, to: f64) -> f64 {
    position + anchor * (to - from)
}

/// 折り返す幅。狭い画面では画面の幅に対する割合を保つ（CSS の `min(幅, 割合%)`）。広い画面では画素のまま。
pub(crate) fn fit_width(width: f64, from: f64, to: f64) -> f64 {
    width.min(width * to / from)
}

#[cfg(test)]
mod tests {
    use super::*;

    const MAIN: [f64; 2] = [1920.0, 1080.0];
    const SHORT: [f64; 2] = [1080.0, 1920.0];

    #[test]
    fn 基準点に近い縁からの距離を保つ() {
        assert_eq!(pin([1872.0, 48.0], [1.0, 0.0], MAIN, SHORT), [1032.0, 48.0]);
        assert_eq!(
            pin([960.0, 958.0], [0.5, 1.0], MAIN, SHORT),
            [540.0, 1798.0]
        );
        assert_eq!(pin([64.0, 200.0], [0.0, 0.0], MAIN, SHORT), [64.0, 200.0]);
        assert_eq!(pin([960.0, 958.0], [0.5, 1.0], MAIN, MAIN), [960.0, 958.0]);
    }

    #[test]
    fn 狭い画面では幅の割合を保ち_広い画面では画素のまま() {
        assert_eq!(fit_width(1200.0, 1920.0, 960.0), 600.0);
        assert_eq!(fit_width(1200.0, 1920.0, 3840.0), 1200.0);
    }
}
