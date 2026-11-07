//! ep02（DTM_Vlog の作曲Vlog #2。公開済み）を `mm geneva` で取り込み、`mm render` で書き出して前の書き出しと比べる
//! （`docs/design/11-testing.md` の「実データ」と「ep02 の再現の結果」）。
//!
//! 前の書き出しは最終の 1080p（`render/ep02_final.mp4`）で、取り込んだ本編のレンダーターゲットで書き出す。
//! 素材はリポジトリに入れず、素材のフォルダーは読むだけにする。`MM_EP02_DIR` に回のフォルダーを渡したときだけ実行し、
//! 無ければ何もせず通る。既定は ep02 の特徴の出る区間だけを書き出し、`MM_EP02_FULL=1` で全体を書き出す。
//! 巻き戻しのトランジション（geneva のマークアップ）は `web.cef` で描くので、CEF の無い環境（`CEF_PATH` が無い）では
//! その区間をスキップする。

use std::path::PathBuf;

use crate::replay::{self, Limits};

/// 回のフォルダーを渡す環境変数。
const ENV_DIR: &str = "MM_EP02_DIR";
/// 全体を書き出す環境変数。
const ENV_FULL: &str = "MM_EP02_FULL";
/// CEF の配布物の場所。無ければ `web.cef` は色の矩形で代わりに描く。
const ENV_CEF: &str = "CEF_PATH";
/// 1 つの区間の秒数。
const WINDOW_SECONDS: f64 = 4.0;

/// 区間で何を描くか。閾値の選び分けに使う。
#[derive(Clone, Copy, PartialEq)]
enum Look {
    /// 映像と PNG と単色だけ。
    Plain,
    /// `web.cef` で描くマークアップを含む。
    Web,
    /// 画像のマスク（輝度で切り抜くトランジション）を含む。
    ImageMask,
}

/// 比べる区間の始まり（秒）と、そこに出る ep02 の特徴。
const WINDOWS: [(f64, &str, Look); 10] = [
    (
        3.0,
        "フック: VR の顔と字幕、喋りの下で下げた曲",
        Look::Plain,
    ),
    (
        11.0,
        "ぼかした背景（cover と blur）と、暗い単色の半透明の重ね",
        Look::Plain,
    ),
    (15.0, "巻き戻しのトランジション（マークアップ）", Look::Web),
    (
        24.5,
        "ワイプの入れ子のコンポジション（角丸のマスク）",
        Look::Plain,
    ),
    (
        341.5,
        "ワイプの入れ子が続く所と、60 倍速の早回しの入り",
        Look::Plain,
    ),
    (388.9, "ぼかした背景と、乗算の単色の色かぶり", Look::Plain),
    (467.1, "加算で重ねた音符の光", Look::Plain),
    (
        499.6,
        "レコードのトランジション（画像のマスク）とワイプの入れ子",
        Look::ImageMask,
    ),
    (
        539.0,
        "光漏れのトランジション（画像のマスク）",
        Look::ImageMask,
    ),
    (852.7, "締めのワイプの入れ子と加算の光", Look::Plain),
];

/// 区間ごとの平均の SSIM の下限。考え方は ep03 と同じ（前の書き出しも movie-maker の書き出しも H.264 を通るので一致しない）。
const SSIM_MEAN_MIN: f64 = 0.97;
/// 1 フレームの SSIM の下限。ep03 と同じく、60 fps の収録で 1/60 秒違うフレームを選ぶ所を見込む。
const SSIM_FRAME_MIN: f64 = 0.85;
/// 区間ごとの平均の PSNR の下限（dB）。
const PSNR_MEAN_MIN: f64 = 33.0;
/// マークアップの区間の平均の SSIM と PSNR の下限。ep04 と同じ（CSS の色は PNG と同じ符号化値に戻るが、際にある）。
const SSIM_MEAN_MIN_WEB: f64 = 0.96;
const PSNR_MEAN_MIN_WEB: f64 = 32.0;
/// 画像のマスクの区間の 1 フレームの SSIM の下限。movie-maker は画像の輝度のマスク（トラックマット）をまだ描かないので、
/// トランジションの 0.7 秒は次の場面がマスク無しで出る。平均の下限は上と同じに保ち、1 フレームの下限だけゆるめる。
const SSIM_FRAME_MIN_IMAGE_MASK: f64 = 0.6;
/// 音の差の下限（dB）。考え方は ep03 と同じ。
const SNR_MIN_DB: f64 = 15.0;
/// 50 ms ごとの大きさの差の平均の上限（dB）。ep03、ep04 と同じ。
const ENVELOPE_MAX_DB: f64 = 0.5;
/// 音の時刻のずれの上限（ミリ秒）。ep03 と同じ（AAC の前置きの扱いを誤ると約 21 ms）。
const LAG_MAX_MS: f64 = 2.0;
/// 全体の書き出しのラウドネスの差の上限（dB）。区間の書き出しはラウドネスを区間で合わせるので見ない。
const GAIN_MAX_DB: f64 = 1.0;
/// 書き出しの音のサンプルのピークの上限（dBFS）。ep03 と同じ（0 dBFS を超えるのはエンコーダーが作ったクリック）。
const PEAK_MAX_DBFS: f64 = 0.0;

/// 変換できない要素として認める一覧（種類、数）。増えたら変換か movie-maker の機能を見直す。
/// 画像のマスクはトランジションの 85 クリップ（トラックマットの描画が無い）。
const KNOWN_GAPS: [(&str, u64); 1] = [("mask", 85)];

/// 画像のマスクを変換できない要素として認めているか。`check_gaps` が一覧の一致を確かめるので、マスクを描けるように
/// なって `KNOWN_GAPS` から外せば、画像のマスクの区間も 1 フレームの下限が元に戻る。
fn image_mask_undrawn() -> bool {
    KNOWN_GAPS.iter().any(|(what, _)| *what == "mask")
}

/// 閾値。全体の書き出しは全部を含むので、いちばんゆるい組を使う。
fn limits(look: Look) -> Limits {
    let web = look == Look::Web;
    Limits {
        ssim_mean_min: if web {
            SSIM_MEAN_MIN_WEB
        } else {
            SSIM_MEAN_MIN
        },
        ssim_frame_min: if look == Look::ImageMask && image_mask_undrawn() {
            SSIM_FRAME_MIN_IMAGE_MASK
        } else {
            SSIM_FRAME_MIN
        },
        psnr_mean_min: if web {
            PSNR_MEAN_MIN_WEB
        } else {
            PSNR_MEAN_MIN
        },
        snr_min_db: SNR_MIN_DB,
        envelope_max_db: ENVELOPE_MAX_DB,
        lag_max_ms: LAG_MAX_MS,
        gain_max_db: GAIN_MAX_DB,
        peak_max_dbfs: PEAK_MAX_DBFS,
    }
}

/// 区間（秒）と、その区間の描くもの。`full` なら全体 1 つ。CEF が無ければ CEF の要る区間をスキップする。
fn windows(duration: f64, full: bool, cef: bool) -> Vec<((f64, f64), Look)> {
    if full {
        return vec![((0.0, duration), Look::ImageMask)];
    }
    WINDOWS
        .iter()
        .filter(|(start, what, look)| {
            let skip = *look == Look::Web && !cef;
            if skip {
                eprintln!("{ENV_CEF} が無いので {start} 秒の区間（{what}）をスキップする");
            }
            !skip
        })
        .map(|(start, _, look)| (replay::window(*start, WINDOW_SECONDS), *look))
        .collect()
}

#[test]
fn ep02_を再現すると前の書き出しと一致する() {
    let Some(dir) = std::env::var_os(ENV_DIR).map(PathBuf::from) else {
        eprintln!("{ENV_DIR} が無いのでスキップする");
        return;
    };
    let expected = replay::fixture("ep02");
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
    let mut failed = Vec::new();
    for (i, (range, look)) in windows(duration, full, cef).into_iter().enumerate() {
        let out = work.path().join(format!("w{i}.mp4"));
        replay::render(&project, &out, (!full).then_some(range), None);
        let (p, s) = replay::compare(&out, &reference, range, work.path());
        assert_eq!(
            p.frames as f64,
            ((range.1 - range.0) * fps).round(),
            "区間のフレーム数"
        );
        replay::report(range, &p, &s);
        failed.extend(replay::misses(range, &p, &s, &limits(look), full));
        // 区間の書き出しは比べ終えたら消し、作業のフォルダーを小さく保つ。
        std::fs::remove_file(&out).ok();
    }
    assert!(failed.is_empty(), "閾値を割った: {failed:#?}");
}
