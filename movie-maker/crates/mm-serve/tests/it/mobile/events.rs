//! 人の待ち受けの SSE。端末の登録を外したら、開いている列を閉じる。

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpStream};
use std::time::Duration;

use super::{rig, Req, Rig};

/// 列が閉じるのを待つ上限。閉じなければ読み出しが時間切れで落ちる。
const CLOSE_TIMEOUT: Duration = Duration::from_secs(10);

/// `GET /m/api/events` を開き、`hello` を読んだところで返す。
fn open_events(r: &Rig, req: &Req) -> BufReader<TcpStream> {
    let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, r.mobile.port()));
    let mut stream = TcpStream::connect(addr).unwrap();
    stream.set_read_timeout(Some(CLOSE_TIMEOUT)).unwrap();
    let mut head = "GET /m/api/events HTTP/1.0\r\n".to_owned();
    for (k, v) in req.headers() {
        head.push_str(&format!("{k}: {v}\r\n"));
    }
    head.push_str("\r\n");
    stream.write_all(head.as_bytes()).unwrap();
    let mut reader = BufReader::new(stream);
    let mut status = String::new();
    reader.read_line(&mut status).unwrap();
    assert!(status.contains(" 200 "), "{status}");
    loop {
        let mut line = String::new();
        assert_ne!(
            reader.read_line(&mut line).unwrap(),
            0,
            "hello の前に閉じた"
        );
        if line.trim_end() == "event: hello" {
            return reader;
        }
    }
}

#[test]
fn 登録を外した端末の_sse_は閉じ_ほかの端末の_sse_は続く() {
    let r = rig();
    let phone = Req::device(&r.pair("phone"));
    let tablet = Req::device(&r.pair("tablet"));
    let mut phone_events = open_events(&r, &phone);
    let mut tablet_events = open_events(&r, &tablet);
    let phone_id = r.mobile.devices().list().unwrap()[0].id.clone();

    assert!(r.mobile.devices().revoke(&phone_id).unwrap());
    let mut rest = Vec::new();
    phone_events
        .read_to_end(&mut rest)
        .expect("登録を外した端末の SSE が閉じない");

    // 外していない端末の列には、外した後の変更も届く。
    let marker = serde_json::json!({
        "op": {"op": "addMarker", "composition": "main",
               "marker": {"id": "m1", "time": {"num": 1, "den": 1}, "name": "見る"}},
        "base_version": 0,
    });
    assert_eq!(
        r.send(&tablet, "POST", "/m/api/ops", Some(&marker)).status,
        200
    );
    loop {
        let mut line = String::new();
        assert_ne!(tablet_events.read_line(&mut line).unwrap(), 0, "列が閉じた");
        if line.trim_end() == "event: change" {
            break;
        }
    }
    let res = r.send(&phone, "GET", "/m/api/events", None);
    assert_eq!(res.status, 401, "繋ぎ直しは断る");
}
