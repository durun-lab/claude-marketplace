//! `docs/areas.toml` の検査。変えたファイルを領域の `write` と `[[shared]]` の `path` に突き合わせる。
//! 自動の改善の回は `[improve]` の `merge`（書いて取り込んでよい所）に収まるかを見て、外に触れていたら PR までで止める。
//!
//! パスは movie-maker/ からの相対で、末尾の "/" はその配下の全部。glob は "*"（"/" をまたがない）だけ。
//! `<panel>` のような山括弧の名前は、任せるときに 1 つの名前に置き換えるので、検査では "*" と同じに扱う。

use mm_types::{MmError, Result};
use serde::{Deserialize, Serialize};

/// 1 つの領域。
#[derive(Clone, Debug, Deserialize)]
pub struct Area {
    /// 名前。
    pub name: String,
    /// 書いてよい所。
    #[serde(default)]
    pub write: Vec<String>,
}

/// どの領域も決まった形でだけ書いてよい共有のファイル。
#[derive(Clone, Debug, Deserialize)]
pub struct Shared {
    /// パス。
    pub path: String,
    /// 書き方の決まり。
    #[serde(default)]
    pub rule: String,
}

/// 自動の改善の回が書いてよい所。
#[derive(Clone, Debug, Default, Deserialize)]
pub struct ImproveAreas {
    /// 書いて取り込んでよい所。
    #[serde(default)]
    pub merge: Vec<String>,
    /// `merge` の中でも PR までで止める所（期待画像など、人が見るもの）。
    #[serde(default)]
    pub hold: Vec<String>,
}

/// `docs/areas.toml`。
#[derive(Clone, Debug, Deserialize)]
pub struct Areas {
    /// 領域。
    #[serde(default, rename = "area")]
    pub areas: Vec<Area>,
    /// 共有のファイル。
    #[serde(default)]
    pub shared: Vec<Shared>,
    /// 自動の改善の回の取り込み。
    #[serde(default)]
    pub improve: ImproveAreas,
}

/// 1 つの領域に照らした結果。
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct AreaReport {
    /// 触った共有のファイル（PR 本文の 1 行にする）。
    pub shared: Vec<String>,
    /// 領域の外のファイル。
    pub outside: Vec<String>,
}

/// 自動の改善の回の取り込みの判定。
#[derive(Clone, Debug, PartialEq, Serialize)]
pub enum ImproveVerdict {
    /// 全部 `merge` に収まる。
    Merge,
    /// 人の承認が要るファイルに触れた。
    Hold(Vec<String>),
}

impl Areas {
    /// TOML の文字列から読む。
    pub fn parse(text: &str) -> Result<Self> {
        toml::from_str(text).map_err(|e| MmError::Corrupt(format!("areas.toml: {e}")))
    }

    /// 名前で領域を引く。
    pub fn area(&self, name: &str) -> Result<&Area> {
        self.areas
            .iter()
            .find(|a| a.name == name)
            .ok_or_else(|| MmError::NotFound(format!("領域 {name}")))
    }

    /// 触った共有のファイル。
    pub fn shared_touched(&self, files: &[String]) -> Vec<String> {
        files
            .iter()
            .filter(|f| self.shared.iter().any(|s| matches(&s.path, f)))
            .cloned()
            .collect()
    }

    /// `area` の `write` にも共有のファイルにも当たらないファイルを出す。
    pub fn check_area(&self, area: &str, files: &[String]) -> Result<AreaReport> {
        let a = self.area(area)?;
        let shared = self.shared_touched(files);
        let outside = files
            .iter()
            .filter(|f| !shared.contains(f) && !a.write.iter().any(|w| matches(w, f)))
            .cloned()
            .collect();
        Ok(AreaReport { shared, outside })
    }

    /// 自動の改善の回が取り込んでよいか。`merge` の外か `hold` に当たるファイルがあれば止める。
    /// 正規化されていないパスは止める。`hold` は大文字と小文字を区別せずに当てる
    /// （Windows のファイルシステムでは `mm-text-API/` が `mm-text-api/` と同じフォルダーになる）。
    pub fn improve_verdict(&self, files: &[String]) -> ImproveVerdict {
        let held: Vec<String> = files
            .iter()
            .filter(|f| {
                let merge = self.improve.merge.iter().any(|m| matches(m, f));
                let lower = f.to_ascii_lowercase();
                let hold = self
                    .improve
                    .hold
                    .iter()
                    .any(|h| matches(&h.to_ascii_lowercase(), &lower));
                !is_plain_path(f) || !merge || hold
            })
            .cloned()
            .collect();
        if held.is_empty() {
            ImproveVerdict::Merge
        } else {
            ImproveVerdict::Hold(held)
        }
    }
}

/// "/" で区切った相対パスで、空、"."、".." の区切りと "\" と ":" を含まないか。
/// "*" は ".." にも当たるので、`ui/panels/../../x` のようなパスを `merge` に通さない。
fn is_plain_path(path: &str) -> bool {
    !path.contains(['\\', ':'])
        && path
            .split('/')
            .all(|s| !s.is_empty() && s != "." && s != "..")
}

/// `pattern` がファイル `path` に当たるか。末尾が "/" ならその配下の全部。
pub fn matches(pattern: &str, path: &str) -> bool {
    let (pat, dir) = match pattern.strip_suffix('/') {
        Some(p) => (p, true),
        None => (pattern, false),
    };
    let pat: Vec<&str> = pat.split('/').collect();
    let segs: Vec<&str> = path.split('/').collect();
    let len_ok = if dir {
        segs.len() > pat.len()
    } else {
        segs.len() == pat.len()
    };
    len_ok && pat.iter().zip(&segs).all(|(p, s)| segment_matches(p, s))
}

/// 1 つの区切りの照合。"*" は任意の文字列、`<名前>` は "*" と同じ。
fn segment_matches(pat: &str, seg: &str) -> bool {
    let pat = placeholder_to_star(pat);
    let parts: Vec<&str> = pat.split('*').collect();
    if parts.len() == 1 {
        return pat == seg;
    }
    let (first, last) = (parts[0], parts[parts.len() - 1]);
    if !seg.starts_with(first) || !seg[first.len()..].ends_with(last) {
        return false;
    }
    let mut rest = &seg[first.len()..seg.len() - last.len()];
    for mid in &parts[1..parts.len() - 1] {
        match rest.find(mid) {
            Some(i) => rest = &rest[i + mid.len()..],
            None => return false,
        }
    }
    true
}

fn placeholder_to_star(pat: &str) -> String {
    let mut out = String::new();
    let mut inside = false;
    for c in pat.chars() {
        match c {
            '<' => inside = true,
            '>' if inside => {
                inside = false;
                out.push('*');
            }
            _ if inside => {}
            _ => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{matches, Areas, ImproveVerdict};

    fn verdict(file: &str) -> ImproveVerdict {
        let areas =
            Areas::parse("[improve]\nmerge = [\"ui/panels/*/\", \"crates/mm-text-*/\"]\nhold = [\"crates/mm-*-api/\"]\n")
                .unwrap();
        areas.improve_verdict(&[file.to_owned()])
    }

    #[test]
    fn unnormalized_paths_are_held() {
        for f in [
            "ui/panels/../../Cargo.toml",
            "ui/panels/./a/b.ts",
            "ui/panels//a.ts",
            "ui\\panels\\a\\b.ts",
            "/ui/panels/a/b.ts",
        ] {
            assert_ne!(verdict(f), ImproveVerdict::Merge, "{f}");
        }
        assert_eq!(verdict("ui/panels/a/b.ts"), ImproveVerdict::Merge);
    }

    #[test]
    fn hold_ignores_ascii_case() {
        assert_ne!(
            verdict("crates/mm-text-API/src/lib.rs"),
            ImproveVerdict::Merge
        );
        assert_eq!(
            verdict("crates/mm-text-glyphon/src/lib.rs"),
            ImproveVerdict::Merge
        );
    }

    #[test]
    fn directory_pattern_covers_children_only() {
        assert!(matches("crates/mm-ops/", "crates/mm-ops/src/op.rs"));
        assert!(!matches("crates/mm-ops/", "crates/mm-ops"));
        assert!(!matches("crates/mm-ops/", "crates/mm-opsx/src/lib.rs"));
    }

    #[test]
    fn star_stays_in_one_segment() {
        assert!(matches("crates/mm-*-api/", "crates/mm-text-api/src/lib.rs"));
        assert!(matches(
            "crates/mm-*-api/",
            "crates/mm-render-api/Cargo.toml"
        ));
        assert!(!matches(
            "crates/mm-*-api/",
            "crates/mm-text-glyphon/src/lib.rs"
        ));
        assert!(matches(
            "apps/movie-maker/src/win/*_ipc.rs",
            "apps/movie-maker/src/win/pack_ipc.rs"
        ));
        assert!(!matches("ui/panels/*/", "ui/panels/README.md"));
    }

    #[test]
    fn placeholder_is_a_star() {
        assert!(matches("ui/panels/<panel>/", "ui/panels/mixer/src/a.ts"));
        assert!(matches(
            "apps/movie-maker/src/win/<panel>.rs",
            "apps/movie-maker/src/win/mixer.rs"
        ));
    }

    #[test]
    fn plain_file_matches_exactly() {
        assert!(matches("Cargo.toml", "Cargo.toml"));
        assert!(!matches("Cargo.toml", "crates/mm-ops/Cargo.toml"));
    }
}
