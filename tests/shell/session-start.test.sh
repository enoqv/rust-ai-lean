#!/bin/sh
# Tests for plugin/hooks/session-start.sh. Run via tests/shell/run.sh.
set -u
here=$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd)
repo=$(CDPATH='' cd -- "$here/../.." && pwd)
# shellcheck source=lib.sh
. "$here/lib.sh"
hook="$repo/plugin/hooks/session-start.sh"

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
mkdir -p "$tmp/rustproj/src" "$tmp/plain" "$tmp/multi/server"
: >"$tmp/rustproj/Cargo.toml"
: >"$tmp/multi/server/Cargo.toml"
base_path=/usr/bin:/bin

# Stub directories for the four environment states.
make_stub "$tmp/bin-ra" rustup 'echo /opt/stub/rust-analyzer'
make_stub "$tmp/bin-cli" rust-ai-lean 'echo rust-ai-lean 0.1.0'
make_stub "$tmp/bin-nora" rustup 'exit 1'

run_hook() { # event project path [extra env assignments via env]
  env -i PATH="$3" HOME="$tmp" CLAUDE_PROJECT_DIR="$2" CLAUDE_ENV_FILE="$tmp/envfile" \
    sh "$hook" "$1" </dev/null
}

# 1. Non-Rust project: no output, but env var still written.
: >"$tmp/envfile"
out=$(run_hook SessionStart "$tmp/plain" "$tmp/bin-ra:$tmp/bin-cli:$base_path")
assert_eq "$out" "" "non-rust project prints nothing"
assert_eq "$(cat "$tmp/envfile")" "export CARGO_TERM_QUIET=true" "env file written for any project"

# 2. Everything available.
out=$(run_hook SessionStart "$tmp/rustproj" "$tmp/bin-ra:$tmp/bin-cli:$base_path")
assert_contains "$out" '{"hookSpecificOutput":{"hookEventName":"SessionStart","additionalContext":"[rust-ai-lean] Rust project' "full: json prefix"
assert_contains "$out" 'rust-ai-lean outline' "full: outline line"
assert_contains "$out" 'LSP tool' "full: LSP line"
assert_contains "$out" 'crate-src' "full: crate-src line"
assert_not_contains "$out" 'Setup incomplete' "full: no setup warning"

# 3. CLI missing, LSP ok.
out=$(run_hook SessionStart "$tmp/rustproj" "$tmp/bin-ra:$base_path")
assert_not_contains "$out" 'rust-ai-lean outline' "no-cli: no outline line"
assert_contains "$out" 'LSP tool' "no-cli: LSP line"
assert_contains "$out" 'Setup incomplete (CLI missing)' "no-cli: setup warning"

# 4. LSP unavailable, CLI ok.
out=$(run_hook SessionStart "$tmp/rustproj" "$tmp/bin-nora:$tmp/bin-cli:$base_path")
assert_not_contains "$out" 'LSP tool' "no-ra: no LSP line"
assert_contains "$out" 'Setup incomplete (LSP unavailable)' "no-ra: setup warning"

# 5. Nothing available.
out=$(run_hook SessionStart "$tmp/rustproj" "$tmp/bin-nora:$base_path")
assert_contains "$out" 'Setup incomplete (LSP unavailable, CLI missing)' "none: both missing"

# 6. SubagentStart: no LSP line, no env write.
: >"$tmp/envfile"
out=$(run_hook SubagentStart "$tmp/rustproj" "$tmp/bin-ra:$tmp/bin-cli:$base_path")
assert_contains "$out" '"hookEventName":"SubagentStart"' "subagent: event name"
assert_not_contains "$out" 'LSP tool' "subagent: no LSP line"
assert_eq "$(cat "$tmp/envfile")" "" "subagent: env file untouched"

# 7. Multi-repo workspace root detected via child Cargo.toml.
out=$(run_hook SessionStart "$tmp/multi" "$tmp/bin-ra:$tmp/bin-cli:$base_path")
assert_contains "$out" '[rust-ai-lean]' "multi-repo root detected"

# 8. Explicit CARGO_TERM_QUIET is respected.
: >"$tmp/envfile"
env -i PATH="$base_path" HOME="$tmp" CLAUDE_PROJECT_DIR="$tmp/plain" CLAUDE_ENV_FILE="$tmp/envfile" \
  CARGO_TERM_QUIET=false sh "$hook" SessionStart </dev/null >/dev/null
assert_eq "$(cat "$tmp/envfile")" "" "user CARGO_TERM_QUIET respected"

# 9. Size cap: additionalContext value <= 700 bytes in the largest state.
out=$(run_hook SessionStart "$tmp/rustproj" "$tmp/bin-ra:$tmp/bin-cli:$base_path")
value=${out#*'"additionalContext":"'}
value=${value%'"}}'}
size=$(printf '%s' "$value" | wc -c | tr -d ' ')
[ "$size" -le 700 ] || { echo "FAIL size cap: $size bytes"; fails=$((fails + 1)); }
worst=$(run_hook SessionStart "$tmp/rustproj" "$tmp/bin-nora:$base_path")
value=${worst#*'"additionalContext":"'}
value=${value%'"}}'}
size=$(printf '%s' "$value" | wc -c | tr -d ' ')
[ "$size" -le 700 ] || { echo "FAIL size cap (none): $size bytes"; fails=$((fails + 1)); }

# 10. Unknown event exits 0 with no output.
out=$(run_hook Bogus "$tmp/rustproj" "$tmp/bin-ra:$base_path")
assert_eq "$out" "" "unknown event silent"

finish session-start
