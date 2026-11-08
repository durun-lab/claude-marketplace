//! 静止画のフレーム。指定の時刻のプレビューを WebP にする（`GET /m/api/frame` の本体）。

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

use mm_doc::Project;
use mm_node_api::Quality;
use mm_runtime::Engine;
use mm_types::transfer::linear_to_code8;
use mm_types::{CompositionId, Id, MmError, RationalTime, Result, Size};

/// 幅の下限と上限（画素）。コメントの時刻の確認からスクラブのつまみまで、この範囲で足りる。
pub const STILL_MIN_WIDTH: u32 = 360;
pub const STILL_MAX_WIDTH: u32 = 720;
/// WebP の品質（0〜100）。幅 360〜720 の画像で 1 枚 20〜60 KB に収まる値。
const WEBP_QUALITY: f32 = 70.0;
/// 静止画のフレームのキャッシュに置く本体の合計の上限（バイト）。
const CACHE_LIMIT_BYTES: usize = 32 * 1024 * 1024;

/// 静止画のフレームの指定。
#[derive(Debug, Clone, PartialEq)]
pub struct StillRequest {
    /// コンポジションの ID。省くと `main`、それも無ければ先頭。
    pub composition: Option<String>,
    /// コンポジションの時間軸での時刻。
    pub time: RationalTime,
    /// 出力の幅。[`STILL_MIN_WIDTH`] から [`STILL_MAX_WIDTH`] に丸める。高さは縦横比を保つ。
    pub width: u32,
}

impl StillRequest {
    /// 丸めた後の幅。
    pub fn clamped_width(&self) -> u32 {
        self.width.clamp(STILL_MIN_WIDTH, STILL_MAX_WIDTH)
    }
}

/// 静止画のフレームのキャッシュのキー。プロジェクトのバージョンはキャッシュ全体を捨てる単位なので持たない。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct StillKey {
    composition: Option<String>,
    time: (i128, u32),
    width: u32,
}

impl StillKey {
    pub(crate) fn of(req: &StillRequest) -> Self {
        Self {
            composition: req.composition.clone(),
            time: (req.time.num(), req.time.den()),
            width: req.clamped_width(),
        }
    }

    /// 応答の `ETag` に使う文字列の元。
    pub(crate) fn tag(&self, version: u64) -> String {
        let comp = self.composition.as_deref().unwrap_or("-");
        format!(
            "\"v{version}-{comp}-{}_{}-{}\"",
            self.time.0, self.time.1, self.width
        )
    }
}

/// 符号化済みの WebP を、本体の合計が上限に収まるまで置く。古いものから捨てる。
#[derive(Default)]
pub(crate) struct StillCache {
    map: HashMap<StillKey, Arc<Vec<u8>>>,
    order: VecDeque<StillKey>,
    bytes: usize,
}

impl StillCache {
    pub(crate) fn get(&self, key: &StillKey) -> Option<Arc<Vec<u8>>> {
        self.map.get(key).cloned()
    }

    pub(crate) fn put(&mut self, key: StillKey, body: Arc<Vec<u8>>) {
        if body.len() > CACHE_LIMIT_BYTES {
            return;
        }
        if let Some(old) = self.map.insert(key.clone(), body.clone()) {
            self.bytes -= old.len();
            self.order.retain(|k| k != &key);
        }
        self.bytes += body.len();
        self.order.push_back(key);
        while self.bytes > CACHE_LIMIT_BYTES {
            let Some(oldest) = self.order.pop_front() else {
                break;
            };
            if let Some(b) = self.map.remove(&oldest) {
                self.bytes -= b.len();
            }
        }
    }
}

/// `engine` で 1 フレームを描いて WebP にする。
pub(crate) fn render_webp(
    engine: &mut Engine,
    project: &Project,
    req: &StillRequest,
) -> Result<Vec<u8>> {
    let comp = pick_composition(project, req.composition.as_deref())?;
    let c = project.composition(&comp)?;
    let size = scaled(Size::new(c.width, c.height), req.clamped_width());
    let image = engine.render_frame(project, &comp, req.time, size, Quality::Full)?;
    // 画像は線形の premultiplied。黒に重ねるので、符号化値にするだけでよい。
    let mut rgb = Vec::with_capacity(image.size().area() * 3);
    for y in 0..image.height() {
        for x in 0..image.width() {
            let p = image.pixel(x, y);
            rgb.extend([p.r, p.g, p.b].map(linear_to_code8));
        }
    }
    let webp = webp::Encoder::new(&rgb, webp::PixelLayout::Rgb, image.width(), image.height())
        .encode_simple(false, WEBP_QUALITY)
        .map_err(|e| MmError::Internal(format!("WebP に書けない: {e:?}")))?;
    Ok(webp.to_vec())
}

fn pick_composition(project: &Project, wanted: Option<&str>) -> Result<CompositionId> {
    if let Some(id) = wanted {
        let id: CompositionId = Id::new(id);
        project.composition(&id)?;
        return Ok(id);
    }
    let main: CompositionId = Id::new("main");
    if project.composition(&main).is_ok() {
        return Ok(main);
    }
    project
        .compositions
        .first()
        .map(|c| c.id.clone())
        .ok_or_else(|| MmError::NotFound("コンポジションが 1 つも無い".into()))
}

/// 幅を決めて縦横比を保つ。
fn scaled(size: Size, width: u32) -> Size {
    let h = (u64::from(size.height) * u64::from(width) / u64::from(size.width.max(1))).max(1);
    Size::new(width, h as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(width: u32) -> StillKey {
        StillKey::of(&StillRequest {
            composition: None,
            time: RationalTime::ZERO,
            width,
        })
    }

    #[test]
    fn 幅は範囲に丸めてキーにする() {
        assert_eq!(key(100), key(STILL_MIN_WIDTH));
        assert_eq!(key(4000), key(STILL_MAX_WIDTH));
        assert_ne!(key(400), key(500));
    }

    #[test]
    fn キャッシュは上限を超えたら古いものから捨てる() {
        let mut c = StillCache::default();
        let chunk = Arc::new(vec![0u8; CACHE_LIMIT_BYTES / 2 + 1]);
        c.put(key(400), chunk.clone());
        c.put(key(500), chunk.clone());
        assert!(c.get(&key(400)).is_none());
        assert!(c.get(&key(500)).is_some());
    }

    #[test]
    fn 縦横比を保って幅を決める() {
        assert_eq!(scaled(Size::new(1920, 1080), 640), Size::new(640, 360));
        assert_eq!(scaled(Size::new(1080, 1920), 360), Size::new(360, 640));
    }
}
