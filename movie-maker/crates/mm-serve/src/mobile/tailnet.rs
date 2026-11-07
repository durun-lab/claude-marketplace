//! この PC の tailnet での名前を `tailscale status --json` から読む。
//! 読み方は durun-dashboard の `server/platform.ts` の `hostsInTailscaleStatus` と同じ。

use std::process::Command;

use serde::Deserialize;

/// 試す `tailscale` の CLI。PATH に無いときは Windows の既定のインストール先を見る。
const TAILSCALE_COMMANDS: &[&str] = &["tailscale", r"C:\Program Files\Tailscale\tailscale.exe"];

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

/// `tailscale serve status --json` の出力が、`https_port` の HTTPS を `127.0.0.1:<port>` へ中継しているか。
/// 何も設定していないと出力は `{}`。
pub fn serve_proxies(json: &str, https_port: u16, port: u16) -> bool {
    let Ok(status) = serde_json::from_str::<ServeStatus>(json) else {
        return false;
    };
    let suffix = format!(":{https_port}");
    let targets = [format!("127.0.0.1:{port}"), format!("localhost:{port}")];
    status
        .web
        .iter()
        .filter(|(site, _)| site.ends_with(&suffix))
        .flat_map(|(_, web)| web.handlers.values())
        .filter_map(|h| h.proxy.as_deref())
        .any(|proxy| targets.iter().any(|t| proxy.ends_with(t.as_str())))
}

/// この PC の `tailscale serve` の設定が人の待ち受けを tailnet に出しているか。CLI が無ければ `false`。
pub fn serve_is_configured(https_port: u16, port: u16) -> bool {
    tailscale_output(&["serve", "status", "--json"])
        .is_some_and(|json| serve_proxies(&json, https_port, port))
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "PascalCase", default)]
struct ServeStatus {
    web: std::collections::HashMap<String, ServeWeb>,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "PascalCase", default)]
struct ServeWeb {
    handlers: std::collections::HashMap<String, ServeHandler>,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "PascalCase", default)]
struct ServeHandler {
    proxy: Option<String>,
}

/// `tailscale` の CLI を呼び、成功したときの標準出力を返す。CLI が無いか失敗すれば `None`。
fn tailscale_output(args: &[&str]) -> Option<String> {
    for command in TAILSCALE_COMMANDS {
        let mut cmd = Command::new(command);
        cmd.args(args);
        hide_console(&mut cmd);
        match cmd.output() {
            Ok(out) if out.status.success() => {
                return Some(String::from_utf8_lossy(&out.stdout).into_owned())
            }
            Ok(out) => {
                tracing::warn!("tailscale {} が失敗した: {}", args[0], out.status);
                return None;
            }
            Err(_) => continue,
        }
    }
    None
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
}
