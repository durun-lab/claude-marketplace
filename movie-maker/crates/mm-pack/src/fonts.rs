//! パックの書体の目録（`fonts/fonts.json`）と、プロジェクトへの取り込み。
//!
//! 書体の本体はパックに入れず、目録が配布元の URL（固定のコミット）と BLAKE3 を持つ。ユーザーのキャッシュに
//! 取ったもの（[`crate::FontCache`]）を、適用するときにプロジェクトのフォルダーの `fonts/<family>/` へ写して
//! 素材（`addAsset`）として足す。プロジェクトは書体を持つので、別の PC に移しても同じ書体で描ける。
//! パックの `fonts/<family>/` にはライセンスの文だけを置き、書体と一緒にプロジェクトへ写す。

use std::path::{Path, PathBuf};

use mm_doc::{Asset, Project};
use mm_ops::{AddAsset, Op};
use mm_text_api::{is_font_path, FontFile, FONTS_DIR};
use mm_types::{ColorSpace, Id, MmError, RationalTime, Result};
use serde::Deserialize;

/// パックの中の書体の目録のファイル名（`fonts/` の下）。
pub(crate) const MANIFEST: &str = "fonts.json";
/// 書体の素材の ID の接頭辞。後ろはファイル名（拡張子を除く）を英小文字と数字と `-` にしたもの。
const ASSET_PREFIX: &str = "font-";
/// BLAKE3 の 16 進の桁数。
const HASH_HEX_LEN: usize = 64;
/// ライセンスの文のファイル名の頭（`OFL.txt`、`LICENSE.txt` など）。書体と一緒に写す。
const LICENSE_HEADS: [&str; 2] = ["ofl", "license"];

/// 目録の書体のファイル 1 つ。
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackFont {
    /// family の名前。テーマと CSS が書く名前で、プロジェクトの `fonts/` の下のフォルダーの名前になる。
    pub family: String,
    /// ファイル名。
    pub file: String,
    /// 配布元の URL。中身が変わらないように固定のコミットを指す。
    pub url: String,
    /// 中身の BLAKE3（16 進）。取ったファイルと照らし、違えば使わない。
    pub blake3: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    fonts: Vec<PackFont>,
}

impl PackFont {
    /// プロジェクトのフォルダーからの場所（`fonts/<family>/<ファイル>`、`/` 区切り）。
    pub fn project_path(&self) -> String {
        format!("{FONTS_DIR}/{}/{}", self.family, self.file)
    }

    /// 素材の ID。
    pub fn asset_id(&self) -> String {
        let stem = Path::new(&self.file)
            .file_stem()
            .map(|s| s.to_string_lossy())
            .unwrap_or_default();
        let slug: String = stem
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() {
                    c.to_ascii_lowercase()
                } else {
                    '-'
                }
            })
            .collect();
        format!("{ASSET_PREFIX}{slug}")
    }
}

/// キャッシュにあり、ハッシュの合った書体。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CachedFont {
    /// 目録の書体。
    pub font: PackFont,
    /// キャッシュの中のファイル。
    pub path: PathBuf,
}

/// `fonts/fonts.json` を読む。無ければ空（書体を持たないパックは OS の書体で描く）。
/// family、ファイル名、ハッシュはキャッシュとプロジェクトのフォルダーやファイルの名前になるので、
/// 区切りや `..` を含むもの、64 桁の 16 進でないハッシュは誤りにする。
pub(crate) fn read_fonts(dir: &Path) -> Result<Vec<PackFont>> {
    let path = dir.join(MANIFEST);
    let Some(manifest) = crate::pack::read_optional::<Manifest>(&path)? else {
        return Ok(Vec::new());
    };
    for font in &manifest.fonts {
        let names_ok = [&font.family, &font.file].iter().all(|n| is_plain_name(n));
        if !names_ok || !is_hash(&font.blake3) {
            return Err(MmError::InvalidArgument(format!(
                "{}: family、file、blake3 をフォルダーやファイルの名前にできない: {font:?}",
                path.display()
            )));
        }
    }
    Ok(manifest.fonts)
}

/// BLAKE3 の 16 進（64 桁）か。
fn is_hash(text: &str) -> bool {
    text.len() == HASH_HEX_LEN && text.chars().all(|c| c.is_ascii_hexdigit())
}

/// 1 つのフォルダーの中の名前として使えるか（空、区切り、`.` と `..` を除く）。
fn is_plain_name(name: &str) -> bool {
    !name.is_empty() && name != "." && name != ".." && !name.contains(['/', '\\', ':'])
}

/// フォルダーの中身を名前の順に。フォルダーが無ければ空。
fn sorted_entries(dir: &Path) -> Result<Vec<PathBuf>> {
    if !dir.is_dir() {
        return Ok(Vec::new());
    }
    let mut paths: Vec<PathBuf> = std::fs::read_dir(dir)
        .map_err(|e| MmError::NotFound(format!("{}: {e}", dir.display())))?
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .collect();
    paths.sort();
    Ok(paths)
}

/// ライセンスの文のファイルか。
fn is_license(path: &Path) -> bool {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_ascii_lowercase());
    name.is_some_and(|n| LICENSE_HEADS.iter().any(|h| n.starts_with(h)))
}

/// family のフォルダーのライセンスの文。
fn licenses(family_dir: &Path) -> Result<Vec<PathBuf>> {
    Ok(sorted_entries(family_dir)?
        .into_iter()
        .filter(|p| p.is_file() && is_license(p))
        .collect())
}

/// family のフォルダーにライセンスの文があるか。
pub(crate) fn has_license(family_dir: &Path) -> bool {
    licenses(family_dir).is_ok_and(|l| !l.is_empty())
}

/// 書体を、文字を組む実装に渡す形にする（プレートの大きさを描くときと同じ書体で測るため）。
pub fn font_files(fonts: &[CachedFont]) -> Vec<FontFile> {
    fonts
        .iter()
        .map(|f| FontFile {
            path: f.path.clone(),
            content_hash: f.font.blake3.clone(),
            family: Some(f.font.family.clone()),
        })
        .collect()
}

/// プロジェクトにまだ無い書体の素材を足す操作。目録の BLAKE3 を付ける（キャッシュから写すときに照らしてある）。
pub fn font_asset_ops(fonts: &[CachedFont], project: &Project) -> Vec<Op> {
    let mut ops = Vec::new();
    for CachedFont { font, .. } in fonts {
        let id = font.asset_id();
        if project.assets.iter().any(|a| a.id.as_str() == id) {
            continue;
        }
        let asset = Asset {
            id: Id::new(&id),
            path: font.project_path(),
            content_hash: font.blake3.clone(),
            color_space: ColorSpace::working(),
            duration: RationalTime::ZERO,
            fps: None,
            timebase_offset: RationalTime::ZERO,
        };
        ops.push(AddAsset { asset, index: None }.into());
    }
    ops
}

/// キャッシュの書体と、パックの `fonts/<family>/` のライセンスの文を、プロジェクトのフォルダーの
/// `fonts/<family>/` へ写す。同じ大きさのファイルは写さない。写したファイルの数を返す。
pub fn copy_fonts(fonts: &[CachedFont], pack_dir: &Path, project_dir: &Path) -> Result<usize> {
    let mut copied = 0;
    let mut families: Vec<&str> = fonts.iter().map(|f| f.font.family.as_str()).collect();
    families.dedup();
    for family in families {
        let to_dir = project_dir.join(FONTS_DIR).join(family);
        std::fs::create_dir_all(&to_dir)?;
        let fonts_of = fonts.iter().filter(|f| f.font.family == family);
        let mut pairs: Vec<(PathBuf, PathBuf)> = fonts_of
            .filter(|f| is_font_path(Path::new(&f.font.file)))
            .map(|f| (f.path.clone(), to_dir.join(&f.font.file)))
            .collect();
        for from in licenses(&pack_dir.join(FONTS_DIR).join(family))? {
            let to = to_dir.join(from.file_name().unwrap_or_default());
            pairs.push((from, to));
        }
        for (from, to) in pairs {
            if copy_if_changed(&from, &to)? {
                copied += 1;
            }
        }
    }
    Ok(copied)
}

/// 大きさが違えば写す。写したら `true`。
fn copy_if_changed(from: &Path, to: &Path) -> Result<bool> {
    let len = |p: &Path| std::fs::metadata(p).ok().map(|m| m.len());
    if len(to) == len(from) {
        return Ok(false);
    }
    std::fs::copy(from, to)?;
    Ok(true)
}
