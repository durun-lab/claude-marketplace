//! 描画の工程とエンコードの工程に分けた書き出しのテスト。フレームの間の割り込み、キューの背圧、失敗、パニック、
//! 取り消しで止まること。wgpu のテストは GPU が無ければスキップし、`MM_REQUIRE_GPU` があれば失敗にする。
use super::*;
use mm_decode_api::{EncodeReport, EncodeSettings, Encoder, Nv12Frame};
use mm_node_api::Quality;
use mm_runtime::{assemble, Backend, Engine, RuntimeConfig};
use mm_testkit::decode::{frame_index_of, SampleMedia};
use mm_testkit::fixtures::vertical_project;
use mm_types::{CompositionId, Id, RationalTime, RgbaImage};
use passes::export_frames_interleaved;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;

/// 止まらずに戻るかを見る上限。書き出し自体は 1 秒もかからない。
const HANG_LIMIT: Duration = Duration::from_secs(60);
/// 描画の工程がエンコードの工程より先に進める最大のフレーム数（先行するリードバック 3、キュー 4、エンコード中の 1）。
const MAX_AHEAD: usize = 3 + 4 + 1;
/// 割り込んだ描画が描く時刻を、このフレーム数で巡らせる（書き出しの時刻と別の時刻にデコーダーをシークさせる）。
const STILL_CYCLE: usize = 7;

fn temp(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("mm_export_stages_{name}_{}", std::process::id()))
}

fn settings_of(media: &SampleMedia) -> EncodeSettings {
    EncodeSettings {
        size: media.size,
        fps: media.fps,
        codec: "sample".into(),
        output_path: temp("unused.json"),
        audio: None,
        stream: None,
        source_peak: 1.0,
    }
}

/// 送られたフレームの中身（全画素の値）を残し、`delay` だけ遅らせ、`fail_at` 回目の送信で失敗し、
/// `panic_at` 回目の送信でパニックする。`nv12` なら NV12 を受ける。
#[derive(Default)]
struct Probe {
    frames: Arc<Mutex<Vec<Vec<u32>>>>,
    pushed: Arc<AtomicUsize>,
    delay: Duration,
    fail_at: Option<usize>,
    panic_at: Option<usize>,
    nv12: bool,
}

impl Probe {
    fn record(&mut self, frames: Vec<Vec<u32>>) -> Result<()> {
        std::thread::sleep(self.delay);
        let n = self.pushed.load(Ordering::SeqCst) + 1;
        if self.fail_at == Some(n) {
            return Err(MmError::Corrupt(format!("push {n}")));
        }
        assert_ne!(self.panic_at, Some(n), "エンコーダーのパニック");
        self.pushed.fetch_add(frames.len(), Ordering::SeqCst);
        self.frames.lock().unwrap().extend(frames);
        Ok(())
    }
}

impl Encoder for Probe {
    fn begin(&mut self, _settings: &EncodeSettings) -> Result<()> {
        Ok(())
    }

    fn push_frames(&mut self, frames: &[RgbaImage]) -> Result<()> {
        let bits = |f: &RgbaImage| f.data().iter().map(|v| v.to_bits()).collect();
        self.record(frames.iter().map(bits).collect())
    }

    fn push_nv12_frames(&mut self, frames: &[Nv12Frame]) -> Result<()> {
        if !self.nv12 {
            return Err(MmError::Unsupported("NV12 を受けない".into()));
        }
        if frames.is_empty() {
            return Ok(());
        }
        let bytes = |f: &Nv12Frame| f.y.iter().chain(&f.uv).map(|&b| u32::from(b)).collect();
        self.record(frames.iter().map(bytes).collect())
    }

    fn push_audio(&mut self, _block: &mm_audio_api::AudioBlock) -> Result<()> {
        Ok(())
    }

    fn finish(&mut self) -> Result<EncodeReport> {
        Err(MmError::Internal("このテストでは閉じない".into()))
    }
}

/// 縦の 1 本の区間を `engine` で描いて `probe` に書き出す。`between` はフレームの間に呼ぶ。
fn run_on(
    engine: &mut Engine,
    probe: &mut Probe,
    control: &ExportControl,
    composition: Option<CompositionId>,
    between: &mut dyn FnMut(&mut Engine),
) -> Result<usize> {
    let media = SampleMedia::default();
    let (project, comp, _) = vertical_project(&media, &temp("unused.json"));
    let range = project.composition(&comp).unwrap().range().unwrap();
    let composition = composition.unwrap_or(comp);
    let plan = Plan {
        target: &Id::new("mp4"),
        composition: &composition,
        range,
        control,
        parallel: 0,
        parallel_min_frames: None,
        pass: PassPosition { index: 1, of: 1 },
    };
    let settings = settings_of(&media);
    export_frames_interleaved(engine, &project, &plan, &settings, probe, between)
}

/// `run_on` をモックのレンダラーで。
fn run(
    probe: &mut Probe,
    control: &ExportControl,
    composition: Option<CompositionId>,
    between: &mut dyn FnMut(&mut Engine),
) -> Result<usize> {
    let mut engine = assemble(&RuntimeConfig::sample()).unwrap();
    run_on(&mut engine, probe, control, composition, between)
}

/// wgpu のレンダラーとモックのデコーダー。GPU が無ければ `None`（`MM_REQUIRE_GPU` があれば失敗にする）。
fn gpu_engine() -> Option<Engine> {
    let config = RuntimeConfig {
        render: Backend::Wgpu,
        decode: Backend::Sample,
        encode: Backend::Sample,
        ..RuntimeConfig::real()
    };
    match assemble(&config) {
        Ok(engine) => Some(engine),
        Err(e) if std::env::var_os("MM_REQUIRE_GPU").is_some() => {
            panic!("MM_REQUIRE_GPU が有るのに GPU を作れない: {e}")
        }
        Err(_) => None,
    }
}

/// `f` を別のスレッドで走らせ、`HANG_LIMIT` までに戻らなければ失敗にする。パニックしたら `Err`。
fn within_limit<T: Send + 'static>(
    f: impl FnOnce() -> T + Send + 'static,
) -> std::thread::Result<T> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(catch_unwind(AssertUnwindSafe(f)));
    });
    rx.recv_timeout(HANG_LIMIT)
        .expect("書き出しが止まったまま戻らない")
}

/// `GET /frame` の代わりに、書き出しと別の時刻の静止画を同じエンジンで描き、左上の画素のフレーム番号を返す。
fn still(engine: &mut Engine, n: usize) -> i64 {
    let media = SampleMedia::default();
    let (project, comp, _) = vertical_project(&media, &temp("unused.json"));
    let t = RationalTime::from_frames((n % STILL_CYCLE) as i64, media.fps).unwrap();
    let image = engine
        .render_frame(&project, &comp, t, media.size, Quality::Full)
        .unwrap();
    frame_index_of(image.pixel(0, 0))
}

#[test]
fn フレームの間に別の描画を挟んでも書き出すフレームは同じ() {
    let mut plain = Probe::default();
    let frames = run(&mut plain, &ExportControl::default(), None, &mut |_| {}).unwrap();

    let mut stills = Vec::new();
    let mut interleaved = Probe::default();
    let n = run(
        &mut interleaved,
        &ExportControl::default(),
        None,
        &mut |engine: &mut Engine| stills.push(still(engine, stills.len())),
    )
    .unwrap();
    assert_eq!(n, frames);
    assert_eq!(
        stills.len(),
        frames,
        "フレームを 1 つ描くごとに 1 回エンジンを渡す"
    );
    let mut alone = assemble(&RuntimeConfig::sample()).unwrap();
    let expected: Vec<i64> = (0..frames).map(|i| still(&mut alone, i)).collect();
    assert_eq!(
        stills, expected,
        "割り込んだ描画は、書き出しの無いときと同じフレーム"
    );
    assert!(
        *plain.frames.lock().unwrap() == *interleaved.frames.lock().unwrap(),
        "割り込みの有無で、送ったフレームの画素が 1 ビットも変わらない"
    );
}

/// wgpu のレンダラーで、割り込みとキャッシュの破棄を挟んでも送るフレームが 1 ビットも変わらないこと。
fn wgpu_interleave_keeps_frames(nv12: bool) {
    let Some(mut engine) = gpu_engine() else {
        return;
    };
    let mut plain = Probe {
        nv12,
        ..Probe::default()
    };
    let frames = run_on(
        &mut engine,
        &mut plain,
        &ExportControl::default(),
        None,
        &mut |_| {},
    )
    .unwrap();
    engine.invalidate_touched(&["clip".into()]);
    let mut interleaved = Probe {
        nv12,
        ..Probe::default()
    };
    let mut n = 0;
    let sent = run_on(
        &mut engine,
        &mut interleaved,
        &ExportControl::default(),
        None,
        &mut |e: &mut Engine| {
            still(e, n);
            if n % 3 == 0 {
                e.invalidate_touched(&["clip".into()]);
            }
            n += 1;
        },
    )
    .unwrap();
    assert_eq!(sent, frames);
    assert_eq!(n, frames, "フレームを 1 つ描くごとに 1 回エンジンを渡す");
    assert!(
        *plain.frames.lock().unwrap() == *interleaved.frames.lock().unwrap(),
        "割り込みの有無で、送ったフレームが 1 ビットも変わらない（NV12: {nv12}）"
    );
}

#[test]
fn wgpu_の_rgba_の書き出しはフレームの間の割り込みで変わらない() {
    wgpu_interleave_keeps_frames(false);
}

#[test]
fn wgpu_の_nv12_の書き出しはフレームの間の割り込みで変わらない() {
    wgpu_interleave_keeps_frames(true);
}

#[test]
fn エンコードが遅いと描画の工程はキューの長さまでしか先に進まない() {
    let pushed = Arc::new(AtomicUsize::new(0));
    let ahead = Arc::new(AtomicUsize::new(0));
    let (p, a) = (pushed.clone(), ahead.clone());
    let n = within_limit(move || {
        let mut probe = Probe {
            pushed: p.clone(),
            delay: Duration::from_millis(3),
            ..Probe::default()
        };
        let mut drawn = 0;
        run(&mut probe, &ExportControl::default(), None, &mut |_| {
            drawn += 1;
            a.fetch_max(drawn - p.load(Ordering::SeqCst), Ordering::SeqCst);
        })
    })
    .unwrap()
    .unwrap();
    assert_eq!(pushed.load(Ordering::SeqCst), n, "全部のフレームを送る");
    let max = ahead.load(Ordering::SeqCst);
    assert!(
        (2..=MAX_AHEAD).contains(&max),
        "描画の工程はエンコードの工程より先に進むが、{MAX_AHEAD} フレームを超えない: {max}"
    );
}

#[test]
fn キューが満ちたままエンコードが失敗しても描画の工程は止まってそのエラーを返す() {
    let drawn = Arc::new(AtomicUsize::new(0));
    let d = drawn.clone();
    let (result, pushed) = within_limit(move || {
        let mut probe = Probe {
            delay: Duration::from_millis(20),
            fail_at: Some(3),
            ..Probe::default()
        };
        let result = run(&mut probe, &ExportControl::default(), None, &mut |_| {
            d.fetch_add(1, Ordering::SeqCst);
        });
        (result, probe.pushed.load(Ordering::SeqCst))
    })
    .unwrap();
    let err = result.expect_err("失敗する");
    assert!(
        matches!(&err, MmError::Corrupt(m) if m == "push 3"),
        "エンコードの工程のエラー: {err}"
    );
    assert_eq!(pushed, 2);
    assert!(
        drawn.load(Ordering::SeqCst) <= 2 + MAX_AHEAD,
        "失敗の後は描き続けない"
    );
}

#[test]
fn エンコーダーがパニックしたら描画の工程は止まってエラーを返す() {
    let result = within_limit(|| {
        let mut probe = Probe {
            delay: Duration::from_millis(5),
            panic_at: Some(3),
            ..Probe::default()
        };
        run(&mut probe, &ExportControl::default(), None, &mut |_| {})
    })
    .expect("パニックは呼んだ側へ伝えず、エラーにする");
    let err = result.expect_err("失敗する");
    assert!(err.to_string().contains("パニック"), "{err}");
}

#[test]
fn フレームの間の割り込みがパニックしても待ち続けずにパニックを伝える() {
    let result = within_limit(|| {
        let mut probe = Probe {
            delay: Duration::from_millis(5),
            ..Probe::default()
        };
        let mut n = 0;
        run(&mut probe, &ExportControl::default(), None, &mut |_| {
            n += 1;
            assert_ne!(n, 2, "割り込んだ仕事のパニック");
        })
    });
    assert!(result.is_err(), "描画の工程のパニックは呼んだ側へ伝わる");
}

#[test]
fn 描画の工程が失敗したらエンコードの工程は何も送らずに止まる() {
    let (result, pushed) = within_limit(|| {
        let mut probe = Probe::default();
        let missing = Some(Id::new("無いコンポジション"));
        let result = run(&mut probe, &ExportControl::default(), missing, &mut |_| {});
        (result, probe.pushed.load(Ordering::SeqCst))
    })
    .unwrap();
    let err = result.expect_err("描けない");
    assert!(
        !err.to_string().contains("止まった"),
        "相手の工程が止まった旨ではなく、描画の工程のエラーを返す: {err}"
    );
    assert_eq!(pushed, 0);
}

#[test]
fn キューが満ちていても取り消すと戻る() {
    let flag = Arc::new(AtomicBool::new(false));
    let f = flag.clone();
    let result = within_limit(move || {
        let mut probe = Probe {
            delay: Duration::from_millis(5),
            ..Probe::default()
        };
        let control = ExportControl::default()
            .with_cancel(f.clone())
            .with_progress(Arc::new(move |p| {
                if p.frames_done == 10 {
                    f.store(true, Ordering::Relaxed);
                }
            }));
        run(&mut probe, &control, None, &mut |_| {})
    })
    .unwrap();
    let err = result.expect_err("取り消す");
    assert!(is_cancel_error(&err), "取り消しのエラー: {err}");
    assert!(flag.load(Ordering::Relaxed));
}
