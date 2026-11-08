//! 書体のキャッシュ。目録（[`PackFont`]）の URL から取り、BLAKE3 を照らして、リポジトリの外に置く。
//!
//! 置き場所は `<キャッシュ>/<BLAKE3>/<ファイル名>`。中身のハッシュで分けるので、同じ書体を別のパックが指しても
//! 1 つで済み、目録の版が変わっても古いファイルと混ざらない。取れないとき（オフライン、ハッシュの違い）は
//! 書体を使わずに進み、描く側は OS の書体に置き換える。

use std::path::{Path, PathBuf};

use mm_types::{MmError, Result};

use crate::fonts::{CachedFont, PackFont};

/// キャッシュの場所を変える環境変数（テストや、共有のフォルダーを使うとき）。
pub const CACHE_ENV: &str = "MM_FONT_CACHE";
/// アプリの名前（ユーザーのキャッシュの下のフォルダー）。
const APP_DIR: &str = "movie-maker";
/// 書体のフォルダー。
const FONTS_SUBDIR: &str = "fonts";
/// 1 つのファイルを取るのにかける時間の上限（秒）。CJK の書体は数 MB あるので、遅い回線でも取りきれる長さ。
const FETCH_TIMEOUT_SECS: u32 = 300;
/// 1 つのファイルの大きさの上限。CJK の書体の数倍に取り、配布元の取り違えで巨大なファイルを落とさない。
const MAX_FONT_BYTES: u64 = 64 * 1024 * 1024;
/// 取っている途中のファイルの拡張子。照らし終えてから名前を変えるので、壊れたファイルを書体として読まない。
const PART_EXT: &str = "part";

/// URL の中身を取る口。テストは置き換えて、ネットワークなしで取得と照合を確かめる。
pub trait Fetch {
    /// `url` の中身を返す。取れなければ理由を添えた誤り。
    fn fetch(&self, url: &str) -> Result<Vec<u8>>;
}

/// `curl` で取る。Windows 10 からは OS に入っており、ほかの OS でもたいてい入っている。
/// HTTPS だけを許し、転送先も HTTPS に限る。
#[derive(Debug, Default, Clone, Copy)]
pub struct Curl;

impl Fetch for Curl {
    fn fetch(&self, url: &str) -> Result<Vec<u8>> {
        let output = mm_types::process::command("curl")
            .args(["--silent", "--show-error", "--fail", "--location"])
            .args(["--proto", "=https", "--proto-redir", "=https"])
            .args(["--max-time", &FETCH_TIMEOUT_SECS.to_string()])
            .args(["--max-filesize", &MAX_FONT_BYTES.to_string()])
            .arg(url)
            .output()
            .map_err(|e| MmError::NotFound(format!("curl を起こせない: {e}")))?;
        if !output.status.success() {
            let reason = String::from_utf8_lossy(&output.stderr);
            return Err(MmError::NotFound(format!("{url}: {}", reason.trim())));
        }
        Ok(output.stdout)
    }
}

/// 取った結果。
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct FetchReport {
    /// 新しく取ったファイルの数。
    pub fetched: usize,
    /// もともとキャッシュにあったファイルの数。
    pub cached: usize,
    /// 取れなかったファイル（ファイル名と理由）。
    pub failed: Vec<(String, String)>,
}

/// 書体のキャッシュのフォルダー。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FontCache {
    dir: PathBuf,
}

impl FontCache {
    /// `dir` をキャッシュにする。
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    /// ユーザーのキャッシュ。`MM_FONT_CACHE`、Windows は `%LOCALAPPDATA%\movie-maker\fonts`、
    /// ほかは `$XDG_CACHE_HOME/movie-maker/fonts` か `~/.cache/movie-maker/fonts`。
    pub fn user() -> Result<Self> {
        let var = |name: &str| std::env::var_os(name).filter(|v| !v.is_empty());
        if let Some(dir) = var(CACHE_ENV) {
            return Ok(Self::new(dir));
        }
        let base = var("LOCALAPPDATA")
            .or_else(|| var("XDG_CACHE_HOME"))
            .map(PathBuf::from)
            .or_else(|| var("HOME").map(|h| PathBuf::from(h).join(".cache")))
            .ok_or_else(|| {
                MmError::NotFound(format!(
                    "書体のキャッシュの場所が無い（{CACHE_ENV} で決める）"
                ))
            })?;
        Ok(Self::new(base.join(APP_DIR).join(FONTS_SUBDIR)))
    }

    /// キャッシュのフォルダー。
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// 書体の置き場所（`<キャッシュ>/<BLAKE3>/<ファイル名>`）。
    pub fn path(&self, font: &PackFont) -> PathBuf {
        self.dir.join(&font.blake3).join(&font.file)
    }

    /// キャッシュにあってハッシュの合う書体。合わないもの、無いものは除く。
    pub fn resolve(&self, fonts: &[PackFont]) -> Vec<CachedFont> {
        fonts
            .iter()
            .filter_map(|font| {
                let path = self.path(font);
                let bytes = std::fs::read(&path).ok()?;
                matches(&bytes, font).then(|| CachedFont {
                    font: font.clone(),
                    path,
                })
            })
            .collect()
    }

    /// キャッシュに無いか、ハッシュの合わない書体を取る。取れなかったものは `failed` に入れて続ける。
    /// 配布元に届かなかったら残りは取りにいかない（オフラインで 1 つずつ待たない）。
    pub fn fetch(&self, fonts: &[PackFont], fetcher: &dyn Fetch) -> FetchReport {
        let mut report = FetchReport::default();
        let have = self.resolve(fonts);
        let mut unreachable: Option<String> = None;
        for font in fonts {
            if have.iter().any(|c| c.font == *font) {
                report.cached += 1;
                continue;
            }
            if let Some(reason) = &unreachable {
                report.failed.push((font.file.clone(), reason.clone()));
                continue;
            }
            let stored = fetcher
                .fetch(&font.url)
                .inspect_err(|e| unreachable = Some(format!("配布元に届かないので取らない: {e}")))
                .and_then(|bytes| self.store(font, &bytes));
            match stored {
                Ok(()) => report.fetched += 1,
                Err(e) => report.failed.push((font.file.clone(), e.to_string())),
            }
        }
        report
    }

    /// ハッシュが合えば置く。途中のファイルに書いてから名前を変える。
    fn store(&self, font: &PackFont, bytes: &[u8]) -> Result<()> {
        if !matches(bytes, font) {
            return Err(MmError::InvalidArgument(format!(
                "{}: ハッシュが目録と違う（目録 {}、取ったもの {}）",
                font.url,
                font.blake3,
                blake3::hash(bytes).to_hex()
            )));
        }
        let path = self.path(font);
        let dir = path.parent().unwrap_or(&self.dir);
        std::fs::create_dir_all(dir)?;
        let part = path.with_extension(PART_EXT);
        std::fs::write(&part, bytes)?;
        std::fs::rename(&part, &path)?;
        Ok(())
    }
}

/// 中身が目録の BLAKE3 と合うか。大文字の 16 進でも合わせる。
fn matches(bytes: &[u8], font: &PackFont) -> bool {
    blake3::hash(bytes)
        .to_hex()
        .eq_ignore_ascii_case(&font.blake3)
}
