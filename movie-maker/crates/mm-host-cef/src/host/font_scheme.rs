//! `FONT_ORIGIN` の URL に、`FontTable` のファイルで答えるスキームハンドラー。
//! ページは `file://` から読むので書体の取得はオリジンをまたぐ。CORS の許可の見出しを付ける。

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use cef::*;
use mm_host_api::FONT_ORIGIN;
use mm_host_cef::fonts::FontTable;

/// 見つからないときの状態の番号。
const NOT_FOUND: i32 = 404;
const OK: i32 = 200;

wrap_resource_handler! {
    struct FontResource {
        data: Arc<Vec<u8>>,
        found: bool,
        sent: Arc<AtomicUsize>,
    }

    impl ResourceHandler {
        fn open(
            &self,
            _request: Option<&mut Request>,
            handle_request: Option<&mut ::std::os::raw::c_int>,
            _callback: Option<&mut Callback>,
        ) -> ::std::os::raw::c_int {
            if let Some(handle) = handle_request {
                *handle = 1;
            }
            1
        }

        fn response_headers(
            &self,
            response: Option<&mut Response>,
            response_length: Option<&mut i64>,
            _redirect_url: Option<&mut CefString>,
        ) {
            if let Some(response) = response {
                response.set_status(if self.found { OK } else { NOT_FOUND });
                response.set_mime_type(Some(&CefString::from("font/ttf")));
                response.set_header_by_name(
                    Some(&CefString::from("Access-Control-Allow-Origin")),
                    Some(&CefString::from("*")),
                    1,
                );
            }
            if let Some(length) = response_length {
                *length = self.data.len() as i64;
            }
        }

        fn read(
            &self,
            data_out: *mut u8,
            bytes_to_read: ::std::os::raw::c_int,
            bytes_read: Option<&mut ::std::os::raw::c_int>,
            _callback: Option<&mut ResourceReadCallback>,
        ) -> ::std::os::raw::c_int {
            let start = self.sent.load(Ordering::Relaxed);
            let n = self.data.len().saturating_sub(start).min(bytes_to_read.max(0) as usize);
            if n > 0 && !data_out.is_null() {
                // SAFETY: CEF が `bytes_to_read` バイトの書き込み先を渡し、`n` はそれ以下。
                unsafe { std::ptr::copy_nonoverlapping(self.data[start..].as_ptr(), data_out, n) };
            }
            self.sent.store(start + n, Ordering::Relaxed);
            if let Some(read) = bytes_read {
                *read = n as i32;
            }
            i32::from(n > 0)
        }
    }
}

wrap_scheme_handler_factory! {
    struct FontSchemeFactory {
        table: FontTable,
    }

    impl SchemeHandlerFactory {
        fn create(
            &self,
            _browser: Option<&mut Browser>,
            _frame: Option<&mut Frame>,
            _scheme_name: Option<&CefString>,
            request: Option<&mut Request>,
        ) -> Option<ResourceHandler> {
            let url = request.map(|r| CefString::from(&r.url()).to_string())?;
            let data = self.table.read_url(&url);
            let found = data.is_some();
            Some(FontResource::new(
                Arc::new(data.unwrap_or_default()),
                found,
                Arc::new(AtomicUsize::new(0)),
            ))
        }
    }
}

/// `FONT_ORIGIN` のスキームと名前にハンドラーを登録する。CEF の初期化の後に 1 回呼ぶ。
pub fn register(table: &FontTable) -> Result<(), String> {
    let (scheme, domain) = FONT_ORIGIN
        .split_once("://")
        .ok_or("FONT_ORIGIN にスキームが無い")?;
    let mut factory = FontSchemeFactory::new(table.clone());
    let ok = register_scheme_handler_factory(
        Some(&CefString::from(scheme)),
        Some(&CefString::from(domain)),
        Some(&mut factory),
    );
    if ok == 1 {
        Ok(())
    } else {
        Err("書体のスキームハンドラーを登録できない".into())
    }
}
