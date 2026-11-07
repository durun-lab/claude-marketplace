//! ホストの描画の途中でパニックしても、以後のフレームを描けることを確かめる。

use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use mm_host_api::{HostLauncher, HostProcess, HostRequest, HostResponse, HostSpec, SharedFrame};
use mm_node_api::{NoInputs, NodeKind};
use mm_nodes_cef::{Stats, WebCefKind, HTML_PARAM};
use mm_testkit::node::Harness;
use mm_types::{ParamValue, Params, RationalTime, Result, Rgba, RgbaImage, Size};

const SIZE: Size = Size::new(8, 8);

/// 最初に起こしたホストだけが描画の途中でパニックするランチャー。
#[derive(Default)]
struct PanicOnce {
    spawned: AtomicUsize,
}

struct Host {
    panics: bool,
}

impl HostProcess for Host {
    fn is_alive(&self) -> bool {
        true
    }
    fn pid(&self) -> Option<u32> {
        None
    }
    fn request(&mut self, req: &HostRequest, _timeout: Duration) -> Result<HostResponse> {
        match req {
            HostRequest::RenderFrame { size, .. } => {
                assert!(!self.panics, "ホストの応答を読む途中のパニック");
                let image = RgbaImage::filled(*size, Rgba::WHITE);
                Ok(HostResponse::Frame(SharedFrame::from_image(&image)))
            }
            _ => Ok(HostResponse::Prepared),
        }
    }
    fn kill(&mut self) -> Result<()> {
        Ok(())
    }
    fn restart(&mut self) -> Result<()> {
        Ok(())
    }
    fn open_handles(&self) -> usize {
        0
    }
}

impl HostLauncher for PanicOnce {
    fn id(&self) -> &'static str {
        "panic-once"
    }
    fn spawn(&self, _spec: &HostSpec) -> Result<Box<dyn HostProcess>> {
        let first = self.spawned.fetch_add(1, Ordering::SeqCst) == 0;
        Ok(Box::new(Host { panics: first }))
    }
}

fn render(kind: &WebCefKind) -> Result<Arc<RgbaImage>> {
    let mut p = Params::new();
    p.set(HTML_PARAM, ParamValue::Text("<p>x</p>".into()));
    let p = p.with_defaults(&kind.params());
    let mut h = Harness::new(None);
    h.render(kind, &p, RationalTime::ZERO, SIZE, 1.0, &mut NoInputs)
        .map(|(image, _)| image)
}

#[test]
fn パニックの後はホストを起こし直して描ける() {
    let launcher = Arc::new(PanicOnce::default());
    let kind = WebCefKind::with_host(launcher.clone(), "fake".into());
    let first = catch_unwind(AssertUnwindSafe(|| render(&kind)));
    assert!(first.is_err(), "最初のホストはパニックする");

    let image = render(&kind).expect("毒の入ったロックでも描ける");
    assert!(image.pixel(0, 0).a > 0.99);
    assert_eq!(
        launcher.spawned.load(Ordering::SeqCst),
        2,
        "ホストは捨てて起こし直す"
    );
    assert_eq!(kind.stats(), Stats { drawn: 1, hits: 0 });
}
