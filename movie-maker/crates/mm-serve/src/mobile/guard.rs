//! 人の待ち受けの守り。`docs/design/12-mobile.md` の「つながり方」の順に確かめ、落ちた理由をログに残す。
//!
//! 1. `Host` が tailnet での名前で、`Origin` があれば同じオリジン（403）。
//! 2. `tailscale serve` が付ける `Tailscale-User-Login` がある（401）。
//! 3. 端末の資格のクッキーが登録した端末のもので、登録した時と同じ人（401）。
//! 4. 書き込みの要求は `X-MM-Mobile: 1` を持つ（403）。

use std::sync::Arc;

use axum::extract::{Request, State};
use axum::http::{header, HeaderMap, Method, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

use super::devices::DeviceInfo;
use super::{MobileError, MobileState};

/// `tailscale serve` が tailnet の要求に付ける本人の見出し。外から来た同じ見出しは `tailscale serve` が消す。
pub const LOGIN_HEADER: &str = "tailscale-user-login";
/// 書き込みの要求に要る見出し。別のオリジンのフォームは付けられない。
pub const MOBILE_HEADER: &str = "x-mm-mobile";
/// 端末の資格を入れるクッキーの名前。
pub const COOKIE_NAME: &str = "mm_device";
/// 資格を確かめる前に受ける唯一の API。
pub const PAIR_PATH: &str = "/m/pair";
/// 資格の要る API の前置き。これの外の読み出し（PWA の画面）は資格の無い端末にも配り、登録の画面を出す。
const API_PREFIX: &str = "/m/api/";

/// 守りを通った要求に、登録した端末（[`DeviceInfo`]）と、その端末の発行元（`Caller`、`Entry::Mobile`）を
/// `Extension` で付ける。Claude の待ち受けと共有するハンドラーは `Extension<Caller>` があれば本体の名乗りを受けない。
pub(crate) async fn guard(
    State(st): State<Arc<MobileState>>,
    mut req: Request,
    next: Next,
) -> Response {
    match check(&st, req.method(), req.uri().path(), req.headers()) {
        Ok(device) => {
            if let Some(d) = device {
                req.extensions_mut().insert(d.caller());
                req.extensions_mut().insert(d);
            }
            next.run(req).await
        }
        Err(e) => {
            if let MobileError::Refuse { code, message, .. } = &e {
                tracing::warn!(
                    code,
                    path = req.uri().path(),
                    "人の待ち受けが断った: {message}"
                );
            }
            e.into_response()
        }
    }
}

fn check(
    st: &MobileState,
    method: &Method,
    path: &str,
    headers: &HeaderMap,
) -> Result<Option<DeviceInfo>, MobileError> {
    check_host(&st.hosts, headers)?;
    let login = header_str(headers, LOGIN_HEADER)
        .filter(|l| !l.is_empty())
        .ok_or_else(|| {
            refuse(
                StatusCode::UNAUTHORIZED,
                "no_tailnet_login",
                "Tailscale-User-Login が無い（tailscale serve を通っていない）",
            )
        })?;
    let read = matches!(*method, Method::GET | Method::HEAD);
    let device = if path == PAIR_PATH || (read && !path.starts_with(API_PREFIX)) {
        None
    } else {
        Some(check_device(st, headers, login)?)
    };
    if !read && header_str(headers, MOBILE_HEADER) != Some("1") {
        return Err(refuse(
            StatusCode::FORBIDDEN,
            "missing_mobile_header",
            "書き込みの要求に X-MM-Mobile: 1 が無い",
        ));
    }
    Ok(device)
}

/// `Host` は tailnet での名前（ポートは問わない）。`Origin` は `https://` と `Host` そのまま。
fn check_host(hosts: &[String], headers: &HeaderMap) -> Result<(), MobileError> {
    let host = header_str(headers, header::HOST.as_str()).unwrap_or_default();
    if !hosts.iter().any(|h| h.eq_ignore_ascii_case(hostname(host))) {
        return Err(refuse(
            StatusCode::FORBIDDEN,
            "forbidden_host",
            &format!("Host {host:?} が tailnet での名前でない"),
        ));
    }
    if let Some(origin) = headers.get(header::ORIGIN) {
        let same = origin
            .to_str()
            .ok()
            .and_then(|o| o.strip_prefix("https://"))
            .is_some_and(|o| o.eq_ignore_ascii_case(host));
        if !same {
            return Err(refuse(
                StatusCode::FORBIDDEN,
                "forbidden_origin",
                "Origin が Host と違う",
            ));
        }
    }
    Ok(())
}

fn check_device(
    st: &MobileState,
    headers: &HeaderMap,
    login: &str,
) -> Result<DeviceInfo, MobileError> {
    // 同じ tailnet の別の端末のページは同じサイトなので、同じ名前のクッキーを足せる。どれか 1 つが合えば通す。
    let credentials: Vec<&str> = cookies(headers, COOKIE_NAME).collect();
    if credentials.is_empty() {
        return Err(refuse(
            StatusCode::UNAUTHORIZED,
            "no_device",
            "端末の資格が無い（登録の符号で登録する）",
        ));
    }
    let mut found = None;
    for credential in credentials {
        found = st.devices.find(credential)?;
        if found.is_some() {
            break;
        }
    }
    let device = found.ok_or_else(|| {
        refuse(
            StatusCode::UNAUTHORIZED,
            "unknown_device",
            "端末の資格が登録に無いか外された",
        )
    })?;
    if device.login != login {
        return Err(refuse(
            StatusCode::UNAUTHORIZED,
            "login_mismatch",
            "登録した時と Tailscale-User-Login が違う",
        ));
    }
    Ok(device)
}

/// `host:port` から名前だけ。tailnet での名前は DNS の名前なので、IPv6 の括弧は考えない。
fn hostname(host: &str) -> &str {
    match host.rsplit_once(':') {
        Some((name, port)) if port.bytes().all(|b| b.is_ascii_digit()) => name,
        _ => host,
    }
}

fn header_str<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    headers.get(name).and_then(|v| v.to_str().ok())
}

/// `Cookie` の見出し（複数に分かれていてもよい）から、名前の合う値を全部引く。
fn cookies<'a>(headers: &'a HeaderMap, name: &'a str) -> impl Iterator<Item = &'a str> {
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(';'))
        .filter_map(|pair| pair.trim().split_once('='))
        .filter(move |(k, _)| *k == name)
        .map(|(_, v)| v)
}

pub(crate) fn refuse(status: StatusCode, code: &'static str, message: &str) -> MobileError {
    MobileError::Refuse {
        status,
        code,
        message: message.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_の名前はポートを除いて比べる() {
        assert_eq!(hostname("desk.tail.ts.net:8443"), "desk.tail.ts.net");
        assert_eq!(hostname("desk.tail.ts.net"), "desk.tail.ts.net");
    }

    #[test]
    fn クッキーは複数の組と見出しから引ける() {
        let mut h = HeaderMap::new();
        h.append(header::COOKIE, "a=1; mm_device=abc".parse().unwrap());
        assert_eq!(cookies(&h, COOKIE_NAME).collect::<Vec<_>>(), ["abc"]);
        let mut split = HeaderMap::new();
        split.append(header::COOKIE, "mm_device=tossed".parse().unwrap());
        split.append(header::COOKIE, "a=1; mm_device=xyz".parse().unwrap());
        assert_eq!(
            cookies(&split, COOKIE_NAME).collect::<Vec<_>>(),
            ["tossed", "xyz"]
        );
    }
}
