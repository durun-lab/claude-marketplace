//! ep03（DTM_Vlog の作曲Vlog #3）を `mm geneva` で取り込み、`mm render` で書き出して前の書き出しと比べる
//! （`docs/design/11-testing.md` の「実データ」）。
//!
//! 素材はリポジトリに入れない。`MM_EP03_DIR` にエピソードのフォルダー（`timeline.json` と `render/ep03_final.mp4` がある所）を
//! 渡したときだけ実行し、無ければ何もせず通る。既定は全体に散らした区間だけを書き出し、`MM_EP03_FULL=1` で全体を書き出す。

use std::path::PathBuf;

use crate::replay::{self, Limits};

/// エピソードのフォルダーを渡す環境変数。
const ENV_DIR: &str = "MM_EP03_DIR";
/// 全体を書き出す環境変数。
const ENV_FULL: &str = "MM_EP03_FULL";
/// 区間の書き出しで比べる位置（全体の長さに対する割合）と、1 つの区間の秒数。
const WINDOW_AT: [f64; 6] = [0.02, 0.2, 0.4, 0.6, 0.8, 0.97];
const WINDOW_SECONDS: f64 = 4.0;

/// 区間ごとの平均の SSIM の下限。
/// 前の書き出し（geneva、H.264）と movie-maker の書き出し（H.264）は、どちらも非可逆の符号化を通るので一致しない。
/// 前の書き出しを movie-maker の既定（libx264、CRF 18、medium）で符号化し直しただけで 0.995 になり、
/// 再現の区間は 0.982〜0.993 だった。残りの差は描き方（縮小のフィルター）に充てる。
const SSIM_MEAN_MIN: f64 = 0.97;
/// 1 フレームの SSIM の下限。60 fps の OBS の収録（mkv の ms 単位の丸め）は、movie-maker が CFR の格子で、
/// geneva が PTS でフレームを選ぶので、3 フレームに 1 回 1/60 秒違うフレームになり、動きの大きい所で 0.88 まで下がる。
const SSIM_FRAME_MIN: f64 = 0.85;
/// 全体の書き出しの 1 フレームの SSIM の下限。全体には区間に入らない 60 fps の収録のスクロール（373.3 秒）があり、
/// 同じ理由で 0.818 まで下がる。
const SSIM_FRAME_MIN_FULL: f64 = 0.80;
/// 区間ごとの平均の PSNR の下限（dB）。再現の区間は 36.7〜41.4 dB だった。
const PSNR_MEAN_MIN: f64 = 33.0;
/// 音の差の下限（dB）。AAC の符号化を 2 回とリミッターを通るので一致しない。再現の区間は 18〜28 dB だった。
const SNR_MIN_DB: f64 = 15.0;
/// 50 ms ごとの大きさの差の平均の上限（dB）。再現の区間は 0.16 dB 以下。フェードが抜けると 2 dB を超える。
const ENVELOPE_MAX_DB: f64 = 0.5;
/// 音の時刻のずれの上限（ミリ秒）。AAC の前置きの扱いを誤ると約 21 ms、1 フレームずれると 33 ms になる。
const LAG_MAX_MS: f64 = 2.0;
/// 全体の書き出しのラウドネスの差の上限（dB）。区間の書き出しはラウドネスを区間で合わせるので見ない。
const GAIN_MAX_DB: f64 = 1.0;
/// 書き出しの音のサンプルのピークの上限（dBFS）。正規化は -1 dBTP に収めるので、0 dBFS を超えるのは
/// エンコーダーが作ったクリック（AAC の雑音の置き換えで、片方のチャンネルに数サンプル +4.6 dBFS が出た）。
const PEAK_MAX_DBFS: f64 = 0.0;

/// 変換できない要素として認める一覧（種類、数）。増えたら変換か movie-maker の機能を見直す。
const KNOWN_GAPS: [(&str, u64); 0] = [];

fn limits(full: bool) -> Limits {
    Limits {
        ssim_mean_min: SSIM_MEAN_MIN,
        ssim_frame_min: if full {
            SSIM_FRAME_MIN_FULL
        } else {
            SSIM_FRAME_MIN
        },
        psnr_mean_min: PSNR_MEAN_MIN,
        snr_min_db: SNR_MIN_DB,
        envelope_max_db: ENVELOPE_MAX_DB,
        lag_max_ms: LAG_MAX_MS,
        gain_max_db: GAIN_MAX_DB,
        peak_max_dbfs: PEAK_MAX_DBFS,
    }
}

/// 区間（秒）。`full` なら全体 1 つ、そうでなければ短い区間を散らす。
fn windows(duration: f64, full: bool) -> Vec<(f64, f64)> {
    if full {
        return vec![(0.0, duration)];
    }
    WINDOW_AT
        .iter()
        .map(|at| replay::window((duration - WINDOW_SECONDS) * at, WINDOW_SECONDS))
        .collect()
}

#[test]
fn ep03_を再現すると前の書き出しと一致する() {
    let Some(dir) = std::env::var_os(ENV_DIR).map(PathBuf::from) else {
        eprintln!("{ENV_DIR} が無いのでスキップする");
        return;
    };
    let expected = replay::fixture("ep03");
    let (timeline, reference) = replay::inputs(&dir, &expected);
    let work = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("作業のフォルダー");
    let project = work.path().join("project.json");
    let summary = replay::import(&timeline, &project);
    replay::check_gaps(&summary, &KNOWN_GAPS);
    let fps = expected["reference"]["fps"].as_f64().expect("fps");
    let frames = expected["reference"]["frames"].as_f64().expect("frames");
    let duration = summary["duration"].as_f64().expect("duration");
    assert_eq!(
        (duration * fps).round(),
        frames,
        "長さ（フレーム数）が前の書き出しと同じ"
    );
    let full = std::env::var(ENV_FULL).is_ok_and(|v| v == "1");
    let limits = limits(full);
    let mut failed = Vec::new();
    for (i, range) in windows(duration, full).into_iter().enumerate() {
        let out = work.path().join(format!("w{i}.mp4"));
        replay::render(&project, &out, (!full).then_some(range), None);
        let (p, s) = replay::compare(&out, &reference, range, work.path());
        assert_eq!(
            p.frames as f64,
            ((range.1 - range.0) * fps).round(),
            "区間のフレーム数"
        );
        replay::report(range, &p, &s);
        failed.extend(replay::misses(range, &p, &s, &limits, full));
    }
    assert!(failed.is_empty(), "閾値を割った: {failed:#?}");
}
