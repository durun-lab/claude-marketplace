//! 描画の段と符号化の段に分けた書き出しのテスト。フレームの間の割り込み、キューの背圧、失敗と取り消しで止まること。
use super::*;
use mm_decode_api::{EncodeReport, EncodeSettings, Encoder};
use mm_node_api::Quality;
use mm_runtime::{assemble, Engine, RuntimeConfig};
use mm_testkit::decode::{frame_index_of, SampleMedia};
use mm_testkit::fixtures::vertical_project;
use mm_types::{CompositionId, Id, RationalTime, RgbaImage};
use passes::export_frames_interleaved;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;

/// 止まらずに戻るかを見る上限。書き出し自体は 1 秒もかからない。
const HANG_LIMIT: Duration = Duration::from_secs(60);
/// 描画の段が符号化の段より先に進める最大のフレーム数（先行するリードバック 3、キュー 4、符号化中の 1）。
const MAX_AHEAD: usize = 3 + 4 + 1;

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

/// 押されたフレームの中身（全画素の値）を残し、`delay` だけ遅らせ、`fail_at` 回目の送信で失敗する。
#[derive(Default)]
struct Probe {
    frames: Arc<Mutex<Vec<Vec<u32>>>>,
    pushed: Arc<AtomicUsize>,
    delay: Duration,
    fail_at: Option<usize>,
}

impl Encoder for Probe {
    fn begin(&mut self, _settings: &EncodeSettings) -> Result<()> {
        Ok(())
    }

    fn push_frames(&mut self, frames: &[RgbaImage]) -> Result<()> {
        std::thread::sleep(self.delay);
        let n = self.pushed.load(Ordering::SeqCst) + 1;
        if self.fail_at == Some(n) {
            return Err(MmError::Corrupt(format!("push {n}")));
        }
        let mut seen = self.frames.lock().unwrap();
        seen.extend(
            frames
                .iter()
                .map(|f| f.data().iter().map(|v| v.to_bits()).collect()),
        );
        self.pushed.fetch_add(frames.len(), Ordering::SeqCst);
        Ok(())
    }

    fn push_audio(&mut self, _block: &mm_audio_api::AudioBlock) -> Result<()> {
        Ok(())
    }

    fn finish(&mut self) -> Result<EncodeReport> {
        Err(MmError::Internal("このテストでは閉じない".into()))
    }
}

/// 縦の 1 本の区間を `probe` に書き出す。`between` はフレームの間に呼ぶ。
fn run(
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
    let mut engine = assemble(&RuntimeConfig::sample()).unwrap();
    let settings = settings_of(&media);
    export_frames_interleaved(&mut engine, &project, &plan, &settings, probe, between)
}

/// `f` を別のスレッドで走らせ、`HANG_LIMIT` までに戻らなければ失敗にする。
fn within_limit<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> T {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(f());
    });
    rx.recv_timeout(HANG_LIMIT)
        .expect("書き出しが止まったまま戻らない")
}

#[test]
fn フレームの間に別の描画を挟んでも書き出すフレームは同じ() {
    let mut plain = Probe::default();
    let frames = run(&mut plain, &ExportControl::default(), None, &mut |_| {}).unwrap();

    let media = SampleMedia::default();
    let (project, comp, _) = vertical_project(&media, &temp("unused.json"));
    // GET /frame の代わりに、書き出しと別の時刻の静止画を同じエンジンで描く
    let still = |engine: &mut Engine, n: usize| {
        let t = RationalTime::from_frames((n % 7) as i64, media.fps).unwrap();
        let image = engine
            .render_frame(&project, &comp, t, media.size, Quality::Full)
            .unwrap();
        frame_index_of(image.pixel(0, 0))
    };
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
        "フレームを 1 つ描くごとに 1 回エンジンを貸す"
    );
    let mut alone = assemble(&RuntimeConfig::sample()).unwrap();
    let expected: Vec<i64> = (0..frames).map(|i| still(&mut alone, i)).collect();
    assert_eq!(
        stills, expected,
        "割り込んだ描画は、書き出しの無いときと同じフレーム"
    );
    assert!(
        *plain.frames.lock().unwrap() == *interleaved.frames.lock().unwrap(),
        "割り込みの有無で、押したフレームの画素が 1 ビットも変わらない"
    );
}

#[test]
fn 符号化が遅いと描画の段はキューの長さまでしか先に進まない() {
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
    .unwrap();
    assert_eq!(pushed.load(Ordering::SeqCst), n, "全部のフレームを押す");
    let max = ahead.load(Ordering::SeqCst);
    assert!(
        (2..=MAX_AHEAD).contains(&max),
        "描画の段は符号化の段より先に進むが、{MAX_AHEAD} フレームを超えない: {max}"
    );
}

#[test]
fn キューが満ちたまま符号化が失敗しても描画の段は止まってそのエラーを返す() {
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
    });
    let err = result.expect_err("失敗する");
    assert!(
        matches!(&err, MmError::Corrupt(m) if m == "push 3"),
        "符号化の段のエラー: {err}"
    );
    assert_eq!(pushed, 2);
    assert!(
        drawn.load(Ordering::SeqCst) <= 2 + MAX_AHEAD,
        "失敗の後は描き続けない"
    );
}

#[test]
fn 描画の段が失敗したら符号化の段は何も押さずに止まる() {
    let (result, pushed) = within_limit(|| {
        let mut probe = Probe::default();
        let missing = Some(Id::new("無いコンポジション"));
        let result = run(&mut probe, &ExportControl::default(), missing, &mut |_| {});
        (result, probe.pushed.load(Ordering::SeqCst))
    });
    let err = result.expect_err("描けない");
    assert!(
        !err.to_string().contains("止まった"),
        "相手の段が止まった旨ではなく、描画の段のエラーを返す: {err}"
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
    });
    let err = result.expect_err("取り消す");
    assert!(is_cancel_error(&err), "取り消しの印: {err}");
    assert!(flag.load(Ordering::Relaxed));
}
