#!/usr/bin/env bash
# Runs every check CLAUDE.md requires, plus the web UI checks, and prints one
# PASS/FAIL line per step. Each step's full output goes to $GATE_LOG_DIR
# (default target/gate). Exits non-zero if any step failed.
#
#   gate.sh            all steps
#   gate.sh --no-dist  skip `cargo xtask dist` (the slowest build)
set -u

root=$(git rev-parse --show-toplevel)
cd "$root" || exit 1
logs=${GATE_LOG_DIR:-target/gate}
mkdir -p "$logs"

dist=1
[ "${1:-}" = "--no-dist" ] && dist=0

failed=0
step() {
  local name=$1
  shift
  local start=$SECONDS
  if "$@" >"$logs/$name.log" 2>&1; then
    printf 'PASS  %-10s %4ss\n' "$name" $((SECONDS - start))
  else
    printf 'FAIL  %-10s %4ss  see %s\n' "$name" $((SECONDS - start)) "$logs/$name.log"
    failed=1
  fi
}

# `topcoat fmt` only formats; it fails the gate when it changed a file.
topcoat_fmt() {
  local out
  out=$(topcoat fmt 2>&1) || { echo "$out"; return 1; }
  echo "$out"
  echo "$out" | grep -q '(0 modified)'
}

# No lint suppressions in the web crate (DESIGN.md, ADR 0005).
no_allows() {
  ! grep -rn 'allow(' crates/web/src
}

# Colours come from the tokens in styles.css, never Tailwind's palette or
# arbitrary values (.impeccable/klotho-rubric.md R1).
no_palette() {
  ! grep -rnE '(bg|text|border|ring|divide|fill|stroke|outline|from|to|via)-(gray|zinc|slate|neutral|stone|red|orange|amber|yellow|lime|green|emerald|teal|cyan|sky|blue|indigo|violet|purple|fuchsia|pink|rose|white|black)\b|\[#|\[rgb|\[oklch' crates/web/src
}

step topcoat-fmt topcoat_fmt
step fmt cargo fmt --all --check
step clippy cargo clippy --workspace --all-targets -- -D warnings
step allows no_allows
step palette no_palette
# Includes raw_memory, which pushes 1 GiB and takes about 6 minutes.
step test cargo test --workspace
step deny cargo deny check
[ $dist = 1 ] && step dist cargo xtask dist

if [ $failed = 0 ]; then echo "GATE PASS"; else echo "GATE FAIL"; fi
exit $failed
