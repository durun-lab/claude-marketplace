//! ホストの実行ファイルと、ホストに読ませるプラグインの置き場所の決まり（`ofx`、`audio.clap`、`audio.vst3` で共通）。
//!
//! プロジェクトのデータは他人が書いたものでもあり、任意のパスの共有ライブラリをホストに読ませるとそのコードが動く。
//! そのため、パラメータで指すプラグインは規格の置き場所の中だけを受ける。シンボリックリンクとジャンクションで
//! 外へ出られないよう、実体のパスに直してから置き場所の中かを確かめる。

use std::path::{Component, Path, PathBuf};

use mm_types::{MmError, Result};

/// `env` の環境変数か、このプロセスの実行ファイルと同じフォルダー、その親のフォルダーの `stem`（拡張子なし）。
/// 親も見るのは、テストの実行ファイル（`target/<profile>/deps/`）から `target/<profile>/` のホストを引くため。
pub fn find_executable(env: &str, stem: &str) -> Option<PathBuf> {
    if let Some(path) = std::env::var_os(env) {
        return Some(PathBuf::from(path)).filter(|p| p.is_file());
    }
    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?;
    let name = format!("{stem}{}", std::env::consts::EXE_SUFFIX);
    let found = [Some(dir), dir.parent()]
        .into_iter()
        .flatten()
        .map(|d: &Path| d.join(&name))
        .find(|p| p.is_file());
    found
}

/// パラメータのプラグインの名前を、置き場所 `roots` の中の実体のパスにする。
/// `is_plugin` は名前（拡張子や末尾）でプラグインかを見る。相対の名前は `roots` の順に探し、`..` やドライブを
/// 含む名前は拒否する。絶対パスは置き場所の中だけを受ける。
pub fn resolve_in_roots(
    roots: &[PathBuf],
    name: &str,
    is_plugin: fn(&Path) -> bool,
) -> Result<PathBuf> {
    let path = Path::new(name);
    if !is_plugin(path) {
        return Err(MmError::InvalidArgument(format!(
            "{name} はこの規格のプラグインの名前でない"
        )));
    }
    let candidates: Vec<PathBuf> = if path.is_absolute() {
        vec![path.to_path_buf()]
    } else if path.components().all(|c| matches!(c, Component::Normal(_))) {
        roots.iter().map(|r| r.join(path)).collect()
    } else {
        return Err(MmError::InvalidArgument(format!(
            "{name} は置き場所からの相対の名前でない（.. やドライブは使えない）"
        )));
    };
    let Some(real) = candidates.iter().find_map(|c| c.canonicalize().ok()) else {
        return Err(MmError::NotFound(format!(
            "プラグイン {name} が置き場所 {roots:?} に無い"
        )));
    };
    let inside = roots
        .iter()
        .filter_map(|r| r.canonicalize().ok())
        .any(|r| real.starts_with(r));
    if inside && is_plugin(&real) {
        Ok(real)
    } else {
        Err(MmError::InvalidArgument(format!(
            "{} はプラグインの置き場所 {roots:?} の外",
            real.display()
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn is_clap(path: &Path) -> bool {
        path.extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("clap"))
    }

    #[test]
    fn 置き場所の中のプラグインだけを受ける() {
        let guard = tempfile::Builder::new()
            .prefix("mm-host-pipe-locate-")
            .tempdir()
            .unwrap();
        let tmp = guard.path().to_path_buf();
        let root = tmp.join("plugins");
        std::fs::create_dir_all(root.join("Vendor")).unwrap();
        std::fs::write(root.join("Vendor/A.clap"), b"").unwrap();
        std::fs::create_dir_all(tmp.join("outside")).unwrap();
        std::fs::write(tmp.join("outside/B.clap"), b"").unwrap();
        let roots = vec![tmp.join("none"), root.clone()];

        let real = resolve_in_roots(&roots, "Vendor/A.clap", is_clap).expect("相対の名前");
        assert!(real.ends_with("A.clap"));
        let abs = root.join("Vendor/A.clap");
        assert_eq!(
            resolve_in_roots(&roots, abs.to_str().unwrap(), is_clap).unwrap(),
            real
        );

        let outside = tmp.join("outside/B.clap");
        let err = resolve_in_roots(&roots, outside.to_str().unwrap(), is_clap);
        assert!(matches!(err, Err(MmError::InvalidArgument(_))), "{err:?}");
        let up = resolve_in_roots(&roots, "../outside/B.clap", is_clap);
        assert!(matches!(up, Err(MmError::InvalidArgument(_))), "{up:?}");
        let none = resolve_in_roots(&roots, "C.clap", is_clap);
        assert!(matches!(none, Err(MmError::NotFound(_))), "{none:?}");
        let dll = resolve_in_roots(&roots, "Vendor/A.dll", is_clap);
        assert!(matches!(dll, Err(MmError::InvalidArgument(_))), "{dll:?}");
    }
}
