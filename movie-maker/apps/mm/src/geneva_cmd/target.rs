//! 出力（`output` と `outputs`）をレンダーターゲットにする。
//!
//! `output` は本編の 1 つ目のターゲット、`outputs` の映像の出力（確認用の 720p など）は 2 つ目以降にする。
//! geneva の `outputs` の映像は本編を縮めて書くので、同じコンポジションを高さだけ変えて書き出すターゲットで表す。

use std::path::Path;

use mm_doc::RenderTarget;
use mm_types::{Id, Rational};
use serde_json::Value;

use super::report::{Gap, Report};

/// 本編のコンポジションとレンダーターゲットの ID。
pub const MAIN_ID: &str = "main";
/// レンダーターゲットのコーデック。
const CODEC: &str = "h264";
/// `outputs` の映像の出力の種類。
const VIDEO_KIND: &str = "video";

/// 本編のレンダーターゲット。音の大きさの目標とトゥルーピークの上限は `output.audio.loudness` から。
pub fn main_target(out: &Value, size: [u32; 2], fps: Rational, path: &Path) -> RenderTarget {
    let loudness = out.pointer("/audio/loudness");
    let mut target = target(MAIN_ID, size, fps, path);
    target.loudness_lufs = loudness
        .and_then(|l| l.get("target_lufs"))
        .and_then(Value::as_f64)
        .map(|v| v as f32);
    target.delivery.true_peak_dbtp = loudness
        .and_then(|l| l.get("true_peak_dbtp"))
        .and_then(Value::as_f64)
        .map(|v| v as f32);
    target
}

/// `outputs` の映像の出力を、本編のターゲットの写し（大きさと出力先だけ違う）にする。
/// 出力先は本編の出力先と同じフォルダーの `path`。映像でない出力（音だけ、静止画）は数えて除く。
pub fn extra_targets(
    outputs: Option<&Value>,
    main: &RenderTarget,
    report: &mut Report,
) -> Vec<RenderTarget> {
    let Some(entries) = outputs.and_then(Value::as_object) else {
        return Vec::new();
    };
    let main_path = Path::new(&main.output_path);
    let mut out = Vec::with_capacity(entries.len());
    for (name, entry) in entries {
        let is_video = entry.get("kind").and_then(Value::as_str) == Some(VIDEO_KIND);
        let path = entry.get("path").and_then(Value::as_str);
        let (Some(path), true) = (path, is_video && name != MAIN_ID) else {
            report.add(Gap::TopLevel);
            continue;
        };
        let size = scaled_size([main.width, main.height], entry);
        let mut t = main.clone();
        t.id = Id::new(name);
        t.width = size[0];
        t.height = size[1];
        t.output_path = main_path
            .with_file_name(path)
            .to_string_lossy()
            .into_owned();
        out.push(t);
    }
    out
}

/// 縦横比を保って `width` か `height` に合わせた大きさ。もう一方は偶数に丸める（H.264 の 4:2:0 は偶数が要る）。
fn scaled_size([w, h]: [u32; 2], entry: &Value) -> [u32; 2] {
    let get = |k: &str| entry.get(k).and_then(Value::as_u64).map(|n| n as u32);
    let even = |x: f64| ((x / 2.0).round() as u32) * 2;
    match (get("width"), get("height")) {
        (Some(ew), Some(eh)) => [ew, eh],
        (None, Some(eh)) => [even(f64::from(w) * f64::from(eh) / f64::from(h)), eh],
        (Some(ew), None) => [ew, even(f64::from(h) * f64::from(ew) / f64::from(w))],
        (None, None) => [w, h],
    }
}

fn target(id: &str, [width, height]: [u32; 2], fps: Rational, path: &Path) -> RenderTarget {
    RenderTarget {
        id: Id::new(id),
        composition: Id::new(MAIN_ID),
        width,
        height,
        fps,
        codec: CODEC.into(),
        output_path: path.to_string_lossy().into_owned(),
        loudness_lufs: None,
        delivery: Default::default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn main() -> RenderTarget {
        main_target(
            &json!({"audio": {"loudness": {"target_lufs": -14, "true_peak_dbtp": -1}}}),
            [1920, 1080],
            Rational::integer(30),
            Path::new("E:/work/render.mp4"),
        )
    }

    #[test]
    fn 高さだけの映像の出力は縦横比を保った写しのターゲットになる() {
        let mut report = Report::default();
        let outputs = json!({"draft": {"kind": "video", "height": 720, "path": "ep04_draft.mp4"}});
        let got = extra_targets(Some(&outputs), &main(), &mut report);
        assert_eq!(got.len(), 1);
        let t = &got[0];
        assert_eq!((t.id.as_str(), t.width, t.height), ("draft", 1280, 720));
        assert_eq!(t.composition.as_str(), MAIN_ID);
        assert_eq!(t.loudness_lufs, Some(-14.0));
        assert_eq!(
            Path::new(&t.output_path),
            Path::new("E:/work/ep04_draft.mp4")
        );
        assert!(report.to_json().as_array().unwrap().is_empty());
    }

    #[test]
    fn 映像でない出力は数えて除く() {
        let mut report = Report::default();
        let outputs = json!({"mix": {"kind": "audio", "path": "audio.wav"}});
        assert!(extra_targets(Some(&outputs), &main(), &mut report).is_empty());
        assert_eq!(report.to_json()[0]["what"], "top-level");
    }
}
