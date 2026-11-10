//! 納品物（ラウドネスの正規化、字幕、チャプター）の、エンジンを通した振る舞い。中身の確かめは `mm` の通しのテスト。

use super::*;
use mm_doc::{Delivery, SubtitleOutput};
use mm_runtime::{assemble, RuntimeConfig};
use mm_testkit::decode::SampleMedia;
use mm_testkit::fixtures::vertical_project;

fn temp(name: &str) -> tempfile::TempDir {
    tempfile::Builder::new()
        .prefix(&format!("mm_export_deliver_{name}_"))
        .tempdir()
        .unwrap()
}

fn project_with(out: &Path, edit: impl FnOnce(&mut mm_doc::RenderTarget)) -> Project {
    let (mut project, _, _) = vertical_project(
        &SampleMedia::default(),
        &temp("unused").path().join("unused.json"),
    );
    project.render_targets[0].output_path = out.to_string_lossy().into_owned();
    edit(&mut project.render_targets[0]);
    project
}

/// 作業の明るさが sdr のまま HDR の素材を HDR の信号で書き出すときだけ、切り替え方を警告に出す。
#[test]
fn sdr_の作業の明るさで_hdr_の素材を_hdr_の信号で書き出すと警告する() {
    let tmp = temp("hdrwarn");
    let out = tmp.path().join("out.mp4");
    let mut project = project_with(&out, |t| t.codec = "hevc_pq".into());
    let warned = |p: &Project| deliver::hdr_signal_warning(p, &p.render_targets[0]);
    assert_eq!(warned(&project), None, "HDR の素材が無ければ出さない");
    project.assets[0].color_space = mm_types::ColorSpace::bt2100_pq();
    assert!(warned(&project).is_some_and(|w| w.contains("setColorConfig")));
    project.color_config.working_range = mm_doc::WorkingRange::Hdr;
    assert_eq!(warned(&project), None, "hdr なら出さない");
}

#[test]
fn 正規化を持たない音の実装でラウドネスの目標があると拒否する() {
    let tmp = temp("loud");
    let out = tmp.path().join("out.json");
    let project = project_with(&out, |t| t.loudness_lufs = Some(-14.0));
    let mut engine = assemble(&RuntimeConfig::sample()).unwrap();
    let err = export_project(&mut engine, &project, &ExportOptions::default()).unwrap_err();
    assert!(matches!(err, MmError::Unsupported(_)), "{err}");
    assert!(!out.exists(), "失敗したら出力を残さない");
}

#[test]
fn 字幕を出す設定で文字起こしが無いと拒否する() {
    let tmp = temp("nosub");
    let out = tmp.path().join("out.json");
    let project = project_with(&out, |t| {
        t.delivery = Delivery {
            subtitles: SubtitleOutput::Sidecar,
            ..Delivery::default()
        }
    });
    let mut engine = assemble(&RuntimeConfig::sample()).unwrap();
    let err = export_project(&mut engine, &project, &ExportOptions::default()).unwrap_err();
    assert!(matches!(err, MmError::InvalidArgument(_)), "{err}");
}

#[test]
fn 出さなくなった字幕とチャプターの前回のファイルを消し_警告に出す() {
    let tmp = temp("stale");
    let dir = tmp.path().join("out");
    let out = dir.join("out.json");
    std::fs::create_dir_all(&dir).unwrap();
    let stale = ["out.srt", "out.vtt", "out.chapters.txt"].map(|n| dir.join(n));
    let other = dir.join("out.ja.srt");
    for p in stale.iter().chain([&other]) {
        std::fs::write(p, "old").unwrap();
    }
    let project = project_with(&out, |_| {});
    let mut engine = assemble(&RuntimeConfig::sample()).unwrap();
    let summary = export_project(&mut engine, &project, &ExportOptions::default()).unwrap();
    for p in &stale {
        assert!(!p.exists(), "{} が残っている", p.display());
    }
    assert!(other.exists(), "自分が作る名前でないファイルは消さない");
    assert_eq!(
        summary
            .delivery
            .warnings
            .iter()
            .filter(|w| w.contains("前回"))
            .count(),
        stale.len(),
        "{:?}",
        summary.delivery.warnings
    );
}

#[test]
fn チャプターの決まりを満たさなくても書き出しは成功し_理由を警告に出す() {
    let tmp = temp("chap");
    let out = tmp.path().join("out.json");
    let project = project_with(&out, |t| {
        t.delivery = Delivery {
            chapters: true,
            ..Delivery::default()
        }
    });
    let mut engine = assemble(&RuntimeConfig::sample()).unwrap();
    let summary = export_project(&mut engine, &project, &ExportOptions::default()).unwrap();
    assert!(summary.delivery.chapters_file.is_none());
    assert!(
        summary
            .delivery
            .warnings
            .iter()
            .any(|w| w.contains("3 つ以上")),
        "{:?}",
        summary.delivery.warnings
    );
}
