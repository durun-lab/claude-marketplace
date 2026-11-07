//! ep04（DTM_Vlog の作曲Vlog #4。制作中）を `mm geneva` で取り込み、`mm render` で書き出して前の書き出しと比べる
//! （`docs/design/11-testing.md` の「実データ」と「ep04 の再現の結果」）。
//!
//! 前の書き出しは確認用の 720p（geneva の `outputs.draft`）なので、取り込んだ `draft` のレンダーターゲットで書き出す。
//! 素材はリポジトリに入れない。`MM_EP04_DIR` にエピソードのフォルダーを渡したときだけ実行し、無ければ何もせず通る。
//! 既定は ep04 の特徴の出る区間だけを書き出し、`MM_EP04_FULL=1` で全体を書き出す。
//! 巻き戻しのトランジション（geneva のマークアップ）は `web.cef` で描くので、CEF の無い環境（`CEF_PATH` が無い）では
//! その区間を飛ばす。

use std::path::PathBuf;

use crate::replay::{self, Limits};

/// エピソードのフォルダーを渡す環境変数。
const ENV_DIR: &str = "MM_EP04_DIR";
/// 全体を書き出す環境変数。
const ENV_FULL: &str = "MM_EP04_FULL";
/// CEF の配布物の場所。無ければ `web.cef` は色の矩形で代わりに描く。
const ENV_CEF: &str = "CEF_PATH";
/// 書き出すレンダーターゲット（geneva の確認用の出力）。
const TARGET: &str = "draft";
/// 1 つの区間の秒数。
const WINDOW_SECONDS: f64 = 4.0;

/// 比べる区間の始まり（秒）と、そこに出る ep04 の特徴。CEF が要る区間は 3 つ目が真。
const WINDOWS: [(f64, &str, bool); 10] = [
    (3.0, "フック: VR の顔と字幕、喋りの下で下げた曲", false),
    (
        25.0,
        "巻き戻しのトランジション（マークアップ）とワイプ",
        true,
    ),
    (158.0, "見出しの ToDo の札と節の切り替え", false),
    (182.0, "作業の画面の枠と札、ワイプ、DAW の音の gain", false),
    (
        634.0,
        "Claude の吹き出しと、喋りの下で下げた DAW の書き出し",
        false,
    ),
    (
        1085.0,
        "ボーカルの聴き比べ（＋1 して DAW で −1）の札",
        false,
    ),
    (1360.0, "ミックスの画面の札と吹き出しの指し示し", false),
    (
        1458.5,
        "ハモリの出し入れ（音量の線を描いた後の再生）",
        false,
    ),
    (
        1518.0,
        "到達点: 調声とミックスの後のサビ（曲の書き出し）",
        false,
    ),
    (1564.0, "締め: 登録と高評価の札", false),
];

/// 区間ごとの平均の SSIM の下限。考え方は ep03 と同じ（前の書き出しも movie-maker の書き出しも H.264 を通るので一致しない）。
/// 再現の区間は 0.974〜0.989 だった（最も低いのは `web.cef` で描く巻き戻しの区間）。
const SSIM_MEAN_MIN: f64 = 0.97;
/// 1 フレームの SSIM の下限。ep03 と同じく、60 fps の収録で 1/60 秒違うフレームを選ぶ所を見込む。
/// 再現の区間の最小は 0.931（巻き戻しの区間のマークアップの出入り）。
const SSIM_FRAME_MIN: f64 = 0.85;
/// 全体の書き出しの 1 フレームの SSIM の下限。ep03 と同じく、区間に入らない収録のスクロールを見込む。
const SSIM_FRAME_MIN_FULL: f64 = 0.80;
/// 区間ごとの平均の PSNR の下限（dB）。再現の区間は 33.1〜41.7 dB だった（最も低いのは巻き戻しの区間）。
const PSNR_MEAN_MIN: f64 = 33.0;
/// 音の差の下限（dB）。考え方は ep03 と同じ。再現の区間は 19.6〜29.8 dB だった。
const SNR_MIN_DB: f64 = 15.0;
/// 50 ms ごとの大きさの差の平均の上限（dB）。再現の区間は 0.26 dB 以下。
/// DAW の音の gain や曲のダッキングのキーフレームが抜けると数 dB になる。
const ENVELOPE_MAX_DB: f64 = 0.5;
/// 音の時刻のずれの上限（ミリ秒）。ep03 と同じ（AAC の前置きの扱いを誤ると約 21 ms）。再現は全部の区間で 0 ms。
const LAG_MAX_MS: f64 = 2.0;
/// 全体の書き出しのラウドネスの差の上限（dB）。区間の書き出しはラウドネスを区間で合わせるので見ない。
const GAIN_MAX_DB: f64 = 1.0;
/// 書き出しの音のサンプルのピークの上限（dBFS）。ep03 と同じ（0 dBFS を超えるのはエンコーダーが作ったクリック）。
/// 再現の区間は -1.03 dBFS 以下。
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

/// 区間（秒）。`full` なら全体 1 つ。CEF が無ければ CEF の要る区間を飛ばす。
fn windows(duration: f64, full: bool, cef: bool) -> Vec<(f64, f64)> {
    if full {
        return vec![(0.0, duration)];
    }
    WINDOWS
        .iter()
        .filter(|(start, what, needs_cef)| {
            let skip = *needs_cef && !cef;
            if skip {
                eprintln!("{ENV_CEF} が無いので {start} 秒の区間（{what}）を飛ばす");
            }
            !skip
        })
        .map(|(start, _, _)| replay::window(*start, WINDOW_SECONDS))
        .collect()
}

#[test]
fn ep04_を再現すると前の書き出しと一致する() {
    let Some(dir) = std::env::var_os(ENV_DIR).map(PathBuf::from) else {
        eprintln!("{ENV_DIR} が無いのでスキップする");
        return;
    };
    let expected = replay::fixture("ep04");
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
    let cef = std::env::var_os(ENV_CEF).is_some();
    let limits = limits(full);
    let mut failed = Vec::new();
    for (i, range) in windows(duration, full, cef).into_iter().enumerate() {
        let out = work.path().join(format!("w{i}.mp4"));
        replay::render(&project, &out, (!full).then_some(range), Some(TARGET));
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
