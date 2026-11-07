use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Mutex;

use mm_types::{ParamValue, Params, RationalTime, Size};

use super::*;
use crate::page::{Page, HTML_PARAM};

/// 偽のホストが共有する記録。`stalled` の間は描画が時間切れになる。
#[derive(Default)]
struct State {
    live: AtomicUsize,
    killed: AtomicUsize,
    stalled: AtomicBool,
    prepared: Mutex<Vec<serde_json::Value>>,
}

/// 起こしたホストの `gpu` を記録し、生きているホストの数を数えるランチャー。
#[derive(Default)]
struct Counting {
    spawned: Mutex<Vec<Option<GpuAdapterId>>>,
    state: Arc<State>,
}

struct Fake(Arc<State>);

impl Drop for Fake {
    fn drop(&mut self) {
        self.0.live.fetch_sub(1, Ordering::SeqCst);
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
            HostRequest::Prepare { params } => {
                self.0.prepared.lock().unwrap().push(params.clone());
                HostResponse::Prepared
            }
            _ if self.0.stalled.load(Ordering::SeqCst) => {
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
        self.0.killed.fetch_add(1, Ordering::SeqCst);
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
        self.state.live.fetch_add(1, Ordering::SeqCst);
        Ok(Box::new(Fake(self.state.clone())))
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
    Page::read_with(&params, false).unwrap()
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
fn 書体の組を_prepare_で渡し_組が変わったら読み直す() {
    let launcher = Arc::new(Counting::default());
    let mut hosts = HostPages::new(launcher.clone(), "fake".into());
    let page = page();
    let font = FontFile::from_asset("p/fonts/A B/a.ttf", "h1");
    hosts.set_fonts(std::slice::from_ref(&font));
    hosts.render(&request(&page), &FrameTransport::Cpu).unwrap();
    hosts.set_fonts(std::slice::from_ref(&font));
    hosts.render(&request(&page), &FrameTransport::Cpu).unwrap();
    hosts.set_fonts(&[]);
    hosts.render(&request(&page), &FrameTransport::Cpu).unwrap();
    let prepared = launcher.state.prepared.lock().unwrap().clone();
    assert_eq!(prepared.len(), 2, "同じ組なら読み直さない");
    assert_eq!(prepared[0]["fonts"][0]["content_hash"], "h1");
    assert_eq!(prepared[0]["fonts"][0]["family"], "A B");
    assert_eq!(prepared[1]["fonts"], serde_json::json!([]));
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
        launcher.state.live.load(Ordering::SeqCst),
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
    launcher.state.stalled.store(true, Ordering::SeqCst);
    for _ in 0..MAX_TIMEOUTS {
        let err = hosts.render(&req, &FrameTransport::Cpu).unwrap_err();
        assert!(matches!(err, MmError::Timeout { .. }), "{err:?}");
    }
    assert_eq!(launcher.state.killed.load(Ordering::SeqCst), 1, "止める");
    assert_eq!(
        launcher.state.live.load(Ordering::SeqCst),
        0,
        "スロットを捨てる"
    );

    launcher.state.stalled.store(false, Ordering::SeqCst);
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
        launcher.state.stalled.store(true, Ordering::SeqCst);
        hosts.render(&req, &FrameTransport::Cpu).unwrap_err();
        launcher.state.stalled.store(false, Ordering::SeqCst);
        hosts.render(&req, &FrameTransport::Cpu).unwrap();
    }
    assert_eq!(launcher.state.killed.load(Ordering::SeqCst), 0);
    assert_eq!(launcher.spawned.lock().unwrap().len(), 1);
}
