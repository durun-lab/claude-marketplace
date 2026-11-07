use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Mutex;

use mm_types::{ParamValue, Params, RationalTime, Size};

use super::*;
use crate::page::{Page, HTML_PARAM};

/// 起こしたホストの `gpu` と、生きているホストの数を数えるランチャー。`stalled` の間は描画が時間切れになる。
#[derive(Default)]
struct Counting {
    spawned: Mutex<Vec<Option<GpuAdapterId>>>,
    live: Arc<AtomicUsize>,
    killed: Arc<AtomicUsize>,
    stalled: Arc<AtomicBool>,
}

struct Fake {
    live: Arc<AtomicUsize>,
    killed: Arc<AtomicUsize>,
    stalled: Arc<AtomicBool>,
}

impl Drop for Fake {
    fn drop(&mut self) {
        self.live.fetch_sub(1, Ordering::SeqCst);
    }
}

impl HostProcess for Fake {
    fn is_alive(&self) -> bool {
        true
    }
    fn pid(&self) -> Option<u32> {
        None
    }
    fn request(&mut self, req: &HostRequest, timeout: Duration) -> Result<HostResponse> {
        Ok(match req {
            HostRequest::Prepare { .. } => HostResponse::Prepared,
            _ if self.stalled.load(Ordering::SeqCst) => {
                return Err(MmError::Timeout {
                    what: "fake.render".into(),
                    limit: timeout,
                })
            }
            _ => HostResponse::Frame(SharedFrame::Cpu {
                size: Size::new(1, 1),
                data: vec![0.0; 4],
            }),
        })
    }
    fn kill(&mut self) -> Result<()> {
        self.killed.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
    fn restart(&mut self) -> Result<()> {
        Ok(())
    }
    fn open_handles(&self) -> usize {
        0
    }
}

impl HostLauncher for Counting {
    fn id(&self) -> &'static str {
        "counting"
    }
    fn spawn(&self, spec: &HostSpec) -> Result<Box<dyn HostProcess>> {
        self.spawned.lock().unwrap().push(spec.gpu.clone());
        self.live.fetch_add(1, Ordering::SeqCst);
        Ok(Box::new(Fake {
            live: self.live.clone(),
            killed: self.killed.clone(),
            stalled: self.stalled.clone(),
        }))
    }
}

fn adapter(name: &str) -> GpuAdapterId {
    GpuAdapterId {
        vendor: 0x10de,
        device: 1,
        name: name.into(),
        luid: Some(1),
    }
}

fn page() -> Page {
    let mut params = Params::new();
    params.set(HTML_PARAM, ParamValue::Text("<p>x</p>".into()));
    Page::read(&params).unwrap()
}

fn request(page: &Page) -> FrameRequest<'_> {
    FrameRequest {
        page,
        time: RationalTime::ZERO,
        size: Size::new(1, 1),
        scale: 1.0,
    }
}

#[test]
fn デバイスを作り直したらホストを新しいアダプターで起こし直す() {
    let launcher = Arc::new(Counting::default());
    let mut hosts = HostPages::new(launcher.clone(), "fake".into());
    let page = page();
    let req = request(&page);
    let old = FrameTransport::Shared {
        adapter: adapter("old"),
        device: 1,
    };
    hosts.render(&req, &old).unwrap();
    hosts.render(&req, &FrameTransport::Cpu).unwrap();
    hosts.render(&req, &old).unwrap();
    assert_eq!(
        launcher.spawned.lock().unwrap().len(),
        2,
        "同じデバイスなら使い回す"
    );

    let fresh = FrameTransport::Shared {
        adapter: adapter("new"),
        device: 2,
    };
    hosts.render(&req, &fresh).unwrap();
    let spawned = launcher.spawned.lock().unwrap().clone();
    assert_eq!(spawned.last(), Some(&Some(adapter("new"))));
    assert_eq!(
        launcher.live.load(Ordering::SeqCst),
        2,
        "古いデバイスのホストは止め、CPU の経路のホストは残す"
    );
}

#[test]
fn 時間切れが続いたホストは止めて次は起こし直す() {
    let launcher = Arc::new(Counting::default());
    let mut hosts = HostPages::new(launcher.clone(), "fake".into());
    let page = page();
    let req = request(&page);
    hosts.render(&req, &FrameTransport::Cpu).unwrap();
    launcher.stalled.store(true, Ordering::SeqCst);
    for _ in 0..MAX_TIMEOUTS {
        let err = hosts.render(&req, &FrameTransport::Cpu).unwrap_err();
        assert!(matches!(err, MmError::Timeout { .. }), "{err:?}");
    }
    assert_eq!(launcher.killed.load(Ordering::SeqCst), 1, "止める");
    assert_eq!(launcher.live.load(Ordering::SeqCst), 0, "スロットを捨てる");

    launcher.stalled.store(false, Ordering::SeqCst);
    hosts.render(&req, &FrameTransport::Cpu).unwrap();
    assert_eq!(launcher.spawned.lock().unwrap().len(), 2, "起こし直す");
}

#[test]
fn 時間切れの間に描けたら数え直す() {
    let launcher = Arc::new(Counting::default());
    let mut hosts = HostPages::new(launcher.clone(), "fake".into());
    let page = page();
    let req = request(&page);
    for _ in 0..MAX_TIMEOUTS {
        launcher.stalled.store(true, Ordering::SeqCst);
        hosts.render(&req, &FrameTransport::Cpu).unwrap_err();
        launcher.stalled.store(false, Ordering::SeqCst);
        hosts.render(&req, &FrameTransport::Cpu).unwrap();
    }
    assert_eq!(launcher.killed.load(Ordering::SeqCst), 0);
    assert_eq!(launcher.spawned.lock().unwrap().len(), 1);
}
