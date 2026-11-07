//! この PC の tailnet での名前を `tailscale status --json` から読む。
//! 読み方は durun-dashboard の `server/platform.ts` の `hostsInTailscaleStatus` と同じ。

use std::collections::HashMap;
use std::io::Read;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

use serde::Deserialize;

/// 試す `tailscale` の CLI。PATH に無いときは Windows の既定のインストール先を見る。
const TAILSCALE_COMMANDS: &[&str] = &["tailscale", r"C:\Program Files\Tailscale\tailscale.exe"];
/// CLI の答えを待つ上限。ふだんは 0.1 秒ほどで返る。
const CLI_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Status {
    backend_state: Option<String>,
    #[serde(rename = "Self")]
    me: Option<Peer>,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Peer {
    #[serde(rename = "DNSName")]
    dns_name: Option<String>,
}

/// `tailscale status --json` の出力から、この PC の tailnet での名前（`<端末>.<tailnet>.ts.net`）を返す。
/// Tailscale が動いていなければ空。`tailscale serve` の HTTPS の証明書はこの名前にだけ出るので、短い名前と IP は返さない。
pub fn hosts_in_status(json: &str) -> Vec<String> {
    let Ok(status) = serde_json::from_str::<Status>(json) else {
        return Vec::new();
    };
    if status.backend_state.as_deref() != Some("Running") {
        return Vec::new();
    }
    status
        .me
        .and_then(|p| p.dns_name)
        .map(|n| n.trim_end_matches('.').to_ascii_lowercase())
        .filter(|n| !n.is_empty())
        .into_iter()
        .collect()
}

/// この PC の tailnet での名前を `tailscale` の CLI に聞く。CLI が無いか Tailscale が止まっていれば空。
pub fn tailnet_hosts() -> Vec<String> {
    tailscale_output(&["status", "--json", "--peers=false"])
        .map(|json| hosts_in_status(&json))
        .unwrap_or_default()
}

/// `tailscale serve status --json` の出力（tailscaled の `ipn.ServeConfig`）が、`https_port` の HTTPS を
/// この PC の `port` へ中継しているか。何も設定していないと出力は `{}`。`--bg` なしで動かしている間の設定は
/// `Foreground` の下に入るので、そこも見る。
pub fn serve_proxies(json: &str, https_port: u16, port: u16) -> bool {
    serde_json::from_str::<ServeConfig>(json).is_ok_and(|c| c.proxies(https_port, port))
}

/// この PC の `tailscale serve` の設定が人の待ち受けを tailnet に出しているか。CLI が無いか、
/// 時間内に答えなければ `false`。
pub fn serve_is_configured(https_port: u16, port: u16) -> bool {
    tailscale_output(&["serve", "status", "--json"])
        .is_some_and(|json| serve_proxies(&json, https_port, port))
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "PascalCase", default)]
struct ServeConfig {
    #[serde(rename = "TCP")]
    tcp: HashMap<String, ServeTcp>,
    web: HashMap<String, ServeWeb>,
    foreground: HashMap<String, ServeConfig>,
}

impl ServeConfig {
    fn proxies(&self, https_port: u16, port: u16) -> bool {
        // `--http=8443` の設定では Secure のクッキーと Service Worker が使えないので、HTTPS のときだけ数える。
        let https = self
            .tcp
            .get(&https_port.to_string())
            .is_some_and(|t| t.https);
        let suffix = format!(":{https_port}");
        let here = https
            && self
                .web
                .iter()
                .filter(|(site, _)| site.ends_with(&suffix))
                .flat_map(|(_, web)| web.handlers.values())
                .filter_map(|h| h.proxy.as_deref())
                .any(|proxy| is_local_port(proxy, port));
        here || self
            .foreground
            .values()
            .any(|c| c.proxies(https_port, port))
    }
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct ServeTcp {
    #[serde(rename = "HTTPS")]
    https: bool,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "PascalCase", default)]
struct ServeWeb {
    handlers: HashMap<String, ServeHandler>,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "PascalCase", default)]
struct ServeHandler {
    proxy: Option<String>,
}

/// 中継先がこの PC の `port` か（`http://127.0.0.1:7338`、`localhost:7338/`、`http://[::1]:7338` など）。
fn is_local_port(proxy: &str, port: u16) -> bool {
    let rest = proxy.strip_prefix("http://").unwrap_or(proxy);
    let Some((host, p)) = rest.trim_end_matches('/').rsplit_once(':') else {
        return false;
    };
    matches!(host, "127.0.0.1" | "localhost" | "[::1]") && p.parse::<u16>() == Ok(port)
}

/// `tailscale` の CLI を呼び、成功したときの標準出力を返す。CLI が無いか、失敗したか、時間内に答えなければ `None`。
fn tailscale_output(args: &[&str]) -> Option<String> {
    for command in TAILSCALE_COMMANDS {
        let mut cmd = Command::new(command);
        cmd.args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        hide_console(&mut cmd);
        match cmd.spawn() {
            Ok(child) => return finish(child, args[0]),
            Err(_) => continue,
        }
    }
    None
}

/// 標準出力を別のスレッドで読み切って待つ。tailscaled が止まりかけていると CLI が返らないことがあるので、
/// `CLI_TIMEOUT` を過ぎたら子のプロセスを止めて諦める。
fn finish(mut child: Child, what: &str) -> Option<String> {
    let mut stdout = child.stdout.take()?;
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut out = Vec::new();
        let _ = stdout.read_to_end(&mut out);
        let _ = tx.send(out);
    });
    let Ok(out) = rx.recv_timeout(CLI_TIMEOUT) else {
        tracing::warn!("tailscale {what} が {CLI_TIMEOUT:?} のうちに答えない");
        let _ = child.kill();
        let _ = child.wait();
        return None;
    };
    match child.wait() {
        Ok(status) if status.success() => Some(String::from_utf8_lossy(&out).into_owned()),
        Ok(status) => {
            tracing::warn!("tailscale {what} が失敗した: {status}");
            None
        }
        Err(e) => {
            tracing::warn!("tailscale {what} の終わりを待てない: {e}");
            None
        }
    }
}

/// 画面のアプリから呼んでも、コンソールの窓を出さない。
#[cfg(windows)]
fn hide_console(cmd: &mut Command) {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    cmd.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(not(windows))]
fn hide_console(_cmd: &mut Command) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 動いている_tailnet_の名前だけを末尾の点なしで返す() {
        let running = r#"{"BackendState":"Running","Self":{"DNSName":"Desk.tail1234.ts.net.","TailscaleIPs":["100.1.2.3"]}}"#;
        assert_eq!(hosts_in_status(running), ["desk.tail1234.ts.net"]);
        let stopped = r#"{"BackendState":"Stopped","Self":{"DNSName":"desk.tail1234.ts.net."}}"#;
        assert!(hosts_in_status(stopped).is_empty());
        assert!(hosts_in_status("not json").is_empty());
    }

    #[test]
    fn serve_の設定は_https_のポートと中継先の両方が合うときだけ真() {
        let set = r#"{"TCP":{"443":{"HTTPS":true},"8443":{"HTTPS":true}},"Web":{
            "desk.tail1234.ts.net:443":{"Handlers":{"/":{"Proxy":"http://127.0.0.1:7337"}}},
            "desk.tail1234.ts.net:8443":{"Handlers":{"/":{"Proxy":"http://127.0.0.1:7338"}}}}}"#;
        assert!(serve_proxies(set, 8443, 7338));
        assert!(!serve_proxies(set, 8443, 7339), "中継先のポートが違う");
        assert!(!serve_proxies(set, 9443, 7338), "HTTPS のポートが違う");
        assert!(!serve_proxies("{}", 8443, 7338));
        assert!(!serve_proxies("not json", 8443, 7338));
    }

    #[test]
    fn serve_の設定は_http_のポートを数えず_書き方の違いと_foreground_を受ける() {
        let http = r#"{"TCP":{"8443":{"HTTP":true}},"Web":{
            "desk.tail1234.ts.net:8443":{"Handlers":{"/":{"Proxy":"http://127.0.0.1:7338"}}}}}"#;
        assert!(!serve_proxies(http, 8443, 7338), "HTTPS でない");
        let slash = r#"{"TCP":{"8443":{"HTTPS":true}},"Web":{
            "desk.tail1234.ts.net:8443":{"Handlers":{"/":{"Proxy":"http://localhost:7338/"}}}}}"#;
        assert!(serve_proxies(slash, 8443, 7338));
        let other_host = r#"{"TCP":{"8443":{"HTTPS":true}},"Web":{
            "desk.tail1234.ts.net:8443":{"Handlers":{"/":{"Proxy":"http://10.0.0.2:7338"}}}}}"#;
        assert!(!serve_proxies(other_host, 8443, 7338), "別の PC へ中継");
        let foreground = r#"{"Foreground":{"s1":{"TCP":{"8443":{"HTTPS":true}},"Web":{
            "desk.tail1234.ts.net:8443":{"Handlers":{"/":{"Proxy":"127.0.0.1:7338"}}}}}}}"#;
        assert!(serve_proxies(foreground, 8443, 7338));
    }

    #[test]
    fn 中継先はこの_pc_のポートだけを数える() {
        assert!(is_local_port("http://127.0.0.1:7338", 7338));
        assert!(is_local_port("http://[::1]:7338", 7338));
        assert!(!is_local_port("http://127.0.0.1:17338", 7338));
        assert!(!is_local_port("https+insecure://127.0.0.1:7338", 7338));
        assert!(!is_local_port("text:hello", 7338));
    }
}
