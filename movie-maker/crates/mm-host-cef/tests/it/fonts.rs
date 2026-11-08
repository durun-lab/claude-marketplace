//! `Prepare` の `fonts` の書体で描く（実機の CEF）。テスト用の書体（fixtures/fonts）で描いた画素が、書体が無いときの置き換えと違い、
//! 書体のファイルが無くても失敗せずに置き換えで描くことを確かめる。
#![cfg(all(feature = "cef", windows))]

use std::path::Path;

use mm_host_api::{HostProcess, HostRequest, HostResponse, SharedFrame, FONT_ORIGIN};
use mm_host_cef::CefLauncher;
use mm_types::{RationalTime, Size};

use crate::serve::{host_exe, render, spec, REQUEST_LIMIT};

const SIZE: Size = Size::new(320, 160);

fn page() -> String {
    format!(
        "<!doctype html><html><head><meta http-equiv=\"Content-Security-Policy\" content=\"default-src 'none'; \
         script-src 'unsafe-inline'; style-src 'unsafe-inline'; font-src {FONT_ORIGIN}\"></head>\
         <body style=\"margin:0;background:#fff\"><div id=\"t\" style=\"font:96px 'Mochiy Pop One';color:#000\"></div>\
         <script>window.mmSetTime = () => {{ document.getElementById('t').textContent = 'おと'; }};</script>\
         </body></html>"
    )
}

fn font(rel: &str) -> serde_json::Value {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/fonts")
        .join(rel);
    serde_json::json!({ "path": path, "content_hash": "", "family": "Mochiy Pop One" })
}

/// `fonts` を渡してページを読み、1 枚を CPU の画像で描く。
fn draw(fonts: serde_json::Value) -> Vec<f32> {
    let exe = host_exe().expect("CEF");
    let mut host = CefLauncher::start(spec(&exe, None)).expect("ホストを起こせる");
    let req = HostRequest::Prepare {
        params: serde_json::json!({ "html": page(), "fonts": fonts }),
    };
    assert_eq!(
        host.request(&req, REQUEST_LIMIT).expect("Prepare"),
        HostResponse::Prepared
    );
    match render(&mut host, RationalTime::ZERO, SIZE, serde_json::json!({})) {
        SharedFrame::Cpu { data, .. } => data,
        other => panic!("CPU の画像のはず: {other:?}"),
    }
}

#[test]
fn 書体で描き_無いファイルは置き換えで描く() {
    if host_exe().is_none() {
        return;
    }
    let fallback = draw(serde_json::json!([]));
    let with = draw(serde_json::json!([font(
        "Mochiy Pop One/MochiyPopOne-Regular.ttf"
    )]));
    let missing = draw(serde_json::json!([font("無い/None.ttf")]));
    assert_ne!(with, fallback, "書体が効いていない");
    assert_eq!(missing, fallback, "無いファイルは置き換えの書体で描く");
    let again = draw(serde_json::json!([font(
        "Mochiy Pop One/MochiyPopOne-Regular.ttf"
    )]));
    assert_eq!(with, again, "同じ入力なら同じ画素");
}
