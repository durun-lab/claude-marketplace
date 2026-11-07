//! 別プロセスのホストの持ち方。ページごとにホストを 1 つ読み込んだまま置き、同じページの次のフレームは
//! `RenderFrame` だけで済ませる。ホストの数は上限までで、溢れたら最後に使ったのが古いホストにページを読み直させる。

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use mm_host_api::{HostLauncher, HostProcess, HostRequest, HostResponse, HostSpec, SharedFrame};
use mm_types::{GpuAdapterId, MmError, Result};

use crate::page::FrameRequest;

/// 同時に起こしておくホストの数。1 つの CEF は数百 MB を使うので、並べて使う部品の数（2〜3）に留める。
const MAX_HOSTS: usize = 3;
/// 1 フレームの期限。ページの描画と共有テクスチャの受け渡しを含む。
const RENDER_TIMEOUT: Duration = Duration::from_secs(5);
/// ページの読み込みの期限。
const PREPARE_TIMEOUT: Duration = Duration::from_secs(15);
/// 続けてこの回数だけ時間切れになったホストは止めて捨てる。止まったページを持つホストに頼み続けると、
/// 毎フレーム期限まで待つことになる。
pub const MAX_TIMEOUTS: u32 = 2;

/// ホストからフレームを受け取る経路。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FrameTransport {
    /// CPU の画像（RGBA f32、premultiplied、線形）。
    Cpu,
    /// このアダプターの上の共有テクスチャ。`device` はレンダラーのデバイスの世代で、作り直すたびに変わる。
    Shared { adapter: GpuAdapterId, device: u64 },
}

struct Slot {
    page: u64,
    transport: FrameTransport,
    host: Box<dyn HostProcess>,
    /// 続けて時間切れになった回数。
    timeouts: u32,
}

/// ホストの組。
pub struct HostPages {
    launcher: Arc<dyn HostLauncher>,
    executable: PathBuf,
    /// 新しく使った順。
    slots: Vec<Slot>,
}

impl HostPages {
    /// `executable` を `launcher` で起こす。起こすのは最初のフレームを頼まれたとき。
    pub fn new(launcher: Arc<dyn HostLauncher>, executable: PathBuf) -> Self {
        Self {
            launcher,
            executable,
            slots: Vec::new(),
        }
    }

    /// 全部のホストを捨てる。次のフレームで起こし直す。
    pub fn clear(&mut self) {
        self.slots.clear();
    }

    fn spec(&self, transport: &FrameTransport) -> HostSpec {
        let (gpu, client_pid) = match transport {
            FrameTransport::Cpu => (None, None),
            FrameTransport::Shared { adapter, .. } => {
                (Some(adapter.clone()), Some(std::process::id()))
            }
        };
        HostSpec {
            executable: self.executable.clone(),
            args: Vec::new(),
            default_timeout: RENDER_TIMEOUT,
            gpu,
            client_pid,
        }
    }

    /// 1 フレームを描く。ホストがクラッシュしていたら 1 回だけ起こし直してやり直す。
    pub fn render(
        &mut self,
        req: &FrameRequest<'_>,
        transport: &FrameTransport,
    ) -> Result<SharedFrame> {
        let _span = tracing::info_span!("nodes_cef.host_frame").entered();
        self.retire_other_devices(transport);
        match self.try_render(req, transport) {
            Err(MmError::HostCrashed(why)) => {
                tracing::warn!(%why, "web.cef のホストを起こし直す");
                self.slots.retain(|s| s.host.is_alive());
                self.try_render(req, transport)
            }
            other => other,
        }
    }

    /// 共有テクスチャで頼むとき、ほかのデバイスの世代に向けて起こしたホストを止める。
    /// デバイスを作り直すと、古いホストは古いアダプター（か消えたデバイス）の上で描き続けるため、
    /// 次のフレームで新しいアダプターのホストを起こし直す。
    fn retire_other_devices(&mut self, transport: &FrameTransport) {
        if matches!(transport, FrameTransport::Shared { .. }) {
            self.slots
                .retain(|s| s.transport == FrameTransport::Cpu || s.transport == *transport);
        }
    }

    fn try_render(
        &mut self,
        req: &FrameRequest<'_>,
        transport: &FrameTransport,
    ) -> Result<SharedFrame> {
        let slot = self.slot_for(req, transport)?;
        let request = HostRequest::RenderFrame {
            time: req.time,
            size: req.size,
            params: req.page.frame_params(req.scale),
        };
        let response = slot.host.request(&request, RENDER_TIMEOUT);
        self.count_timeout(&response);
        match response? {
            HostResponse::Frame(frame) => Ok(frame),
            HostResponse::Error { message } => Err(MmError::Internal(format!(
                "web.cef のページを描けない: {message}"
            ))),
            other => Err(MmError::Internal(format!(
                "RenderFrame に Frame でない応答: {other:?}"
            ))),
        }
    }

    /// 先頭（今頼んだ）のホストの時間切れを数え、続いたら止めて捨てる。
    fn count_timeout(&mut self, response: &Result<HostResponse>) {
        let Some(slot) = self.slots.first_mut() else {
            return;
        };
        if !matches!(response, Err(MmError::Timeout { .. })) {
            slot.timeouts = 0;
            return;
        }
        slot.timeouts += 1;
        if slot.timeouts >= MAX_TIMEOUTS {
            let mut stale = self.slots.remove(0);
            tracing::warn!(
                timeouts = stale.timeouts,
                "web.cef のホストが応答しないので止める"
            );
            if let Err(e) = stale.host.kill() {
                tracing::warn!(%e, "web.cef のホストを止められない");
            }
        }
    }

    /// そのページを読み込んだホスト。無ければ起こすか、古いホストに読み直させる。先頭に置いて返す。
    fn slot_for(
        &mut self,
        req: &FrameRequest<'_>,
        transport: &FrameTransport,
    ) -> Result<&mut Slot> {
        let hit = self
            .slots
            .iter()
            .position(|s| s.page == req.page.hash && s.transport == *transport);
        let slot = match hit {
            Some(i) => self.slots.remove(i),
            None => self.load(req, transport)?,
        };
        self.slots.insert(0, slot);
        Ok(&mut self.slots[0])
    }

    fn load(&mut self, req: &FrameRequest<'_>, transport: &FrameTransport) -> Result<Slot> {
        let reuse = (self.slots.len() >= MAX_HOSTS)
            .then(|| self.slots.iter().rposition(|s| s.transport == *transport))
            .flatten();
        let mut host = match reuse {
            Some(i) => self.slots.remove(i).host,
            None => {
                if self.slots.len() >= MAX_HOSTS {
                    self.slots.pop();
                }
                self.launcher.spawn(&self.spec(transport))?
            }
        };
        let prepare = HostRequest::Prepare {
            params: serde_json::json!({ "html": req.page.html }),
        };
        match host.request(&prepare, PREPARE_TIMEOUT)? {
            HostResponse::Prepared => Ok(Slot {
                page: req.page.hash,
                transport: transport.clone(),
                host,
                timeouts: 0,
            }),
            other => Err(MmError::Internal(format!(
                "web.cef のページを読み込めない: {other:?}"
            ))),
        }
    }
}

impl std::fmt::Debug for HostPages {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HostPages")
            .field("launcher", &self.launcher.id())
            .field("executable", &self.executable)
            .field("live", &self.slots.len())
            .finish()
    }
}

#[cfg(test)]
mod tests;
