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
    for command in TAILSCALE_COMMANDS {
        let mut cmd = Command::new(command);
        cmd.args(["status", "--json", "--peers=false"]);
        hide_console(&mut cmd);
        match cmd.output() {
            Ok(out) if out.status.success() => {
                return hosts_in_status(&String::from_utf8_lossy(&out.stdout))
            }
            Ok(out) => tracing::warn!("tailscale status が失敗した: {}", out.status),
            Err(_) => continue,
        }
    }
    Vec::new()
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
}
