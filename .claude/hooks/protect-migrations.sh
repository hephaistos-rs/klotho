#!/usr/bin/env bash
# PreToolUse (Edit, Write): refuses changes to committed migrations. sqlx
# checksums every applied migration, so editing one breaks existing databases.
# New migration files are allowed.
input=$(cat)

path=$(printf '%s' "$input" \
  | grep -o '"file_path"[[:space:]]*:[[:space:]]*"[^"]*"' \
  | head -n 1 \
  | sed -e 's/^"file_path"[[:space:]]*:[[:space:]]*"//' -e 's/"$//' -e 's/\\\\/\//g' -e 's#//*#/#g')

case "$path" in
  *crates/core/migrations/*.sql) ;;
  *) exit 0 ;;
esac

rel="crates/core/migrations/${path##*crates/core/migrations/}"
root="${CLAUDE_PROJECT_DIR:-.}"
if git -C "$root" ls-files --error-unmatch -- "$rel" >/dev/null 2>&1; then
  echo "Blocked by .claude/hooks/protect-migrations.sh: $rel is committed, and sqlx checksums applied migrations. Add a new migration instead (see the new-migration skill)." >&2
  exit 2
fi
exit 0
