//! バンドルの置き場所の決まり。プロジェクトのデータは他人が書いたものでもあり、任意のパスの DLL をホストに読ませると
//! そのコードが動くので、ホストに渡すのは OpenFX の置き場所（組み立てが渡す `OFX_PLUGIN_PATH` と OS の既定のフォルダー）の
//! 中のバンドルと、プロジェクトの素材のバンドルだけにする。シンボリックリンクとジャンクションで外へ出られないよう、
//! 実体のパスに直してから置き場所の中かを確かめる。

use std::path::{Path, PathBuf};

use mm_types::{MmError, Result};

/// バンドルのフォルダー名の末尾（OpenFX の仕様の「Packaging OFX Plug-ins」）。
pub const BUNDLE_SUFFIX: &str = ".ofx.bundle";

/// 名前が `*.ofx.bundle` か。
pub fn is_bundle_name(path: &Path) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|n| n.len() > BUNDLE_SUFFIX.len() && n.ends_with(BUNDLE_SUFFIX))
}

/// パラメータ `bundle` の名前を、置き場所 `roots` の中のバンドルの実体のパスにする。
/// 相対の名前は `roots` の順に探し、`..` やドライブを含む名前は拒否する。絶対パスは置き場所の中だけを受ける。
pub fn resolve_named(roots: &[PathBuf], name: &str) -> Result<PathBuf> {
    let real = mm_host_pipe::resolve_in_roots(roots, name, is_bundle_name)?;
    if real.is_dir() {
        Ok(real)
    } else {
        Err(MmError::InvalidArgument(format!(
            "{} はバンドルのフォルダーでない",
            real.display()
        )))
    }
}

/// プロジェクトの素材のパスを、バンドルの実体のパスにする。素材がバンドルの中のファイル（`Contents/Win64/Foo.ofx`）なら
/// 上へたどってバンドルのフォルダーを返す。
pub fn resolve_asset(path: &Path) -> Result<PathBuf> {
    let real = path.canonicalize().map_err(|e| {
        MmError::NotFound(format!("素材のバンドル {} を開けない: {e}", path.display()))
    })?;
    real.ancestors()
        .find(|p| is_bundle_name(p) && p.is_dir())
        .map(Path::to_path_buf)
        .ok_or_else(|| {
            MmError::InvalidArgument(format!(
                "素材 {} はバンドル（*{BUNDLE_SUFFIX}）でも、その中のファイルでもない",
                path.display()
            ))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `roots/Vendor/A.ofx.bundle/Contents/Win64/A.ofx` と、置き場所の外の `outside/B.ofx.bundle` を作る。
    fn tree(tag: &str) -> (PathBuf, PathBuf) {
        let base = std::env::temp_dir().join(format!("mm-nodes-ofx-{tag}-{}", std::process::id()));
        let root = base.join("plugins");
        std::fs::create_dir_all(root.join("Vendor/A.ofx.bundle/Contents/Win64")).unwrap();
        std::fs::write(root.join("Vendor/A.ofx.bundle/Contents/Win64/A.ofx"), b"").unwrap();
        std::fs::create_dir_all(base.join("outside/B.ofx.bundle")).unwrap();
        (base, root)
    }

    #[test]
    fn 置き場所の中のバンドルだけを受ける() {
        let (base, root) = tree("named");
        let roots = vec![base.join("none"), root.clone()];
        let real = resolve_named(&roots, "Vendor/A.ofx.bundle").expect("相対の名前");
        assert!(real.ends_with("A.ofx.bundle"));
        let abs = root.join("Vendor/A.ofx.bundle");
        assert_eq!(resolve_named(&roots, abs.to_str().unwrap()).unwrap(), real);

        let outside = base.join("outside/B.ofx.bundle");
        let err = resolve_named(&roots, outside.to_str().unwrap());
        assert!(matches!(err, Err(MmError::InvalidArgument(_))), "{err:?}");
        let up = resolve_named(&roots, "../outside/B.ofx.bundle");
        assert!(matches!(up, Err(MmError::InvalidArgument(_))), "{up:?}");
        let none = resolve_named(&roots, "C.ofx.bundle");
        assert!(matches!(none, Err(MmError::NotFound(_))), "{none:?}");
        let not_bundle = resolve_named(&roots, "Vendor");
        assert!(matches!(not_bundle, Err(MmError::InvalidArgument(_))));
        std::fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn 素材はバンドルかその中のファイル() {
        let (base, root) = tree("asset");
        let bundle = root.join("Vendor/A.ofx.bundle");
        let real = resolve_asset(&bundle).expect("バンドルのフォルダー");
        let inner = resolve_asset(&bundle.join("Contents/Win64/A.ofx")).expect("中のファイル");
        assert_eq!(real, inner);
        let err = resolve_asset(&root.join("Vendor"));
        assert!(matches!(err, Err(MmError::InvalidArgument(_))), "{err:?}");
        std::fs::remove_dir_all(&base).unwrap();
    }
}
