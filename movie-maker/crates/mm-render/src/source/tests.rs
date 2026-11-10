use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use mm_decode_api::{DecodedFrame, Decoder, DecoderFactory, GpuContextRef, MediaInfo};
use mm_testkit::decode::{SampleDecoderFactory, SampleMedia};
use mm_testkit::fixtures::vertical_project;
use mm_types::{AssetId, GpuAdapterId, Id, ParamValue, Params, RationalTime, Result, TimeRange};

use super::*;

/// 開いた数とシークの数を数えるモックのデコーダー。
#[derive(Default)]
struct Counts {
    opens: AtomicUsize,
    seeks: AtomicUsize,
    /// 開き始めた数（`Gated` が止める前に数える）。
    started: AtomicUsize,
}

struct Counting {
    inner: SampleDecoderFactory,
    counts: Arc<Counts>,
}

impl DecoderFactory for Counting {
    fn id(&self) -> &'static str {
        "counting"
    }

    fn open(&self, path: &Path, gpu: Option<&dyn GpuContextRef>) -> Result<Box<dyn Decoder>> {
        self.counts.opens.fetch_add(1, Ordering::SeqCst);
        Ok(Box::new(CountingDecoder {
            inner: self.inner.open(path, gpu)?,
            counts: self.counts.clone(),
        }))
    }
}

struct CountingDecoder {
    inner: Box<dyn Decoder>,
    counts: Arc<Counts>,
}

impl Decoder for CountingDecoder {
    fn info(&self) -> &MediaInfo {
        self.inner.info()
    }

    fn seek(&mut self, t: RationalTime) -> Result<()> {
        self.counts.seeks.fetch_add(1, Ordering::SeqCst);
        self.inner.seek(t)
    }

    fn next_frame(&mut self) -> Result<Option<DecodedFrame>> {
        self.inner.next_frame()
    }

    fn decode_audio(&mut self, range: TimeRange) -> Result<AudioBlock> {
        self.inner.decode_audio(range)
    }
}

struct NoGpu(GpuAdapterId);

impl GpuContextRef for NoGpu {
    fn adapter(&self) -> &GpuAdapterId {
        &self.0
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

/// 試験が `release` を送るまで開くのを止めるデコーダー。長い録画の素材の、索引を作る間の代わり。
struct Gated {
    inner: Counting,
    gate: Mutex<Receiver<()>>,
}

impl DecoderFactory for Gated {
    fn id(&self) -> &'static str {
        "gated"
    }

    fn open(&self, path: &Path, gpu: Option<&dyn GpuContextRef>) -> Result<Box<dyn Decoder>> {
        self.inner.counts.started.fetch_add(1, Ordering::SeqCst);
        let _ = self.gate.lock().expect("gate").recv();
        self.inner.open(path, gpu)
    }
}

struct Rig {
    pool: SourcePool,
    opener: Opener,
    counts: Arc<Counts>,
    project: Project,
    params: Params,
    media: SampleMedia,
}

fn counting(media: &SampleMedia, counts: &Arc<Counts>) -> Counting {
    Counting {
        inner: SampleDecoderFactory {
            media: media.clone(),
        },
        counts: counts.clone(),
    }
}

fn rig_with(factory: impl FnOnce(Counting) -> Arc<dyn DecoderFactory>) -> Rig {
    let media = SampleMedia::default();
    let path = std::env::temp_dir().join("mm_render_source_pool.json");
    let (project, _, _) = vertical_project(&media, &path);
    let asset: AssetId = Id::new("clip");
    let mut params = Params::default();
    params.0.insert("asset".into(), ParamValue::AssetRef(asset));
    let counts = Arc::new(Counts::default());
    let gpu = NoGpu(GpuAdapterId {
        vendor: 0,
        device: 0,
        name: "なし".into(),
        luid: None,
    });
    Rig {
        pool: SourcePool::default(),
        opener: Opener {
            decoders: factory(counting(&media, &counts)),
            gpu: Arc::new(gpu),
        },
        counts,
        project,
        params,
        media,
    }
}

fn rig() -> Rig {
    rig_with(|c| Arc::new(c))
}

/// 開くのを止めるデコーダーの試験台と、開くのを許す送り手。
fn gated_rig(wait: Duration) -> (Rig, Sender<()>) {
    let (release, gate) = mpsc::channel();
    let mut r = rig_with(|inner| {
        Arc::new(Gated {
            inner,
            gate: Mutex::new(gate),
        })
    });
    r.pool.set_open_wait(Some(wait));
    (r, release)
}

impl Rig {
    fn at(&self, i: i64) -> RationalTime {
        RationalTime::from_frames(i, self.media.fps).expect("表せる")
    }

    /// 1 フレームの中で、素材の番号 `indices` を順に読む。
    fn frame(&mut self, indices: &[i64]) -> Vec<i64> {
        self.pool.begin_frame();
        indices
            .iter()
            .map(|&i| {
                let t = self.at(i);
                let source = self
                    .pool
                    .footage_for(&self.opener, &self.project, &self.params, t)
                    .expect("開ける")
                    .expect("素材がある");
                source.frame_at(t).expect("読める").index
            })
            .collect()
    }

    /// 新しいフレームで素材を 1 回求め、開いている途中か（`true`）と、かかった時間を返す。
    fn ask(&mut self) -> (bool, Duration) {
        self.pool.begin_frame();
        let started = Instant::now();
        let t = self.at(0);
        let footage = self
            .pool
            .footage_with_backend(&self.opener, &self.project, &self.params, t)
            .expect("開ける");
        (matches!(footage, Footage::Pending), started.elapsed())
    }

    /// 裏のスレッドで開き終わった数が `n` になるまで待つ。
    fn wait_opened(&self, n: u64) {
        let pending = self.pool.pending();
        let deadline = Instant::now() + Duration::from_secs(10);
        while pending.opened() < n {
            assert!(Instant::now() < deadline, "裏のスレッドが開き終わらない");
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    fn seeks(&self) -> usize {
        self.counts.seeks.load(Ordering::SeqCst)
    }

    fn opens(&self) -> usize {
        self.counts.opens.load(Ordering::SeqCst)
    }
}

#[test]
fn 続けて読むフレームはシークしない() {
    let mut r = rig();
    for i in 0..30 {
        assert_eq!(r.frame(&[i]), vec![i]);
    }
    assert_eq!((r.opens(), r.seeks()), (1, 1), "最初の 1 回だけシークする");
}

#[test]
fn 同じ素材を時刻をずらして重ねても_それぞれのデコーダーが読み進める() {
    let mut r = rig();
    for i in 0..30 {
        assert_eq!(r.frame(&[i, i + 40]), vec![i, i + 40]);
    }
    assert_eq!(r.opens(), 2, "重ねた数だけ開く");
    assert_eq!(r.seeks(), 2, "それぞれ最初の 1 回だけシークする");
}

#[test]
fn 入れ子を順に並べた素材の頭出しは開き直さずシークで済ませる() {
    let mut r = rig();
    for _ in 0..3 {
        for i in 0..10 {
            r.frame(&[i]);
        }
    }
    assert_eq!(r.opens(), 1);
    assert_eq!(r.seeks(), 3, "先頭に戻るたびに 1 回");
}

#[test]
fn 重ねる数が上限を超えたら最も前に使ったデコーダーをシークし直す() {
    let mut r = rig();
    let layers = MAX_DECODERS_PER_ASSET as i64 + 1;
    let at = |i: i64| (0..layers).map(|k| i + k * 20).collect::<Vec<_>>();
    for i in 0..5 {
        assert_eq!(r.frame(&at(i)), at(i));
    }
    assert_eq!(r.opens(), MAX_DECODERS_PER_ASSET);
}

#[test]
fn 長く使わないデコーダーは閉じ_次に使うときに開き直す() {
    let mut r = rig();
    r.frame(&[0]);
    for _ in 0..IDLE_FRAMES * 2 {
        r.pool.begin_frame();
    }
    assert!(
        r.pool.sources.is_empty(),
        "使わない素材のデコーダーを閉じた"
    );
    assert_eq!(r.frame(&[1]), vec![1]);
    assert_eq!(r.opens(), 2, "閉じた素材は開き直す");
}

#[test]
fn 使い続けるデコーダーは閉じない() {
    let mut r = rig();
    for i in 0..(IDLE_FRAMES as i64 * 2) {
        r.frame(&[i % 30]);
    }
    assert_eq!(r.opens(), 1);
}

#[test]
fn 待つ上限までに開き終わらない素材は待たずに返し_開き終わったら次のフレームで受け取る() {
    let wait = Duration::from_millis(20);
    let (mut r, release) = gated_rig(wait);
    let (pending, took) = r.ask();
    assert!(pending, "開いている途中を返す");
    assert!(
        took < Duration::from_secs(2),
        "開き終わるのを待たない: {took:?}"
    );
    assert_eq!(r.pool.pending_layers(), 1, "描かなかったレイヤーを数える");

    release.send(()).expect("開くのを許す");
    r.wait_opened(1);
    let (pending, _) = r.ask();
    assert!(!pending, "開き終わったデコーダーを受け取る");
    assert_eq!(r.pool.pending_layers(), 0, "フレームの始めに数え直す");
    assert_eq!(r.opens(), 1, "裏で開いたものを使い、開き直さない");
}

#[test]
fn 待つのは開き始めたフレームだけで_後のフレームは開き終わったかを見るだけ() {
    let wait = Duration::from_millis(300);
    let (mut r, release) = gated_rig(wait);
    let (pending, first) = r.ask();
    assert!(
        pending && first >= wait,
        "開き始めたフレームは上限まで待つ: {first:?}"
    );
    let (pending, later) = r.ask();
    assert!(pending, "まだ開いている途中");
    assert!(later < wait / 2, "後のフレームは待たない: {later:?}");
    release.send(()).expect("開くのを許す");
    r.wait_opened(1);
}

#[test]
fn 開いている途中の素材を次のフレームで求めても_もう一度は開かない() {
    let (mut r, release) = gated_rig(Duration::from_millis(5));
    assert!(r.ask().0);
    assert!(r.ask().0, "まだ開いている途中");
    std::thread::sleep(Duration::from_millis(50));
    assert_eq!(
        r.counts.started.load(Ordering::SeqCst),
        1,
        "開き始めたのは 1 回"
    );
    release.send(()).expect("開くのを許す");
    r.wait_opened(1);
    assert!(!r.ask().0);
    assert_eq!(r.opens(), 1);
}

#[test]
fn 待つ上限の中で開き終わる素材はそのフレームで受け取る() {
    let mut r = rig();
    r.pool.set_open_wait(Some(Duration::from_secs(5)));
    assert!(!r.ask().0, "上限の中で開き終わる");
    assert_eq!((r.opens(), r.pool.pending_layers()), (1, 0));
}

#[test]
fn 素材を忘れたら開いている途中のものも捨て_次に求めたら開き直す() {
    let (mut r, release) = gated_rig(Duration::from_millis(5));
    assert!(r.ask().0);
    r.pool.forget(&["clip".to_owned()]);
    release.send(()).expect("1 回目を許す");
    r.wait_opened(1);
    assert!(r.ask().0, "捨てた結果は使わず、開き直す");
    release.send(()).expect("2 回目を許す");
    r.wait_opened(2);
    assert!(!r.ask().0);
    assert_eq!(r.opens(), 2);
}
