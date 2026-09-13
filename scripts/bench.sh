#!/bin/sh
# shellcheck disable=SC2016 # backticks are Markdown, not command substitution
# Reproduce the README token table from this repository's own fixtures.
# Usage: scripts/bench.sh   (run from the repository root; needs cargo)
set -eu

repo=$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd)
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
export CARGO_TARGET_DIR="$work/target" CARGO_TERM_COLOR=never

cargo build --quiet --manifest-path "$repo/Cargo.toml"
bin="$CARGO_TARGET_DIR/debug/rust-ai-lean"

chars() { wc -c | tr -d ' '; }
pct() { echo "$(( $2 * 100 / $1 ))%"; }
row() { printf '| %s | %s | %s chars | %s |\n' "$1" "$2" "$3" "$4"; }

# 300 passing tests and 1 failing test, generated so the fixture stays small.
mkdir -p "$work/tests-crate/src"
printf '[package]\nname = "bench-tests"\nversion = "0.1.0"\nedition = "2021"\n\n[workspace]\n' >"$work/tests-crate/Cargo.toml"
{
  echo 'pub fn add(a: u64, b: u64) -> u64 { a + b }'
  echo '#[cfg(test)] mod tests {'
  i=0
  while [ $i -lt 300 ]; do
    echo "  #[test] fn case_$i() { assert_eq!(super::add($i, 1), $i + 1); }"
    i=$((i + 1))
  done
  echo '  #[test] fn case_failing() { assert_eq!(super::add(2, 2), 5, "deliberate failure"); }'
  echo '}'
} >"$work/tests-crate/src/lib.rs"

cd "$work/tests-crate"
cargo test --quiet --no-run 2>/dev/null
t_default=$(cargo test 2>&1 | chars || true)
t_quiet=$(CARGO_TERM_QUIET=true cargo test 2>&1 | chars || true)

cd "$repo/tests/fixtures/diag-noisy"
cargo check --quiet 2>/dev/null || true
c_human=$(cargo check 2>&1 | chars || true)
c_short=$(cargo check --message-format=short 2>&1 | chars || true)
c_diag=$("$bin" diag check 2>&1 | chars || true)

o_cat=$(cat -n "$repo/src/outline.rs" | chars)
o_outline=$("$bin" outline "$repo/src/outline.rs" | chars)

echo '| Scenario | Variant | Output | vs. baseline |'
echo '|---|---|---|---|'
row '`cargo test`, 300 pass + 1 fail' 'default' "$t_default" '100%'
row '' '`CARGO_TERM_QUIET=true`' "$t_quiet" "$(pct "$t_default" "$t_quiet")"
row '`cargo check`, 2 errors + 25 warnings' 'default' "$c_human" '100%'
row '' '`--message-format=short` (drops help/notes)' "$c_short" "$(pct "$c_human" "$c_short")"
row '' '`rust-ai-lean diag check`' "$c_diag" "$(pct "$c_human" "$c_diag")"
row 'Read `src/outline.rs`' '`cat -n`' "$o_cat" '100%'
row '' '`rust-ai-lean outline`' "$o_outline" "$(pct "$o_cat" "$o_outline")"
