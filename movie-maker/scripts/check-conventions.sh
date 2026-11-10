#!/bin/sh
# 並行の PR が衝突しにくくなる決まり（docs/design/14-dev-structure.md の「並行開発の境界」）を確かめる。
#   sh scripts/check-conventions.sh                 ルートの Cargo.toml の並び
#   PR_NUMBER=N BASE=origin/main sh scripts/...     加えて、PR が足した RFC のファイルの名前が N で始まるか
set -eu
cd "$(dirname "$0")/.."
status=0

# workspace.dependencies の中のクレート（mm-*）の行が 1 か所にまとまり、名前の順か。
mm_lines="$(awk '/^\[/{t=$0} t=="[workspace.dependencies]" && /^mm-/{print NR": "$1}' Cargo.toml)"
names="$(printf '%s\n' "$mm_lines" | sed 's/^[0-9]*: //')"
if [ "$names" != "$(printf '%s\n' "$names" | LC_ALL=C sort)" ]; then
  echo "Cargo.toml: [workspace.dependencies] の mm-* を名前の順に並べる（末尾に足さない）" >&2
  status=1
fi
first="$(printf '%s\n' "$mm_lines" | head -n 1 | cut -d: -f1)"
last="$(printf '%s\n' "$mm_lines" | tail -n 1 | cut -d: -f1)"
count="$(printf '%s\n' "$mm_lines" | wc -l | tr -d ' ')"
if [ $((last - first + 1)) -ne "$count" ]; then
  echo "Cargo.toml: [workspace.dependencies] の mm-* を 1 か所にまとめる" >&2
  status=1
fi

# 結合テストは 1 クレートに 1 つの実行ファイル（docs/ci.md）。2 つ目からは、冒頭に「別の実行ファイル」と理由を書いたものだけ。
for t in crates/*/tests apps/*/tests; do
  [ -d "$t" ] || continue
  bins="$(ls "$t"/*.rs "$t"/*/main.rs 2>/dev/null || true)"
  [ "$(printf '%s\n' "$bins" | grep -c .)" -le 1 ] && continue
  for f in $bins; do
    [ "$f" = "$t/it/main.rs" ] && continue
    if ! grep -q "別の実行ファイル" "$f"; then
      echo "$f: 結合テストは $t/it/ に置き、main.rs に mod を足す（docs/ci.md）" >&2
      status=1
    fi
  done
done

# 子プロセスは mm_types::process::command で起動する（Windows でコンソールのウィンドウを出さない）。
# 例外は、その関数の中身と、cargo が動かす build.rs、開発者の端末で動く xtask だけ。別名の import も拾い、コメントの行は除く。
raw_spawn="$(grep -rnE 'Command::new|Command as ' crates apps --include='*.rs' \
  | grep -v '^crates/mm-types/src/process.rs:' | grep -v '/build.rs:' \
  | grep -vE '^[^:]+:[0-9]+:[[:space:]]*//' || true)"
if [ -n "$raw_spawn" ]; then
  printf '%s\n' "$raw_spawn" >&2
  echo "子プロセスは Command::new でなく mm_types::process::command で起動する" >&2
  status=1
fi

# テストの一時フォルダーは tempfile で作る。temp_dir() にプロセスの ID だけを足した名前は、Windows が ID を使い回すため前の実行が残したフォルダーと同じ名前になる。
# rustfmt が分けた文も拾うよう、temp_dir() の行から 3 行先か文の終わり（;）までを見る。例外は、開く前に前の実行が残したフォルダーを消す host/mod.rs だけ。コメントの行は除く。
temp_pid="$(grep -rlE 'process::id\(\)' crates apps --include='*.rs' \
  | grep -v '^crates/mm-host-cef/src/host/mod\.rs$' \
  | xargs -r awk '
      FNR == 1 { from = 0 }
      /^[ \t]*\/\// { next }
      /temp_dir\(\)/ { from = FNR }
      from && FNR - from <= 3 && /process::id\(\)/ { print FILENAME ":" FNR ":" $0; from = 0 }
      /;/ { from = 0 }
    ' || true)"
if [ -n "$temp_pid" ]; then
  printf '%s\n' "$temp_pid" >&2
  echo "テストの一時フォルダーは temp_dir() とプロセスの ID でなく tempfile::tempdir()（か Builder の prefix）で作る" >&2
  status=1
fi

# RFC の番号は PR の番号。作業中は docs/rfcs/<題>.md で書き、PR を作ったら <PR の番号 4 桁>-<題>.md に改める。
if [ -n "${PR_NUMBER:-}" ] && [ -n "${BASE:-}" ]; then
  want="$(printf '%04d' "$PR_NUMBER")"
  # for の中の置換の失敗は set -e に掛からないので、先に取って失敗を検査の失敗にする。
  added="$(git diff --name-only --diff-filter=A "$BASE...HEAD" -- docs/rfcs)" || exit 1
  for f in $added; do
    case "$(basename "$f")" in
      "$want"-*.md) ;;
      *.md)
        echo "$f: RFC のファイルの名前は ${want}-<題>.md にする（番号は PR の番号）" >&2
        status=1
        ;;
    esac
  done
fi

exit $status
