//! 子プロセス。止めるときは木ごと止める（claude は node の子を持つ）。

use std::process::Command;

use mm_types::Result;

/// プロセスの木を止める。止められなくても失敗にしない（もう終わっていることが多い）。
pub fn kill_tree(pid: u32) {
    let pid = pid.to_string();
    let status = if cfg!(windows) {
        Command::new("taskkill")
            .args(["/PID", &pid, "/T", "/F"])
            .output()
    } else {
        Command::new("kill").args(["-TERM", &pid]).output()
    };
    drop(status);
}

/// 走らせた結果。
#[derive(Clone, Debug)]
pub struct Ran {
    /// 0 で終わったか。
    pub ok: bool,
    /// 標準出力。
    pub stdout: String,
    /// 標準エラー。
    pub stderr: String,
}

impl Ran {
    /// 標準出力と標準エラーの末尾（返事と待機の理由に添える）。
    pub fn tail(&self, lines: usize) -> String {
        let all = format!("{}\n{}", self.stdout, self.stderr);
        let v: Vec<&str> = all.lines().filter(|l| !l.trim().is_empty()).collect();
        v[v.len().saturating_sub(lines)..].join("\n")
    }
}

/// 走らせて、終わるまで待つ。
pub fn run(cmd: &mut Command) -> Result<Ran> {
    let out = cmd.output()?;
    Ok(Ran {
        ok: out.status.success(),
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
    })
}
