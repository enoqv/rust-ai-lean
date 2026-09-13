#!/bin/sh
# Tests for plugin/skills/doctor/scripts/doctor.sh. Run via tests/shell/run.sh.
set -u
here=$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd)
repo=$(CDPATH='' cd -- "$here/../.." && pwd)
# shellcheck source=lib.sh
. "$here/lib.sh"
doctor="$repo/plugin/skills/doctor/scripts/doctor.sh"

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
mkdir -p "$tmp/plain"
base_path=/usr/bin:/bin
version=$(sed -n 's/^[[:space:]]*"version"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' "$repo/plugin/.claude-plugin/plugin.json" | head -n 1)

make_stub "$tmp/ra-ok" rust-analyzer 'echo "rust-analyzer 1.0.0-stub"'
make_stub "$tmp/bin-ok" rustup "case \"\$1 \$2\" in 'which --toolchain') echo $tmp/ra-ok/rust-analyzer ;; *) exit 1 ;; esac"
make_stub "$tmp/bin-ok" rust-ai-lean "echo rust-ai-lean $version"
make_stub "$tmp/bin-bad" rustup 'exit 1'
make_stub "$tmp/bin-bad" rust-ai-lean 'echo rust-ai-lean 0.0.1'

run_doctor() { # path project [VAR=value...]
  p=$1; proj=$2; shift 2
  env -i PATH="$p" HOME="$tmp" "$@" sh "$doctor" --plugin-root "$repo/plugin" --project "$proj" </dev/null
}

status_of() { # output id
  printf '%s\n' "$1" | awk -F '\t' -v id="$2" '$2 == id { print $1 }'
}

fix_of() { # output id
  printf '%s\n' "$1" | awk -F '\t' -v id="$2" '$2 == id { print $4 }'
}

# Healthy toolchain, non-Rust directory.
out=$(run_doctor "$tmp/bin-ok:$base_path" "$tmp/plain" CARGO_TERM_QUIET=true)
for id in toolchain.rustup toolchain.ra project.detected toolchain.rust-src cli.installed cli.version env.quiet project.lock project.crate-src; do
  count=$(printf '%s\n' "$out" | awk -F '\t' -v id="$id" '$2 == id' | wc -l | tr -d ' ')
  assert_eq "$count" 1 "healthy: $id reported once"
done
assert_eq "$(status_of "$out" toolchain.ra)" OK "healthy: ra ok"
assert_eq "$(status_of "$out" cli.version)" OK "healthy: version ok"
assert_eq "$(status_of "$out" env.quiet)" OK "healthy: env ok"
assert_eq "$(status_of "$out" project.detected)" INFO "healthy: plain dir is not rust"

# Broken toolchain: component missing, CLI version mismatch, env unset.
out=$(run_doctor "$tmp/bin-bad:$base_path" "$tmp/plain")
assert_eq "$(status_of "$out" toolchain.ra)" FAIL "broken: ra fail"
assert_eq "$(fix_of "$out" toolchain.ra)" "rustup component add rust-analyzer --toolchain stable" "broken: ra fix"
assert_eq "$(status_of "$out" cli.version)" WARN "broken: version mismatch"
assert_eq "$(fix_of "$out" cli.version)" "cargo install --git https://github.com/enoqv/rust-ai-lean --tag v$version --locked --force" "broken: version fix"
assert_eq "$(status_of "$out" env.quiet)" WARN "broken: env unset"

# No rustup, no CLI.
out=$(run_doctor "$base_path" "$tmp/plain" CARGO_TERM_QUIET=false)
assert_eq "$(status_of "$out" toolchain.rustup)" WARN "bare: rustup warn"
assert_eq "$(status_of "$out" cli.installed)" WARN "bare: cli missing"
assert_eq "$(fix_of "$out" cli.installed)" "cargo install --git https://github.com/enoqv/rust-ai-lean --tag v$version --locked" "bare: install fix"
assert_eq "$(status_of "$out" env.quiet)" INFO "bare: user value respected"

# Missing plugin root never crashes.
out=$(env -i PATH="$base_path" sh "$doctor" --plugin-root "$tmp/nowhere" --project "$tmp/plain" </dev/null)
assert_eq "$(status_of "$out" plugin.lib)" FAIL "missing lib reported"

finish doctor
