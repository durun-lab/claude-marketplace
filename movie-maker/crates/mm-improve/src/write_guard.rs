//! 回の `claude -p` が worktree の外へ書くのを拒否する規則（Claude Code の permissions）。
//!
//! - `blockWritesOutsideWorkingDirectories`: Write、Edit、NotebookEdit が作業フォルダーの外を書くのを、どのモードでも拒否する。
//! - 作業フォルダーは回の worktree だけ。`--add-dir` を渡さず、`--setting-sources project` で人の設定
//!   （`additionalDirectories` のほかのリポジトリや、`Bash(bash *)` のような広い allow）を読ませない。
//! - `Edit(...)` の deny: Bash のリダイレクト、`tee`、Claude Code が見分けるファイルのコマンドが、要望のファイル、
//!   置き場所の状態、人のチェックアウトへ書くのを拒否する。
//!
//! Bash から起こした任意のプログラムが書くのは止められない（ネイティブの Windows では Claude Code のサンドボックスが動かない）。

use std::path::Path;

/// 規則のパスで、そのまま書くと glob になる文字。
const GLOB_CHARS: [char; 4] = ['*', '?', '[', ']'];

/// 回の `claude -p` に渡す、読むだけの所と書かせない所。値は permissions の規則のパス（`//c/...`）。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WriteGuard {
    /// 作業フォルダーの外で、読むだけを許す所（要望のファイル）。`Read(...)` の allow にする。
    pub read_only: Vec<String>,
    /// 書かせない所。`Edit(...)` の deny にする。
    pub no_write: Vec<String>,
}

impl WriteGuard {
    /// 回の置き場所から作る。`worktree` を含むフォルダーは deny に入れない（回が自分の作業フォルダーを書けなくなる）。
    pub fn for_run(data_dir: &Path, feedback_dir: &Path, repo: &Path, worktree: &Path) -> Self {
        let feedback = format!("{}/**", rule_path(feedback_dir));
        let data = rule_path(data_dir);
        let mut no_write = vec![
            feedback.clone(),
            format!("{data}/*.json"),
            format!("{data}/{}", crate::round::LOCK_FILE),
            format!("{data}/{}", crate::round::STOP_MARK),
            format!("{data}/runs/**"),
        ];
        if !worktree.starts_with(repo) {
            no_write.push(format!("{}/**", rule_path(repo)));
        }
        Self {
            read_only: vec![feedback],
            no_write,
        }
    }

    /// `permissions` に足す allow と deny。
    pub fn rules(&self) -> (Vec<String>, Vec<String>) {
        let allow = self
            .read_only
            .iter()
            .map(|p| format!("Read({p})"))
            .collect();
        let deny = self.no_write.iter().map(|p| format!("Edit({p})")).collect();
        (allow, deny)
    }
}

/// 絶対パスを permissions の規則のパスにする。Claude Code は Windows のパスを POSIX の形
/// （`C:\Users\a` は `/c/Users/a`）にしてから照合し、規則の `//` は根からの絶対パスを表す。glob の文字は逃がす。
pub fn rule_path(path: &Path) -> String {
    let s = path.to_string_lossy().replace('\\', "/");
    let s = s.strip_prefix("//?/").unwrap_or(&s);
    let mut chars = s.chars();
    let posix = match (chars.next(), chars.next()) {
        (Some(d), Some(':')) if d.is_ascii_alphabetic() => {
            format!("/{}{}", d.to_ascii_lowercase(), chars.as_str())
        }
        _ => s.to_owned(),
    };
    let mut out = String::from("/");
    for c in posix.trim_end_matches('/').chars() {
        if GLOB_CHARS.contains(&c) {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_paths_become_double_slash_posix() {
        assert_eq!(
            rule_path(Path::new(r"C:\Users\a\AppData\Local\mm")),
            "//c/Users/a/AppData/Local/mm"
        );
        assert_eq!(rule_path(Path::new(r"\\?\E:\repos\x\")), "//e/repos/x");
        assert_eq!(rule_path(Path::new("/home/a/[x]")), r"//home/a/\[x\]");
    }

    #[test]
    fn repo_is_denied_only_when_the_worktree_is_outside_it() {
        let g = WriteGuard::for_run(
            Path::new("/d"),
            Path::new("/d/feedback"),
            Path::new("/r"),
            Path::new("/d/worktrees/1"),
        );
        assert!(g.no_write.contains(&"//r/**".to_owned()), "{g:?}");
        assert!(g.no_write.contains(&"//d/feedback/**".to_owned()));
        assert!(!g.no_write.iter().any(|p| p.contains("worktrees")));
        let inside = WriteGuard::for_run(
            Path::new("/d"),
            Path::new("/d/feedback"),
            Path::new("/r"),
            Path::new("/r/.wt/1"),
        );
        assert!(!inside.no_write.contains(&"//r/**".to_owned()));
    }
}
