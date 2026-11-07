//! JSON のファイルの読み書き。アプリと CLI が同じファイルを読むので、書いている途中の内容を見せないよう一時ファイルから名前を変える。

use std::path::Path;

use mm_types::Result;
use serde::de::DeserializeOwned;
use serde::Serialize;

/// 読む。ファイルが無ければ `None`。
pub(crate) fn read_json<T: DeserializeOwned>(path: &Path) -> Result<Option<T>> {
    match std::fs::read(path) {
        Ok(bytes) => Ok(Some(serde_json::from_slice(&bytes)?)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    }
}

/// 読む。ファイルが無ければ既定値。
pub(crate) fn read_or_default<T: DeserializeOwned + Default>(path: &Path) -> Result<T> {
    Ok(read_json(path)?.unwrap_or_default())
}

/// 一時ファイルに書いてから名前を変える。
pub(crate) fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(value)?)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}
