#!/usr/bin/env bash
# PostToolUse (Edit, Write): formats the Rust file that was just changed, with
# the project's rustfmt.toml. Never fails the edit: a file that doesn't parse
# yet is left alone and `cargo fmt --check` catches it later.
input=$(cat)

path=$(printf '%s' "$input" \
  | grep -o '"file_path"[[:space:]]*:[[:space:]]*"[^"]*"' \
  | head -n 1 \
  | sed -e 's/^"file_path"[[:space:]]*:[[:space:]]*"//' -e 's/"$//' -e 's/\\\\/\//g' -e 's#//*#/#g')

case "$path" in
  *.rs) ;;
  *) exit 0 ;;
esac

root="${CLAUDE_PROJECT_DIR:-.}"
rustfmt --edition 2024 --config-path "$root/rustfmt.toml" "$path" >/dev/null 2>&1
# rustfmt leaves `view!` bodies alone; topcoat fmt formats them.
case "$path" in
  *crates/web/*) command -v topcoat >/dev/null && topcoat fmt "$path" >/dev/null 2>&1 ;;
esac
exit 0
