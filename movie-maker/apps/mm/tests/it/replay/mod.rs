//! 終わったエピソード（DTM_Vlog の geneva のタイムライン）を `mm geneva` で取り込み、`mm render` で書き出して
//! 前の書き出しと比べる手順（`docs/design/11-testing.md` の「実データ」）。回ごとの区間と閾値は `ep03.rs`、`ep04.rs` が持つ。

pub mod measure;

use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;
use sha2::{Digest, Sha256};

use measure::{Picture, Sound};

/// 区間の始まりをそろえる刻み（1 秒あたりの数）。区間を変えると前に計った数字と比べられないので、0.1 秒の格子に保つ。
const WINDOW_GRID: f64 = 10.0;

/// 区間ごとの閾値。根拠は回ごとのテストの定数のコメント。
pub struct Limits {
    pub ssim_mean_min: f64,
    pub ssim_frame_min: f64,
    pub psnr_mean_min: f64,
    pub snr_min_db: f64,
    pub envelope_max_db: f64,
    pub lag_max_ms: f64,
    /// 全体の書き出しのラウドネスの差の上限。区間の書き出しはラウドネスを区間で合わせるので見ない。
    pub gain_max_db: f64,
    pub peak_max_dbfs: f64,
}

/// `fixtures/<name>/expected.json`（入力と前の書き出しの SHA-256、フレーム数、fps）。
pub fn fixture(name: &str) -> Value {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures")
        .join(name)
        .join("expected.json");
    serde_json::from_str(&std::fs::read_to_string(path).expect("expected.json がある"))
        .expect("JSON")
}

fn sha256(path: &Path) -> String {
    let mut file = std::fs::File::open(path).expect("開ける");
    let mut hasher = Sha256::new();
    std::io::copy(&mut file, &mut hasher).expect("読める");
    format!("{:x}", hasher.finalize())
}

/// 入力と前の書き出しが、フィクスチャに書いたものと同じか確かめてパスを返す。
pub fn inputs(dir: &Path, expected: &Value) -> (PathBuf, PathBuf) {
    let check = |key: &str| {
        let path = dir.join(expected[key]["path"].as_str().expect("path"));
        let want = expected[key]["sha256"].as_str().expect("sha256");
        assert_eq!(sha256(&path), want, "{} が変わった", path.display());
        path
    };
    (check("timeline"), check("reference"))
}

fn mm(args: &[&OsStr]) -> String {
    let out = Command::new(env!("CARGO_BIN_EXE_mm"))
        .args(args)
        .output()
        .expect("mm を実行できる");
    assert!(
        out.status.success(),
        "mm {:?} が成功する: {}",
        args,
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// 取り込んでプロジェクトの JSON を書き、要約を返す。
pub fn import(timeline: &Path, project: &Path) -> Value {
    let summary = mm(&[
        OsStr::new("geneva"),
        timeline.as_os_str(),
        OsStr::new("-o"),
        project.as_os_str(),
    ]);
    serde_json::from_str(summary.trim()).expect("要約の JSON")
}

/// `target`（レンダーターゲットの ID。省くと最初の 1 つ）を、`range` の区間（省くと全体）で書き出す。
pub fn render(project: &Path, out: &Path, range: Option<(f64, f64)>, target: Option<&str>) {
    let range = range.map(|(a, b)| format!("{a}:{b}"));
    let mut args: Vec<&OsStr> = vec![
        OsStr::new("render"),
        project.as_os_str(),
        OsStr::new("-o"),
        out.as_os_str(),
    ];
    if let Some(r) = &range {
        args.push(OsStr::new("--range"));
        args.push(OsStr::new(r));
    }
    if let Some(t) = target {
        args.push(OsStr::new("--target"));
        args.push(OsStr::new(t));
    }
    mm(&args);
}

/// 長さ `len` 秒の区間を、始まりを 0.1 秒の格子にそろえて作る。
pub fn window(start: f64, len: f64) -> (f64, f64) {
    let start = (start * WINDOW_GRID).floor() / WINDOW_GRID;
    (start, start + len)
}

/// 変換できない要素の一覧が `known`（種類、数）と同じか確かめる。
pub fn check_gaps(summary: &Value, known: &[(&str, u64)]) {
    let gaps: Vec<(String, u64)> = summary["gaps"]
        .as_array()
        .expect("gaps")
        .iter()
        .map(|g| {
            (
                g["what"].as_str().unwrap_or("").to_owned(),
                g["count"].as_u64().unwrap_or(0),
            )
        })
        .collect();
    let known: Vec<(String, u64)> = known.iter().map(|(w, n)| ((*w).to_owned(), *n)).collect();
    assert_eq!(gaps, known, "変換できない要素の一覧が変わった");
}

/// 書き出し `out`（区間の書き出しなら区間だけ、全体なら全体）を、参照の同じ区間と比べる。
pub fn compare(out: &Path, reference: &Path, range: (f64, f64), work: &Path) -> (Picture, Sound) {
    let len = range.1 - range.0;
    let p = measure::picture(out, reference, range.0, len, work);
    let s = measure::sound(
        &measure::pcm(out, 0.0, len),
        &measure::pcm(reference, range.0, len),
        measure::peak_dbfs(out),
    );
    (p, s)
}

/// 区間の数字を 1 行で出す。
pub fn report(range: (f64, f64), p: &Picture, s: &Sound) {
    eprintln!(
        "[{:7.2}, {:7.2}) 映像 {} フレーム SSIM 平均 {:.4} 最小 {:.4} PSNR {:.2} dB / 音 倍率 {:+.2} dB ずれ {:+.2} ms SNR {:.1} dB 包絡 {:.2} dB ピーク {:+.2} dBFS",
        range.0, range.1, p.frames, p.ssim_mean, p.ssim_min, p.psnr_mean, s.gain_db, s.lag_ms, s.snr_db, s.envelope_db, s.peak_dbfs
    );
}

/// 閾値を割った項目の一覧。全部の区間を見てから失敗させるため、ここでは集めるだけ。
pub fn misses(
    range: (f64, f64),
    p: &Picture,
    s: &Sound,
    limits: &Limits,
    full: bool,
) -> Vec<String> {
    let checks = [
        (
            p.ssim_mean >= limits.ssim_mean_min,
            format!("SSIM の平均 {:.4}", p.ssim_mean),
        ),
        (
            p.ssim_min >= limits.ssim_frame_min,
            format!("SSIM の最小 {:.4}", p.ssim_min),
        ),
        (
            p.psnr_mean >= limits.psnr_mean_min,
            format!("PSNR {:.2}", p.psnr_mean),
        ),
        (
            s.snr_db >= limits.snr_min_db,
            format!("音の SNR {:.1}", s.snr_db),
        ),
        (
            s.envelope_db <= limits.envelope_max_db,
            format!("音の包絡の差 {:.2}", s.envelope_db),
        ),
        (
            s.lag_ms.abs() <= limits.lag_max_ms,
            format!("音のずれ {:.2} ms", s.lag_ms),
        ),
        (
            !full || s.gain_db.abs() <= limits.gain_max_db,
            format!("ラウドネスの差 {:.2}", s.gain_db),
        ),
        (
            s.peak_dbfs <= limits.peak_max_dbfs,
            format!("音のピーク {:+.2} dBFS", s.peak_dbfs),
        ),
    ];
    checks
        .into_iter()
        .filter(|(ok, _)| !ok)
        .map(|(_, what)| format!("[{:.2}, {:.2}) {what}", range.0, range.1))
        .collect()
}
