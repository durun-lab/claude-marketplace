//! 読んだアニメーションと描いたフレームの覚え。

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;

use velato::Composition;
use vello_cpu::kurbo::Affine;

use mm_types::{MmError, Result, RgbaImage, Size};

use crate::raster::{DrawRequest, Raster};

/// 覚えるアニメーションの数。1 つのプロジェクトで同時に見える Lottie のレイヤーの数の目安。
const KEPT_COMPOSITIONS: usize = 4;
/// 覚えるフレームの数。速さ 0 の静止と、短いループの往復で同じフレームが続く分。
const KEPT_FRAMES: usize = 8;

/// 読んだアニメーションと、読んだ時のファイルの更新時刻。
struct Loaded {
    path: PathBuf,
    modified: Option<SystemTime>,
    composition: Arc<Composition>,
}

/// 描いたフレームの内容を決める入力。時刻ではなく決めたフレームで持つので、ループの 2 周目も同じ画像を使う。
#[derive(Debug, Clone, PartialEq)]
struct FrameKey {
    path: PathBuf,
    modified: Option<SystemTime>,
    frame_bits: u64,
    size: Size,
    transform: [u64; 6],
}

/// 1 フレームを描く入力。
pub(crate) struct FrameJob<'a> {
    pub path: &'a Path,
    pub frame: f64,
    pub size: Size,
    pub transform: Affine,
}

/// 種類が持つ状態。新しい順に並べ、上限を超えたら古いものから捨てる。
#[derive(Default)]
pub(crate) struct LottieCache {
    loaded: Vec<Loaded>,
    frames: Vec<(FrameKey, Arc<RgbaImage>)>,
    raster: Raster,
}

impl LottieCache {
    /// `path` のアニメーション。更新時刻が同じなら読み直さない。
    pub(crate) fn composition(&mut self, path: &Path) -> Result<Arc<Composition>> {
        let modified = modified(path)?;
        if let Some(i) = self
            .loaded
            .iter()
            .position(|l| l.path == path && l.modified == modified)
        {
            let hit = self.loaded.remove(i);
            let composition = hit.composition.clone();
            self.loaded.insert(0, hit);
            return Ok(composition);
        }
        let bytes = std::fs::read(path)?;
        let composition = Arc::new(Composition::from_slice(&bytes).map_err(|e| {
            MmError::InvalidArgument(format!("lottie: {} を読めない: {e}", path.display()))
        })?);
        self.loaded.insert(
            0,
            Loaded {
                path: path.to_path_buf(),
                modified,
                composition: composition.clone(),
            },
        );
        self.loaded.truncate(KEPT_COMPOSITIONS);
        Ok(composition)
    }

    /// `job` のフレーム。同じ入力を前に描いていれば、描かずにその画像を返す。
    pub(crate) fn frame(
        &mut self,
        composition: &Composition,
        job: &FrameJob<'_>,
    ) -> Result<Arc<RgbaImage>> {
        let key = FrameKey {
            path: job.path.to_path_buf(),
            modified: modified(job.path)?,
            frame_bits: job.frame.to_bits(),
            size: job.size,
            transform: job.transform.as_coeffs().map(f64::to_bits),
        };
        if let Some(i) = self.frames.iter().position(|(k, _)| *k == key) {
            let hit = self.frames.remove(i);
            let image = hit.1.clone();
            self.frames.insert(0, hit);
            return Ok(image);
        }
        let image = Arc::new(self.raster.draw(&DrawRequest {
            composition,
            frame: job.frame,
            size: job.size,
            transform: job.transform,
        })?);
        self.frames.insert(0, (key, image.clone()));
        self.frames.truncate(KEPT_FRAMES);
        Ok(image)
    }
}

/// ファイルの更新時刻。時刻を返さないファイルシステムでは `None`（中身の変化は見分けない）。
fn modified(path: &Path) -> Result<Option<SystemTime>> {
    Ok(std::fs::metadata(path)?.modified().ok())
}
