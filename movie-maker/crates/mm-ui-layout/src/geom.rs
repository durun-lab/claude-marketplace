//! 画面の矩形と色。単位は論理ピクセル（物理ピクセルへは [`crate::Scale`] で換える）で、色は表示の値（0..=1、プリマルチなし）。

/// 矩形（左上と大きさ）。
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    pub const fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { x, y, w, h }
    }

    /// 右端。
    pub fn right(&self) -> f32 {
        self.x + self.w
    }

    /// 下端。
    pub fn bottom(&self) -> f32 {
        self.y + self.h
    }

    /// 点が中にあるか（右端と下端は含まない）。
    pub fn contains(&self, px: f32, py: f32) -> bool {
        px >= self.x && px < self.right() && py >= self.y && py < self.bottom()
    }

    /// 平行移動。
    pub fn translated(&self, dx: f32, dy: f32) -> Self {
        Self::new(self.x + dx, self.y + dy, self.w, self.h)
    }

    /// 内側へ `m` だけ縮める。幅と高さは 0 を下回らない。
    pub fn inset(&self, m: f32) -> Self {
        Self::new(
            self.x + m,
            self.y + m,
            (self.w - 2.0 * m).max(0.0),
            (self.h - 2.0 * m).max(0.0),
        )
    }
}

/// 色（RGBA、0..=1）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Color(pub [f32; 4]);

impl Color {
    /// 8 ビットの RGB（0xRRGGBB）から。
    pub const fn hex(rgb: u32) -> Self {
        Self([
            ((rgb >> 16) & 0xFF) as f32 / 255.0,
            ((rgb >> 8) & 0xFF) as f32 / 255.0,
            (rgb & 0xFF) as f32 / 255.0,
            1.0,
        ])
    }

    /// 不透明度を差し替える。
    pub const fn with_alpha(self, a: f32) -> Self {
        Self([self.0[0], self.0[1], self.0[2], a])
    }
}

/// `Quad` の形。矩形の中に収まる（外接する矩形が `Quad::rect`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Shape {
    /// 矩形全体（既定）。
    #[default]
    Rect,
    /// 矩形に内接するひし形。
    Diamond,
    /// 頂点が上の二等辺三角形（底辺は矩形の下端）。
    TriUp,
    /// 頂点が下の二等辺三角形（底辺は矩形の上端）。
    TriDown,
    /// 頂点が左の二等辺三角形（底辺は矩形の右端）。
    TriLeft,
    /// 頂点が右の二等辺三角形（底辺は矩形の左端）。
    TriRight,
}

/// 塗りつぶしの図形 1 つ（既定は矩形）。UI の部品はこれの並びに落とす。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Quad {
    /// 図形に外接する矩形。
    pub rect: Rect,
    pub color: Color,
    pub shape: Shape,
}

impl Quad {
    pub const fn new(rect: Rect, color: Color) -> Self {
        Self {
            rect,
            color,
            shape: Shape::Rect,
        }
    }

    /// 中心 `center`、外接する矩形の大きさ `size`（幅、高さ）の `shape`。
    pub const fn shaped(center: (f32, f32), size: (f32, f32), shape: Shape, color: Color) -> Self {
        Self {
            rect: Rect::new(
                center.0 - size.0 / 2.0,
                center.1 - size.1 / 2.0,
                size.0,
                size.1,
            ),
            color,
            shape,
        }
    }

    /// 中心 `center`、大きさ `size`（幅、高さ）のひし形。
    pub const fn diamond(center: (f32, f32), size: (f32, f32), color: Color) -> Self {
        Self::shaped(center, size, Shape::Diamond, color)
    }
}

/// `content`（幅、高さ）を縦横比を保って `container` の中に最大で収めた矩形（中央寄せ）。
pub fn fit_rect(content_w: f32, content_h: f32, container: Rect) -> Rect {
    if content_w <= 0.0 || content_h <= 0.0 || container.w <= 0.0 || container.h <= 0.0 {
        return Rect::new(container.x, container.y, 0.0, 0.0);
    }
    let scale = (container.w / content_w).min(container.h / content_h);
    let (w, h) = (content_w * scale, content_h * scale);
    Rect::new(
        container.x + (container.w - w) / 2.0,
        container.y + (container.h - h) / 2.0,
        w,
        h,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 横長の入れ物には縦の余白ができる() {
        let r = fit_rect(1920.0, 1080.0, Rect::new(0.0, 0.0, 1000.0, 1000.0));
        assert!((r.w - 1000.0).abs() < 1e-2 && r.x.abs() < 1e-2);
        assert!((r.h - 562.5).abs() < 1e-3);
        assert!((r.y - 218.75).abs() < 1e-3);
    }

    #[test]
    fn 大きさ0の入れ物は空の矩形を返す() {
        let r = fit_rect(16.0, 9.0, Rect::new(5.0, 5.0, 0.0, 10.0));
        assert_eq!((r.w, r.h), (0.0, 0.0));
    }

    #[test]
    fn 中心と大きさから外接する矩形を作る() {
        let q = Quad::diamond((10.0, 20.0), (8.0, 12.0), Color::hex(0xFFFFFF));
        assert_eq!(q.rect, Rect::new(6.0, 14.0, 8.0, 12.0));
        assert_eq!(q.shape, Shape::Diamond);
        let t = Quad::shaped((10.0, 20.0), (9.0, 5.0), Shape::TriDown, q.color);
        assert_eq!(t.rect, Rect::new(5.5, 17.5, 9.0, 5.0));
        assert_eq!(Quad::new(q.rect, q.color).shape, Shape::Rect);
    }

    #[test]
    fn 右端と下端は含まない() {
        let r = Rect::new(0.0, 0.0, 10.0, 10.0);
        assert!(r.contains(0.0, 0.0) && r.contains(9.9, 9.9));
        assert!(!r.contains(10.0, 5.0) && !r.contains(5.0, 10.0));
    }
}
