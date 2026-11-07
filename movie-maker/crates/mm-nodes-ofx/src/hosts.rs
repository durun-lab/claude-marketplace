//! 別プロセスのホストの持ち方。プラグインごとにホストを 1 つ `Prepare` したまま置き、同じプラグインの次のフレームは
//! `RenderFrame` だけで済ませる。ホストの数は上限までで、溢れたら最後に使ったのが古いホストを止める。
//! ホストはプラグインの共有ライブラリを解放しない（`mm-host-ofx` の README）ので、別のプラグインを読み直させずに起こし直す。

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use mm_host_api::{HostLauncher, HostProcess, HostRequest, HostResponse, HostSpec, SharedFrame};
use mm_types::{MmError, Result, RgbaImage};

use crate::frames::{FrameJob, Transport};

/// 同時に起こしておくホストの数。1 つのレイヤーに効果を重ねる数（2〜4）に合わせる。
const MAX_HOSTS: usize = 4;
/// 1 フレームの期限。共有メモリの無い環境では画像を JSON で往復し、4K の dev のビルドで 20 s かかるので、
/// それに収まる長さにする。
const RENDER_TIMEOUT: Duration = Duration::from_secs(30);
/// プラグインの読み込みとインスタンスの作成の期限。
const PREPARE_TIMEOUT: Duration = Duration::from_secs(15);
/// 続けてこの回数だけ時間切れになったホストは止めて捨てる。
pub const MAX_TIMEOUTS: u32 = 2;

/// ホストに読み込ませるプラグイン。`stamp` はバンドルの中身の印で、変わったら別のホストにする。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    pub bundle: PathBuf,
    pub plugin: String,
    pub stamp: u64,
}

struct Slot {
    target: Target,
    host: Box<dyn HostProcess>,
    /// 続けて時間切れになった回数。
    timeouts: u32,
}

/// ホストの組。
pub struct Hosts {
    launcher: Arc<dyn HostLauncher>,
    executable: PathBuf,
    /// 新しく使った順。
    slots: Vec<Slot>,
    /// 画像を置く共有メモリ。描くのはロックの中で 1 つずつなので、全部のホストで 1 つを使い回す。
    transport: Transport,
}

impl Hosts {
    /// `executable` を `launcher` で起こす。起こすのは最初のフレームを頼まれたとき。
    pub fn new(launcher: Arc<dyn HostLauncher>, executable: PathBuf) -> Self {
        Self {
            launcher,
            executable,
            slots: Vec::new(),
            transport: Transport::default(),
        }
    }

    /// 全部のホストを捨てる。次のフレームで起こし直す。
    pub fn clear(&mut self) {
        self.slots.clear();
    }

    /// 1 フレームを描いて画像にする。画像は共有メモリで受け渡す（`frames.rs`）。
    pub fn render_image(&mut self, target: &Target, job: &FrameJob<'_>) -> Result<RgbaImage> {
        let request = self.transport.request(job)?;
        let frame = self.render(target, &request);
        if matches!(frame, Err(MmError::Timeout { .. })) {
            self.transport.reset();
        }
        self.transport.image(frame?, job.size)
    }

    /// 1 フレームを描く。ホストがクラッシュしていたら 1 回だけ起こし直してやり直す。
    fn render(&mut self, target: &Target, request: &HostRequest) -> Result<SharedFrame> {
        let _span = tracing::info_span!("nodes_ofx.host_frame").entered();
        match self.try_render(target, request) {
            Err(MmError::HostCrashed(why)) => {
                tracing::warn!(%why, plugin = %target.plugin, "ofx のホストを起こし直す");
                self.slots.retain(|s| s.host.is_alive());
                self.try_render(target, request)
            }
            other => other,
        }
    }

    fn try_render(&mut self, target: &Target, request: &HostRequest) -> Result<SharedFrame> {
        let slot = self.slot_for(target)?;
        let response = slot.host.request(request, RENDER_TIMEOUT);
        self.count_timeout(&response);
        match response? {
            HostResponse::Frame(frame) => Ok(frame),
            HostResponse::Error { message } => Err(MmError::Internal(format!(
                "{} が描けない: {message}",
                target.plugin
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
                why = "時間切れが続いた",
                plugin = %stale.target.plugin,
                "ofx のホストを止める"
            );
            if let Err(e) = stale.host.kill() {
                tracing::warn!(%e, "ofx のホストを止められない");
            }
        }
    }

    /// そのプラグインを読み込んだホスト。無ければ起こす。先頭に置いて返す。
    fn slot_for(&mut self, target: &Target) -> Result<&mut Slot> {
        let slot = match self.slots.iter().position(|s| s.target == *target) {
            Some(i) => self.slots.remove(i),
            None => self.load(target)?,
        };
        self.slots.insert(0, slot);
        Ok(&mut self.slots[0])
    }

    fn load(&mut self, target: &Target) -> Result<Slot> {
        if self.slots.len() >= MAX_HOSTS {
            self.slots.pop();
        }
        let spec = HostSpec {
            executable: self.executable.clone(),
            args: Vec::new(),
            default_timeout: RENDER_TIMEOUT,
            gpu: None,
            client_pid: None,
        };
        let mut host = self.launcher.spawn(&spec)?;
        let prepare = HostRequest::Prepare {
            params: serde_json::json!({ "bundle": target.bundle, "plugin": target.plugin }),
        };
        match host.request(&prepare, PREPARE_TIMEOUT)? {
            HostResponse::Prepared => Ok(Slot {
                target: target.clone(),
                host,
                timeouts: 0,
            }),
            other => Err(MmError::Internal(format!(
                "{} を読み込めない: {other:?}",
                target.plugin
            ))),
        }
    }
}

impl std::fmt::Debug for Hosts {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Hosts")
            .field("launcher", &self.launcher.id())
            .field("executable", &self.executable)
            .field("live", &self.slots.len())
            .finish()
    }
}
