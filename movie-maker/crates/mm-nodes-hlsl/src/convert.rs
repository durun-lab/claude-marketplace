//! HLSL → SPIR-V（DXC）→ WGSL（naga）の変換と、そのキャッシュ。
//!
//! キャッシュは DXC のバージョンと全文のハッシュで引き、源の失敗もキャッシュする（毎フレーム DXC を起こさない）。
//! DXC を起こせない失敗と時間切れ（`Stage::Run`）は一過性なのでキャッシュしない。並行の書き出しは
//! ワーカーごとに組み立てるので、キャッシュはプロセスで 1 つにする。

use std::collections::HashMap;
use std::fmt;
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, PoisonError};
use std::time::Instant;

use naga::back::wgsl::WriterFlags;
use naga::front::spv;
use naga::valid::{Capabilities, ValidationFlags, Validator};

use crate::diag::Diagnostic;
use crate::dxc::{Dxc, DxcJob};
use crate::source::{full_source, line_label};
use crate::spirv_lines;

/// 覚える変換の数の上限。毎フレーム違う源が来ても増え続けない。
const MAX_ENTRIES: usize = 64;

/// 変換で失敗した段。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    /// DXC を起こせないか、期限までに終わらない。
    Run,
    /// DXC が HLSL を SPIR-V にできなかった。
    Dxc,
    /// naga が SPIR-V を読めないか、WGSL にできなかった。
    Naga,
}

/// 変換の失敗。最初の誤りを先頭に、DXC の誤りを全部持つ。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConvertError {
    /// 失敗した段。
    pub stage: Stage,
    /// 誤り。1 件以上。
    pub diagnostics: Vec<Diagnostic>,
}

impl fmt::Display for ConvertError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let stage = match self.stage {
            Stage::Run => "DXC を実行できない",
            Stage::Dxc => "DXC が HLSL を SPIR-V にできない",
            Stage::Naga => "naga が SPIR-V を WGSL にできない",
        };
        write!(f, "{stage}: ")?;
        let mut items = self.diagnostics.iter();
        if let Some(first) = items.next() {
            write!(f, "{first}")?;
        }
        let rest = items.count();
        if rest > 0 {
            write!(f, "（ほかに {rest} 件）")?;
        }
        Ok(())
    }
}

/// 変換の結果。WGSL の全文か、失敗。
pub type Converted = Result<Arc<str>, ConvertError>;

type Cache = HashMap<blake3::Hash, Converted>;

fn cache() -> &'static Mutex<Cache> {
    static CACHE: OnceLock<Mutex<Cache>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// 利用者の HLSL を、入力の数に合わせた束ね方の WGSL にする。`label` は誤りに出す源の名前。
pub fn convert(dxc: &Dxc, code: &str, label: &str, inputs: usize) -> Converted {
    let label = &line_label(label);
    let source = full_source(code, label, inputs);
    let mut hasher = blake3::Hasher::new();
    hasher.update(dxc.version().as_bytes());
    hasher.update(&[0, inputs as u8]);
    hasher.update(source.as_bytes());
    let key = hasher.finalize();
    if let Some(hit) = lock().get(&key) {
        return hit.clone();
    }
    let started = Instant::now();
    let result = convert_uncached(dxc, &source, label, inputs);
    tracing::debug!(
        ms = started.elapsed().as_secs_f64() * 1e3,
        ok = result.is_ok(),
        "HLSL を WGSL に変換した"
    );
    if matches!(&result, Err(e) if e.stage == Stage::Run) {
        return result;
    }
    let mut guard = lock();
    if guard.len() >= MAX_ENTRIES {
        guard.clear();
    }
    guard.insert(key, result.clone());
    result
}

fn lock() -> MutexGuard<'static, Cache> {
    cache().lock().unwrap_or_else(PoisonError::into_inner)
}

fn convert_uncached(dxc: &Dxc, source: &str, label: &str, inputs: usize) -> Converted {
    let spirv = dxc.compile(&DxcJob { source, inputs })?;
    spirv_to_wgsl(&spirv, label)
        .map(Arc::from)
        .map_err(|d| ConvertError {
            stage: Stage::Naga,
            diagnostics: vec![d],
        })
}

/// SPIR-V を naga で読み、検証して WGSL を書く。読めなければ、つまずいた命令の利用者の行を付ける。
pub(crate) fn spirv_to_wgsl(spirv: &[u8], label: &str) -> Result<String, Diagnostic> {
    let options = spv::Options {
        adjust_coordinate_space: false,
        strict_capabilities: true,
        block_ctx_dump_prefix: None,
    };
    let module = spv::parse_u8_slice(&spirv_lines::strip_lines(spirv), &options).map_err(|e| {
        let at = spirv_lines::locate(spirv, label, &e);
        Diagnostic {
            file: at.map(|_| label.to_string()),
            line: at.map(|a| a.line),
            column: at.map(|a| a.column),
            message: format!("SPIR-V を読めない: {e}"),
        }
    })?;
    let info = Validator::new(ValidationFlags::all(), Capabilities::default())
        .validate(&module)
        .map_err(|e| Diagnostic::plain(format!("naga の検証: {}", e.as_inner())))?;
    naga::back::wgsl::write_string(&module, &info, WriterFlags::empty())
        .map_err(|e| Diagnostic::plain(format!("WGSL を書けない: {e}")))
}
