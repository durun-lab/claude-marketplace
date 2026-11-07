//! パックの書体（`fonts/<family>/<ファイル>`）。
//!
//! 適用するときにプロジェクトのフォルダーの `fonts/` へ写し、素材（`addAsset`）として足す。描く側は書体の素材を
//! 読み込むので、書体が入っていない PC でも、プロジェクトを別の PC に移しても同じ書体で描ける。
//! フォルダーの名前はテーマと CSS が書く family の名前で、描く側も同じ名前で登録する（`mm_text_api::FontFile`）。

use std::path::{Path, PathBuf};

use mm_doc::{Asset, Project};
use mm_ops::{AddAsset, Op};
use mm_text_api::{is_font_path, FontFile, FONTS_DIR};
use mm_types::{ColorSpace, Id, MmError, RationalTime, Result};

/// 書体の素材の ID の接頭辞。後ろはファイル名（拡張子を除く）を英小文字と数字と `-` にしたもの。
const ASSET_PREFIX: &str = "font-";
/// ライセンスの文のファイル名の頭（`OFL.txt`、`LICENSE.txt` など）。書体と一緒に写す。
const LICENSE_HEADS: [&str; 2] = ["ofl", "license"];

/// パックの書体のファイル 1 つ。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackFont {
    /// family の名前（`fonts/` の直下のフォルダーの名前）。
    pub family: String,
    /// ファイルの場所。
    pub path: PathBuf,
}

impl PackFont {
    /// ファイル名。
    pub fn file_name(&self) -> String {
        self.path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default()
    }

    /// プロジェクトのフォルダーからの場所（`fonts/<family>/<ファイル>`、`/` 区切り）。
    pub fn project_path(&self) -> String {
        format!("{FONTS_DIR}/{}/{}", self.family, self.file_name())
    }

    /// 素材の ID。
    pub fn asset_id(&self) -> String {
        let stem = self
            .path
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

/// `fonts/` の下の書体を family の名前、ファイル名の順に読む。フォルダーが無ければ空。
pub(crate) fn read_fonts(dir: &Path) -> Result<Vec<PackFont>> {
    let mut fonts = Vec::new();
    for family_dir in sorted_entries(dir)?.into_iter().filter(|p| p.is_dir()) {
        let family = family_dir
            .file_name()
            .map(|n| n.to_string_lossy().into_owned());
        let Some(family) = family else { continue };
        for path in sorted_entries(&family_dir)? {
            if path.is_file() && is_font_path(&path) {
                fonts.push(PackFont {
                    family: family.clone(),
                    path,
                });
            }
        }
    }
    Ok(fonts)
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

/// family のフォルダーにライセンスの文があるか。
pub(crate) fn has_license(family_dir: &Path) -> bool {
    sorted_entries(family_dir).is_ok_and(|e| e.iter().any(|p| p.is_file() && is_license(p)))
}

/// パックの書体を、文字を組む実装に渡す形にする（プレートの大きさを描くときと同じ書体で測るため）。
pub fn font_files(fonts: &[PackFont]) -> Vec<FontFile> {
    fonts
        .iter()
        .map(|f| FontFile {
            path: f.path.clone(),
            content_hash: String::new(),
            family: Some(f.family.clone()),
        })
        .collect()
}

/// プロジェクトにまだ無い書体の素材を足す操作。中身の BLAKE3 を付ける（描く側の同じ組の判定とキャッシュのキー）。
pub fn font_asset_ops(fonts: &[PackFont], project: &Project) -> Result<Vec<Op>> {
    let mut ops = Vec::new();
    for font in fonts {
        let id = font.asset_id();
        if project.assets.iter().any(|a| a.id.as_str() == id) {
            continue;
        }
        let bytes = std::fs::read(&font.path)
            .map_err(|e| MmError::NotFound(format!("{}: {e}", font.path.display())))?;
        let asset = Asset {
            id: Id::new(&id),
            path: font.project_path(),
            content_hash: blake3::hash(&bytes).to_hex().to_string(),
            color_space: ColorSpace::working(),
            duration: RationalTime::ZERO,
            fps: None,
            timebase_offset: RationalTime::ZERO,
        };
        ops.push(AddAsset { asset, index: None }.into());
    }
    Ok(ops)
}

/// 書体とライセンスの文を、プロジェクトのフォルダーの `fonts/<family>/` へ写す。同じ大きさのファイルは写さない。
/// 写したファイルの数を返す。
pub fn copy_fonts(fonts: &[PackFont], project_dir: &Path) -> Result<usize> {
    let mut copied = 0;
    let mut families: Vec<&Path> = fonts.iter().filter_map(|f| f.path.parent()).collect();
    families.dedup();
    for family_dir in families {
        let name = family_dir.file_name().unwrap_or_default();
        let to_dir = project_dir.join(FONTS_DIR).join(name);
        std::fs::create_dir_all(&to_dir)?;
        let files = sorted_entries(family_dir)?;
        for from in files.iter().filter(|p| is_font_path(p) || is_license(p)) {
            let to = to_dir.join(from.file_name().unwrap_or_default());
            let same = std::fs::metadata(&to).ok().map(|m| m.len())
                == std::fs::metadata(from).ok().map(|m| m.len());
            if !same {
                std::fs::copy(from, &to)?;
                copied += 1;
            }
        }
    }
    Ok(copied)
}
