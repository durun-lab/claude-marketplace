//! DXC（DirectXShaderCompiler）の実行ファイルを探して呼ぶ。
//!
//! 別プロセスで呼ぶのは、DXC が落ちても描画のプロセスを巻き込まないためと、ビルドに DXC の SDK を要らなくするため。
//! Windows SDK の `dxc.exe` は SPIR-V の出力を持たないので、試しに 1 つ変換できた候補だけを使う。

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use crate::convert::{ConvertError, Stage};
use crate::diag::Diagnostic;

/// DXC の実行ファイルを指す環境変数。最初に試す。
pub const DXC_ENV: &str = "MM_DXC";

/// winget の `Microsoft.DirectX.ShaderCompiler` が展開する場所（`%LOCALAPPDATA%` からの相対）。
const WINGET_DIR: &str = "Microsoft/WinGet/Packages";
/// winget のパッケージのフォルダー名の頭。
const WINGET_PACKAGE: &str = "Microsoft.DirectX.ShaderCompiler_";
/// 実行ファイルの名前。
const EXE: &str = if cfg!(windows) { "dxc.exe" } else { "dxc" };

/// 源のファイルの頭に付ける印。
const UTF8_BOM: char = '\u{feff}';
/// 一時のフォルダーの中の源と出力の名前。相対で渡し、SPIR-V に埋まる名前を毎回同じにする。
const INPUT_NAME: &str = "shader.hlsl";
const OUTPUT_NAME: &str = "shader.spv";

/// 1 回の DXC の呼び出しの期限。大きな `[unroll]` やマクロの膨張で止まらない源でも、描画のスレッドを止めない。
const DXC_TIMEOUT: Duration = Duration::from_secs(30);
/// 終わったかを見直す間隔。
const WAIT_STEP: Duration = Duration::from_millis(5);

/// 候補を試すための最小の源。
const PROBE_HLSL: &str = "float4 main() : SV_Target { return 1; }\n";

/// 使える DXC。`version` は変換のキャッシュのキーに入れ、DXC を入れ替えたら変換し直す。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dxc {
    path: PathBuf,
    version: String,
}

/// DXC の 1 回の呼び出しの入力。
pub(crate) struct DxcJob<'a> {
    /// HLSL の全文。
    pub source: &'a str,
    /// 入力のテクスチャの数。サンプラーの binding を決める。
    pub inputs: usize,
}

impl Dxc {
    /// 実行ファイルを試し、SPIR-V を出せれば返す。DXC は一時のフォルダーで動かすので、パスは絶対にして持つ。
    pub fn probe(path: &Path) -> Option<Self> {
        let path = std::path::absolute(path).ok()?;
        let output = run_bounded(
            mm_types::process::command(&path).arg("--version"),
            DXC_TIMEOUT,
        )
        .ok()?;
        let version = output.stdout.trim().to_string();
        let dxc = Self { path, version };
        let probe = DxcJob {
            source: PROBE_HLSL,
            inputs: 0,
        };
        dxc.compile_with(&probe, &["-E", "main"]).ok().map(|_| dxc)
    }

    /// 環境変数、winget のパッケージ、`PATH` の順に探し、最初に使えたもの。プロセスで 1 回だけ探す。
    pub fn discover() -> Option<Self> {
        static FOUND: OnceLock<Option<Dxc>> = OnceLock::new();
        FOUND
            .get_or_init(|| {
                let found = candidates().iter().find_map(|p| Self::probe(p));
                match &found {
                    Some(dxc) => tracing::info!(path = %dxc.path.display(), "DXC を使う"),
                    None => tracing::info!("SPIR-V を出せる DXC が無いので shader.hlsl は使えない"),
                }
                found
            })
            .clone()
    }

    /// 実行ファイルのパス。
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// `--version` の出力。
    pub fn version(&self) -> &str {
        &self.version
    }

    /// `mm_fs` を入口に、ピクセルシェーダーとして SPIR-V にする。
    /// 起動の失敗と時間切れは `Stage::Run`（キャッシュしない）、源の誤りは `Stage::Dxc`。
    pub(crate) fn compile(&self, job: &DxcJob<'_>) -> Result<Vec<u8>, ConvertError> {
        self.compile_with(job, &["-E", "mm_fs"])
    }

    fn compile_with(&self, job: &DxcJob<'_>, extra: &[&str]) -> Result<Vec<u8>, ConvertError> {
        let io_failed = |e: std::io::Error| ConvertError {
            stage: Stage::Run,
            diagnostics: vec![Diagnostic::plain(format!("DXC を呼べない: {e}"))],
        };
        if let Some(line) = include_line(job.source) {
            return Err(ConvertError {
                stage: Stage::Dxc,
                diagnostics: vec![Diagnostic::plain(format!(
                    "#include は使えない（源の外のファイルを読ませない）: {}",
                    line.trim()
                ))],
            });
        }
        let dir = tempfile::tempdir().map_err(io_failed)?;
        let input = dir.path().join(INPUT_NAME);
        let output = dir.path().join(OUTPUT_NAME);
        // BOM が無いと、DXC は源を SPIR-V に埋めるときに ASCII の外の文字（日本語のコメント）で失敗する。
        std::fs::write(&input, format!("{UTF8_BOM}{}", job.source)).map_err(io_failed)?;
        let sampler_shift = (job.inputs + 1).to_string();
        let mut cmd = mm_types::process::command(&self.path);
        cmd.current_dir(dir.path())
            .args(["-spirv", "-T", "ps_6_0", "-HV", "2021", "-fspv-debug=line"])
            .args([
                "-fvk-t-shift",
                "1",
                "0",
                "-fvk-s-shift",
                &sampler_shift,
                "0",
            ])
            .args(extra)
            .args([INPUT_NAME, "-Fo", OUTPUT_NAME]);
        let run = run_bounded(&mut cmd, DXC_TIMEOUT).map_err(io_failed)?;
        if !run.status.success() {
            return Err(ConvertError {
                stage: Stage::Dxc,
                diagnostics: Diagnostic::parse_dxc(&run.stderr),
            });
        }
        std::fs::read(&output).map_err(io_failed)
    }
}

/// 源の中の `#include` の行。DXC は取り込むファイルを絶対パスでも読み、その行を誤りに出すので、源の外を読ませない。
fn include_line(source: &str) -> Option<&str> {
    source.lines().find(|line| {
        line.trim_start()
            .strip_prefix('#')
            .is_some_and(|rest| rest.trim_start().starts_with("include"))
    })
}

/// 終わった子の出力。
struct Finished {
    status: ExitStatus,
    stdout: String,
    stderr: String,
}

/// `cmd` を `limit` まで待って実行する。過ぎたら止めて `TimedOut`。出力は別のスレッドで読み、パイプを詰まらせない。
fn run_bounded(cmd: &mut Command, limit: Duration) -> std::io::Result<Finished> {
    let mut child = cmd
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let read = |pipe: Option<Box<dyn Read + Send>>| {
        std::thread::spawn(move || {
            let mut text = Vec::new();
            if let Some(mut pipe) = pipe {
                let _ = pipe.read_to_end(&mut text);
            }
            String::from_utf8_lossy(&text).into_owned()
        })
    };
    let stdout = read(
        child
            .stdout
            .take()
            .map(|p| Box::new(p) as Box<dyn Read + Send>),
    );
    let stderr = read(
        child
            .stderr
            .take()
            .map(|p| Box::new(p) as Box<dyn Read + Send>),
    );
    let start = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if start.elapsed() >= limit {
            let _ = child.kill();
            return Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                format!("DXC が {limit:?} の間に終わらない"),
            ));
        }
        std::thread::sleep(WAIT_STEP);
    };
    let joined = |h: std::thread::JoinHandle<String>| h.join().unwrap_or_default();
    Ok(Finished {
        status,
        stdout: joined(stdout),
        stderr: joined(stderr),
    })
}

/// 探す順の候補。
fn candidates() -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = std::env::var_os(DXC_ENV)
        .map(PathBuf::from)
        .into_iter()
        .collect();
    out.extend(winget_candidates());
    if let Some(paths) = std::env::var_os("PATH") {
        // 空や相対の項目は作業フォルダーに解かれ、そこに置かれた dxc を起こしてしまうのでスキップする。
        out.extend(
            std::env::split_paths(&paths)
                .filter(|dir| dir.is_absolute())
                .map(|dir| dir.join(EXE)),
        );
    }
    out.retain(|p| p.is_file());
    out
}

/// winget で入れた DXC。x64 の実行ファイル。
fn winget_candidates() -> Vec<PathBuf> {
    let Some(local) = std::env::var_os("LOCALAPPDATA") else {
        return Vec::new();
    };
    let Ok(entries) = std::fs::read_dir(PathBuf::from(local).join(WINGET_DIR)) else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().starts_with(WINGET_PACKAGE))
        .map(|e| e.path().join("bin").join("x64").join(EXE))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn include_の行を見つける() {
        assert!(include_line(
            "float4 a;
  #  include \"C:/x.hlsl\"
"
        )
        .is_some());
        assert!(include_line("#include <x>").is_some());
        assert!(include_line(
            "// include は書ける
#define X 1
"
        )
        .is_none());
    }

    #[test]
    fn 同じ源と版からは同じ_spirv_を出す() {
        let Some(dxc) = Dxc::discover() else {
            eprintln!("SPIR-V を出せる DXC が無いのでスキップする");
            return;
        };
        let job = DxcJob {
            source:
                "float4 mm_fs(float2 uv : TEXCOORD0) : SV_Target { return float4(uv, 0, 1); }\n",
            inputs: 0,
        };
        let a = dxc.compile(&job).expect("変換できる");
        let b = dxc.compile(&job).expect("変換できる");
        assert_eq!(a, b, "一時のフォルダーの名前が SPIR-V に入らない");
    }
}
