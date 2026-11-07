//! ページが読み込んでよい URL の決まり（CEF の要らないテスト）。

use std::path::Path;

use mm_host_cef::policy::{allows_navigation, allows_resource, file_url, is_file_url_of};

const PAGE: &str = "file:///C:/Users/a/AppData/Local/Temp/mm-host-cef-1-0/pages/page-0.html";

#[test]
fn ページそのものと埋め込みの資源だけを読み込める() {
    assert!(allows_resource(PAGE, PAGE));
    assert!(allows_resource("data:font/woff2;base64,AAAA", PAGE));
    assert!(allows_resource("DATA:image/png;base64,AAAA", PAGE));
    assert!(allows_resource(
        "blob:file:///6f1c1f2e-0000-4000-8000-000000000000",
        PAGE
    ));
}

#[test]
fn ネットワークとほかのファイルは読み込めない() {
    for url in [
        "http://127.0.0.1:8080/a.png",
        "https://example.com/font.woff2",
        "ws://127.0.0.1/",
        "file:///C:/Windows/win.ini",
        "file:///C:/Users/a/AppData/Local/Temp/mm-host-cef-1-0/pages/page-1.html",
        "chrome://version",
        "",
    ] {
        assert!(!allows_resource(url, PAGE), "{url}");
    }
}

#[test]
fn 遷移はページそのものだけ() {
    assert!(allows_navigation(PAGE, PAGE));
    for url in [
        "http://127.0.0.1/",
        "data:text/html,<p>x</p>",
        "file:///C:/Windows/win.ini",
        "about:blank",
    ] {
        assert!(!allows_navigation(url, PAGE), "{url}");
    }
    assert!(!allows_navigation("", ""), "ページが無ければ何も通さない");
}

#[test]
fn ホストが書いたページのファイルの_url_だけをそのページと見なす() {
    let page = Path::new(r"C:\Users\a b\あ\pages\page-1.html");
    assert_eq!(file_url(page), "file:///C:/Users/a b/あ/pages/page-1.html");
    for url in [
        "file:///C:/Users/a%20b/%E3%81%82/pages/page-1.html",
        "file:///c:/users/A%20B/%e3%81%82/pages/page-1.html",
        file_url(page).as_str(),
    ] {
        assert!(is_file_url_of(url, page), "{url}");
    }
    for url in [
        "file:///C:/Users/a%20b/%E3%81%82/pages/page-2.html",
        "file:///C:/Users/a%20b/%E3%81%82/pages/page-1.html?x",
        "file:///C:/Users/a%2Xb/%E3%81%82/pages/page-1.html",
        "file:///C:/Users/a%+2b/%E3%81%82/pages/page-1.html",
        "file:///C:/Users/a%20b/%E3%81/pages/page-1.html",
        "http://127.0.0.1/C:/Users/a%20b/%E3%81%82/pages/page-1.html",
        "",
    ] {
        assert!(!is_file_url_of(url, page), "{url}");
    }
}
