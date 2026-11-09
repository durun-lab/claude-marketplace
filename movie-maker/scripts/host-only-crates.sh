#!/bin/sh
# Cargo.toml の [profile.dev.package] で opt-level 0 に戻す名前の一覧を、cargo metadata から求めて出す。
# 実行時の木（メンバーの normal と dev の依存を、proc-macro の手前までたどる）に入らず、
# proc-macro か build script からだけ届くクレートを出す。依存を足したらこれと Cargo.toml の一覧を見比べる。
# 大きな入力を読む build script の道具（gl_generator、xml-rs など）は 0 にすると遅くなるので、出ても挙げない。
#   引数に cargo metadata のオプションを渡せる（bindgen の側は --features mm-decode-ffmpeg/ffmpeg で出る）
set -eu
cd "$(dirname "$0")/.."
cargo metadata --format-version 1 --filter-platform x86_64-pc-windows-msvc "$@" | jq -r '
  (.packages | map({key: .id, value: .}) | from_entries) as $pkgs
  | (.resolve.nodes | map({key: .id, value: .}) | from_entries) as $nodes
  | def is_pm($id): $pkgs[$id].targets | any(.kind | index("proc-macro"));
    def deps($id; $kinds):
      [$nodes[$id].deps[] | select(any(.dep_kinds[]; .kind as $k | $kinds | index($k))) | .pkg];
    def reach(f):
      def go($seen; $front):
        if ($front | length) == 0 then $seen
        else ([$front[] | f] | add // [] | unique - $seen) as $new | go($seen + $new; $new) end;
      go(.; .);
    .workspace_members as $m
  | [$m[] | deps(.; [null, "dev"])] | add | unique - $m
  | map(select(is_pm(.) | not))
  | reach(deps(.; [null]) | map(select(is_pm(.) | not))) as $rt
  | ($nodes | keys) - $m - $rt
  | [.[] | $pkgs[.].name] | unique - [$rt[] | $pkgs[.].name]
  | .[]
' | tr -d '\r'
