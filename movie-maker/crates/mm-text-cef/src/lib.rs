//! 文字の描画の実装（`cef`）。シェーパーが組んだ行を、別プロセスの CEF（`mm-host-api` のホスト）に CSS で描かせる。
//!
//! - 行組み（どこで折るか、行の位置）はシェーパー（cosmic-text）が決め、ここでは行の中を折り返さずに描く。
//!   文字の幅と行の数がネイティブの描画と同じになり、`bounds` とプレートの大きさが合う。
//! - CSS で付けるもの: 書体（`font-family`）、縁取り（`-webkit-text-stroke` を `paint-order: stroke fill` で外側だけ）、
//!   ルビ（親文字の上の中央に absolute で置き、行の幅を変えない。高さはシェーパーが行の上に空けた場所に収める。
//!   注記はシェーパーと同じ `mm_text_api::parse_ruby` で読む）。
//! - 文字は `ShapedText::text` から取る。元の文字列を持たない行組みの結果は `Unsupported`。
//! - 画像は CPU の経路（RGBA f32、premultiplied、線形）で受ける。文字の画布は小さく、`text` の種類がキャッシュするので、
//!   共有テクスチャの経路を使わない。
//! - 続けて時間切れになったホストは止めて捨て、次の描画で起こし直す。
//!
//! ホストの実行ファイルとランチャーは組み立て（`mm-runtime`）が選んで渡す（実装は実装に依存しない）。

mod page;

use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use mm_host_api::{HostLauncher, HostProcess, HostRequest, HostResponse, HostSpec, SharedFrame};
use mm_text_api::{ShapedText, TextRasterizer, TextStyle};
use mm_types::{MmError, RationalTime, Result, RgbaImage, Size};

pub use page::{css_color, page_params};

/// 描画のページ。
const PAGE: &str = include_str!("../assets/text.html");
/// 1 回の描画の期限。
const RENDER_TIMEOUT: Duration = Duration::from_secs(5);
/// ページの読み込みの期限。
const PREPARE_TIMEOUT: Duration = Duration::from_secs(15);
/// 続けてこの回数だけ時間切れになったホストは止めて捨て、次の描画で起こし直す。
pub const MAX_TIMEOUTS: u32 = 2;

/// CSS で文字を描くレンダラー。ホストは最初の描画で起こし、ページを読み込んだまま使い回す。
pub struct CefRasterizer {
    launcher: Arc<dyn HostLauncher>,
    executable: PathBuf,
    slot: Mutex<HostSlot>,
}

/// 起こしたホストと、続けて時間切れになった回数。
#[derive(Default)]
struct HostSlot {
    host: Option<Box<dyn HostProcess>>,
    timeouts: u32,
}

impl HostSlot {
    /// 応答の時間切れを数え、続いたらホストを捨てる。止まったページに頼み続けると毎回期限まで待つ。
    fn count(&mut self, response: &Result<HostResponse>) {
        if !matches!(response, Err(MmError::Timeout { .. })) {
            self.timeouts = 0;
            return;
        }
        self.timeouts += 1;
        if self.timeouts >= MAX_TIMEOUTS {
            tracing::warn!(
                timeouts = self.timeouts,
                "文字のホストが応答しないので止める"
            );
            self.discard();
        }
    }

    /// ホストを止めて捨てる。
    fn discard(&mut self) {
        if let Some(mut host) = self.host.take() {
            if let Err(e) = host.kill() {
                tracing::warn!(%e, "文字のホストを止められない");
            }
        }
        self.timeouts = 0;
    }
}

impl CefRasterizer {
    /// `executable`（`mm-host-cef`）を `launcher` で起こして描く。
    pub fn new(launcher: Arc<dyn HostLauncher>, executable: PathBuf) -> Self {
        Self {
            launcher,
            executable,
            slot: Mutex::new(HostSlot::default()),
        }
    }

    /// ホストのロック。描画の途中でパニックしたホストは応答の途中かもしれないので、毒の入ったロックはホストを捨ててから使う。
    fn slot(&self) -> MutexGuard<'_, HostSlot> {
        self.slot.lock().unwrap_or_else(|poisoned| {
            let mut slot = poisoned.into_inner();
            slot.discard();
            self.slot.clear_poison();
            slot
        })
    }

    fn start(&self) -> Result<Box<dyn HostProcess>> {
        let spec = HostSpec {
            executable: self.executable.clone(),
            args: Vec::new(),
            default_timeout: RENDER_TIMEOUT,
            gpu: None,
            client_pid: None,
        };
        let mut host = self.launcher.spawn(&spec)?;
        let prepare = HostRequest::Prepare {
            params: serde_json::json!({ "html": PAGE }),
        };
        match host.request(&prepare, PREPARE_TIMEOUT)? {
            HostResponse::Prepared => Ok(host),
            other => Err(MmError::Internal(format!(
                "文字のページを読み込めない: {other:?}"
            ))),
        }
    }

    /// ホストに 1 枚描かせる。クラッシュしていたら 1 回だけ起こし直す。
    fn draw(&self, size: Size, params: serde_json::Value) -> Result<SharedFrame> {
        let mut slot = self.slot();
        let request = HostRequest::RenderFrame {
            time: RationalTime::ZERO,
            size,
            params,
        };
        for attempt in 0..2 {
            let host = match slot.host.as_mut() {
                Some(host) if host.is_alive() => host,
                _ => slot.host.insert(self.start()?),
            };
            let response = host.request(&request, RENDER_TIMEOUT);
            slot.count(&response);
            match response {
                Ok(HostResponse::Frame(frame)) => return Ok(frame),
                Ok(other) => {
                    return Err(MmError::Internal(format!(
                        "文字のページの描画に Frame でない応答: {other:?}"
                    )))
                }
                Err(MmError::HostCrashed(why)) if attempt == 0 => {
                    tracing::warn!(%why, "文字のホストを起こし直す");
                    slot.host = None;
                }
                Err(e) => return Err(e),
            }
        }
        Err(MmError::HostCrashed("文字のホストが起き直らない".into()))
    }
}

/// 画像の大きさ（`bounds * scale` を切り上げ。インターフェースの契約）。
fn raster_size(shaped: &ShapedText, scale: f32) -> Size {
    Size::new(
        (shaped.bounds.width * scale).ceil().max(0.0) as u32,
        (shaped.bounds.height * scale).ceil().max(0.0) as u32,
    )
}

impl TextRasterizer for CefRasterizer {
    fn id(&self) -> &'static str {
        "cef"
    }

    fn rasterize(&self, shaped: &ShapedText, style: &TextStyle, scale: f32) -> Result<RgbaImage> {
        let _span = tracing::info_span!("text_cef.rasterize").entered();
        let size = raster_size(shaped, scale);
        if shaped.lines.is_empty() || size.area() == 0 {
            return Ok(RgbaImage::transparent(size));
        }
        if shaped.text.is_empty() {
            return Err(MmError::Unsupported(
                "行組みの結果に元の文字列が無いので CSS で描けない".into(),
            ));
        }
        match self.draw(size, page_params(shaped, style, scale))? {
            SharedFrame::Cpu { size: got, data } if got == size => RgbaImage::from_raw(size, data),
            other => Err(MmError::Internal(format!(
                "文字の画像の大きさか形が違う（頼んだのは {size:?}）: {:?}",
                other.size()
            ))),
        }
    }
}

impl std::fmt::Debug for CefRasterizer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CefRasterizer")
            .field("launcher", &self.launcher.id())
            .field("executable", &self.executable)
            .finish()
    }
}
