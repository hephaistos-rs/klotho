#!/usr/bin/env bash
# PreToolUse (Bash, PowerShell): refuses commands the project rules forbid.
# Exit code 2 blocks the call and shows stderr to Claude.
input=$(cat)

block() {
  echo "Blocked by .claude/hooks/guard-commands.sh: $1" >&2
  exit 2
}

if printf '%s' "$input" | grep -Eiq '(^|[^a-z0-9_-])(python[0-9.]*|py)(\.exe)?([[:space:]]|$|")'; then
  block "Python is not used in this project. Use the built-in tools, shell or Rust (CLAUDE.md)."
fi
# Global options before the subcommand, including those that take a value (-C dir, -c key=value).
opts='([[:space:]]+(-[Cc][[:space:]]+[^[:space:]]+|-[^[:space:]]+))*'
if printf '%s' "$input" | grep -Eq "git${opts}[[:space:]]+push([[:space:]]|\$|\")"; then
  block "never push. Commit locally; the maintainer pushes (CLAUDE.md)."
fi
if printf '%s' "$input" | grep -Eq "git${opts}[[:space:]]+remote[[:space:]]+(add|set-url|remove|rm|rename)"; then
  block "never change remotes (CLAUDE.md)."
fi
if printf '%s' "$input" | grep -Eq -- '--no-verify|--no-gpg-sign|commit\.gpgsign=false'; then
  block "don't skip hooks or signing. Fix what the hook complains about instead."
fi
exit 0
