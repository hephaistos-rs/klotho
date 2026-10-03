#!/usr/bin/env bash
# PreToolUse (Write): refuses new Markdown files outside the allowlist in
# .gitignore, so agents don't leave untracked notes around. Existing files can
# still be rewritten, and .impeccable/ is run scratch, so it is allowed.
input=$(cat)

path=$(printf '%s' "$input" \
  | grep -o '"file_path"[[:space:]]*:[[:space:]]*"[^"]*"' \
  | head -n 1 \
  | sed -e 's/^"file_path"[[:space:]]*:[[:space:]]*"//' -e 's/"$//' \
  | tr '\134' / | tr -s /)

case "$path" in
  *.md | *.MD | *.markdown) ;;
  *) exit 0 ;;
esac
[ -e "$path" ] && exit 0

root="${CLAUDE_PROJECT_DIR:-.}"
root=$(printf '%s' "$root" | tr '\134' / | tr -s /)
# Compare case-insensitively: Windows drive letters and folders vary in case.
lpath=$(printf '%s' "$path" | tr '[:upper:]' '[:lower:]')
lroot=$(printf '%s' "$root" | tr '[:upper:]' '[:lower:]')
case "$lpath" in
  "$lroot"/*) rel="${path:${#root}+1}" ;;
  *) exit 0 ;; # outside the project: not ours to police
esac

case "$rel" in
  README* | CHANGELOG* | CONTRIBUTING* | LICENSE* | SECURITY* | */README* | */CHANGELOG* | */LICENSE*) exit 0 ;;
  AGENTS.md | CLAUDE.md | ROADMAP.md | DESIGN.md | PRODUCT.md) exit 0 ;;
  docs/*) exit 0 ;;
  .impeccable/*) exit 0 ;;
  .claude/skills/*/SKILL.md | .claude/agents/*.md) exit 0 ;;
esac

echo "Blocked by .claude/hooks/protect-markdown.sh: $rel is not on the Markdown allowlist (CLAUDE.md, .gitignore). Put notes in an allowed file, in docs/, or in the scratchpad instead." >&2
exit 2
