//! 画像の受け渡し。入力と結果を共有メモリ（`mm_host_pipe::shm`）に置き、パイプの JSON には区画の名前だけを書く
//! （`mm-host-ofx` の `RenderParams::shared`）。画像を JSON の数の配列にすると、1080p の往復に release で 1.6 s かかるため。
//! 共有メモリを作れない環境（Windows のほか）では、画像を JSON に入れて渡す。

use mm_host_api::{HostRequest, SharedFrame};
use mm_host_pipe::shm::{self, Mapping};
use mm_types::{MmError, Params, RationalTime, Result, RgbaImage, Size};

/// 共有メモリの名前の前置き。
const LINK_PREFIX: &str = "mm-ofx";

/// 1 フレームの頼み。
pub struct FrameJob<'a> {
    pub time: RationalTime,
    pub size: Size,
    /// 入力 0 の画像（大きさは `size`）。`None` なら透明。
    pub source: Option<&'a RgbaImage>,
    /// ホストに渡すパラメータの値。
    pub values: Params,
}

/// 1 枚の画像の共有メモリの上の大きさ（RGBA f32）。`mm_host_ofx::frame_bytes` と同じ式。
fn frame_bytes(size: Size) -> usize {
    size.width as usize * size.height as usize * 4 * std::mem::size_of::<f32>()
}

/// 共有メモリの区画。大きい画像が来たら作り直す。
#[derive(Default)]
pub struct Transport {
    shared: Option<(String, Mapping)>,
}

impl Transport {
    /// 次の頼みで区画を作り直す。時間切れの後に遅れて書かれた結果が、次のフレームの区画に入らないようにする。
    pub fn reset(&mut self) {
        self.shared = None;
    }

    /// `bytes` 以上の区画。作れなければ `None`（JSON で渡す）。
    fn mapping(&mut self, bytes: usize) -> Option<(&str, &mut Mapping)> {
        if !matches!(&self.shared, Some((_, m)) if m.len() >= bytes) {
            self.shared = None;
            let name = shm::unique_link(LINK_PREFIX);
            match Mapping::create(&name, bytes) {
                Ok(m) => self.shared = Some((name, m)),
                Err(e) => tracing::debug!(%e, "共有メモリを作れないので画像を JSON で渡す"),
            }
        }
        self.shared.as_mut().map(|(n, m)| (n.as_str(), m))
    }

    /// `RenderFrame` の要求。共有メモリがあれば入力をそこに書き、名前だけを JSON に入れる。
    pub fn request(&mut self, job: &FrameJob<'_>) -> Result<HostRequest> {
        let one = frame_bytes(job.size);
        let params = match self.mapping(2 * one) {
            Some((name, mapping)) => {
                if let Some(src) = job.source {
                    shm::write_f32(mapping.bytes_mut(), 0, src.data())
                        .ok_or_else(|| MmError::Internal("共有メモリが小さい".into()))?;
                }
                serde_json::json!({
                    "values": job.values,
                    "shared": { "name": name, "has_source": job.source.is_some() },
                })
            }
            None => serde_json::json!({
                "source": job.source.map(SharedFrame::from_image),
                "values": job.values,
            }),
        };
        Ok(HostRequest::RenderFrame {
            time: job.time,
            size: job.size,
            params,
        })
    }

    /// 応答を画像にする。`data` が空なら結果は共有メモリの後ろ半分にある。
    pub fn image(&self, frame: SharedFrame, size: Size) -> Result<RgbaImage> {
        let (got, data) = match frame {
            SharedFrame::Cpu { size, data } => (size, data),
            SharedFrame::SharedTexture { .. } => {
                return Err(MmError::Internal(
                    "CPU の画像を頼んだのに共有テクスチャが来た".into(),
                ))
            }
        };
        if got != size {
            return Err(MmError::Internal(format!(
                "ofx のフレームの大きさ {got:?} が頼んだ {size:?} と違う"
            )));
        }
        let data = match (&self.shared, data.is_empty()) {
            (Some((_, mapping)), true) => {
                let one = frame_bytes(size);
                shm::read_f32(mapping.bytes(), one, one / 4)
                    .ok_or_else(|| MmError::Internal("共有メモリが小さい".into()))?
            }
            (_, _) => data,
        };
        RgbaImage::from_raw(size, data)
    }
}

impl std::fmt::Debug for Transport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Transport")
            .field("shared", &self.shared.as_ref().map(|(n, m)| (n, m.len())))
            .finish()
    }
}
