//! DXC（DirectXShaderCompiler）の実行ファイルを探して呼ぶ。
//!
//! 別プロセスで呼ぶのは、DXC が落ちても描画のプロセスを巻き込まないためと、ビルドに DXC の SDK を要らなくするため。
//! Windows SDK の `dxc.exe` は SPIR-V の出力を持たないので、試しに 1 つ変換できた候補だけを使う。

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

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
        let output = Command::new(&path).arg("--version").output().ok()?;
        let version = String::from_utf8_lossy(&output.stdout).trim().to_string();
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
    pub(crate) fn compile(&self, job: &DxcJob<'_>) -> Result<Vec<u8>, Vec<Diagnostic>> {
        self.compile_with(job, &["-E", "mm_fs"])
    }

    fn compile_with(&self, job: &DxcJob<'_>, extra: &[&str]) -> Result<Vec<u8>, Vec<Diagnostic>> {
        let io_failed = |e: std::io::Error| vec![Diagnostic::plain(format!("DXC を呼べない: {e}"))];
        let dir = tempfile::tempdir().map_err(io_failed)?;
        let input = dir.path().join(INPUT_NAME);
        let output = dir.path().join(OUTPUT_NAME);
        // BOM が無いと、DXC は源を SPIR-V に埋めるときに ASCII の外の文字（日本語のコメント）で失敗する。
        std::fs::write(&input, format!("{UTF8_BOM}{}", job.source)).map_err(io_failed)?;
        let sampler_shift = (job.inputs + 1).to_string();
        let run = Command::new(&self.path)
            .current_dir(dir.path())
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
            .args([INPUT_NAME, "-Fo", OUTPUT_NAME])
            .output()
            .map_err(io_failed)?;
        if !run.status.success() {
            let stderr = String::from_utf8_lossy(&run.stderr);
            return Err(Diagnostic::parse_dxc(&stderr));
        }
        std::fs::read(&output).map_err(io_failed)
    }
}

/// 探す順の候補。
fn candidates() -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = std::env::var_os(DXC_ENV)
        .map(PathBuf::from)
        .into_iter()
        .collect();
    out.extend(winget_candidates());
    if let Some(paths) = std::env::var_os("PATH") {
        out.extend(std::env::split_paths(&paths).map(|dir| dir.join(EXE)));
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
