//! ホストの時間切れとパニックの後の扱い。CEF の代わりに、決めた通りに応答する偽のホストを使う。

use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use mm_host_api::{HostLauncher, HostProcess, HostRequest, HostResponse, HostSpec, SharedFrame};
use mm_text_api::{TextRasterizer, TextShaper, TextStyle};
use mm_text_cef::{CefRasterizer, MAX_TIMEOUTS};
use mm_text_glyphon::TextEngine;
use mm_types::{MmError, Result, Rgba, RgbaImage};

/// 偽のホストの振る舞い。`stalled` の間は描画が時間切れ、`panics` の間はパニックする。
#[derive(Default)]
struct Behavior {
    spawned: AtomicUsize,
    killed: AtomicUsize,
    stalled: AtomicBool,
    panics: AtomicBool,
}

struct Launcher(Arc<Behavior>);
struct Host(Arc<Behavior>);

impl HostProcess for Host {
    fn is_alive(&self) -> bool {
        true
    }
    fn pid(&self) -> Option<u32> {
        None
    }
    fn request(&mut self, req: &HostRequest, timeout: Duration) -> Result<HostResponse> {
        let HostRequest::RenderFrame { size, .. } = req else {
            return Ok(HostResponse::Prepared);
        };
        assert!(
            !self.0.panics.load(Ordering::SeqCst),
            "応答を読む途中のパニック"
        );
        if self.0.stalled.load(Ordering::SeqCst) {
            return Err(MmError::Timeout {
                what: "fake.render".into(),
                limit: timeout,
            });
        }
        let image = RgbaImage::filled(*size, Rgba::WHITE);
        Ok(HostResponse::Frame(SharedFrame::from_image(&image)))
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

impl HostLauncher for Launcher {
    fn id(&self) -> &'static str {
        "fake"
    }
    fn spawn(&self, _spec: &HostSpec) -> Result<Box<dyn HostProcess>> {
        self.0.spawned.fetch_add(1, Ordering::SeqCst);
        Ok(Box::new(Host(self.0.clone())))
    }
}

fn setup() -> (Arc<Behavior>, CefRasterizer) {
    let behavior = Arc::new(Behavior::default());
    let raster = CefRasterizer::new(Arc::new(Launcher(behavior.clone())), "fake".into());
    (behavior, raster)
}

fn draw(raster: &CefRasterizer) -> Result<RgbaImage> {
    let style = TextStyle::default();
    let shaped = TextEngine::new().shaper().shape("abc", &style)?;
    raster.rasterize(&shaped, &style, 1.0)
}

#[test]
fn 時間切れが続いたホストは止めて次は起こし直す() {
    let (behavior, raster) = setup();
    draw(&raster).expect("描ける");
    behavior.stalled.store(true, Ordering::SeqCst);
    for _ in 0..MAX_TIMEOUTS {
        let err = draw(&raster).unwrap_err();
        assert!(matches!(err, MmError::Timeout { .. }), "{err:?}");
    }
    assert_eq!(behavior.killed.load(Ordering::SeqCst), 1, "止める");

    behavior.stalled.store(false, Ordering::SeqCst);
    draw(&raster).expect("起こし直して描ける");
    assert_eq!(behavior.spawned.load(Ordering::SeqCst), 2);
}

#[test]
fn パニックの後はホストを捨てて描ける() {
    let (behavior, raster) = setup();
    behavior.panics.store(true, Ordering::SeqCst);
    let first = catch_unwind(AssertUnwindSafe(|| draw(&raster)));
    assert!(first.is_err(), "パニックする");

    behavior.panics.store(false, Ordering::SeqCst);
    draw(&raster).expect("毒の入ったロックでも描ける");
    assert_eq!(
        behavior.spawned.load(Ordering::SeqCst),
        2,
        "捨てて起こし直す"
    );
}
