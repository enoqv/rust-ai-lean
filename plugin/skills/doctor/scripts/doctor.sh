#!/bin/sh
# rust-ai-lean doctor: static environment checks.
# Usage: doctor.sh [--plugin-root DIR] [--project DIR]
# Output: one line per check, STATUS<TAB>id<TAB>message<TAB>fix. Always exits 0.

REPO_URL=https://github.com/enoqv/rust-ai-lean
plugin_root=
project=$PWD
while [ $# -gt 0 ]; do
  case $1 in
    --plugin-root) plugin_root=${2:-}; shift 2 || shift ;;
    --project) project=${2:-}; shift 2 || shift ;;
    *) shift ;;
  esac
done
[ -n "$plugin_root" ] || plugin_root=$(CDPATH='' cd -- "$(dirname -- "$0")/../../.." && pwd)

report() { printf '%s\t%s\t%s\t%s\n' "$1" "$2" "$3" "${4:-}"; }

# `.` on a missing file aborts a POSIX shell, so test for it first.
if [ ! -f "$plugin_root/scripts/lib.sh" ]; then
  report FAIL plugin.lib "cannot load $plugin_root/scripts/lib.sh" ""
  exit 0
fi
# shellcheck source=../../../scripts/lib.sh
. "$plugin_root/scripts/lib.sh"

plugin_version=$(sed -n 's/^[[:space:]]*"version"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' \
  "$plugin_root/.claude-plugin/plugin.json" 2>/dev/null | head -n 1)
install_cmd="cargo install --git $REPO_URL --tag v$plugin_version --locked"

# Never let rustup download a toolchain as a side effect of a check.
RUSTUP_AUTO_INSTALL=0
export RUSTUP_AUTO_INSTALL

check_rustup() {
  if command -v rustup >/dev/null 2>&1; then
    report OK toolchain.rustup "rustup found"
    has_rustup=true
  else
    report WARN toolchain.rustup "rustup not found; the LSP launcher will use rust-analyzer from PATH"
    has_rustup=false
  fi
}

check_ra() {
  ra=$(ral_resolve_ra) || ra=
  if [ -n "$ra" ] && ra_version=$("$ra" --version 2>/dev/null); then
    report OK toolchain.ra "$ra_version ($ra)"
  elif $has_rustup; then
    report FAIL toolchain.ra "rust-analyzer is not installed for the stable toolchain" \
      "rustup component add rust-analyzer --toolchain stable"
  else
    report FAIL toolchain.ra "rust-analyzer not found on PATH; see https://rust-analyzer.github.io/book/installation.html"
  fi
}

check_project_detected() {
  if ral_is_rust_project "$project"; then
    report OK project.detected "Rust project: $project"
    is_rust=true
  else
    report INFO project.detected "not a Rust project: $project"
    is_rust=false
  fi
}

check_rust_src() {
  if ! $is_rust || ! $has_rustup; then
    report INFO toolchain.rust-src "skipped"
    return
  fi
  toolchain=$(cd "$project" 2>/dev/null && rustup show active-toolchain 2>/dev/null | awk 'NR == 1 { print $1 }')
  sysroot=$(cd "$project" 2>/dev/null && rustc --print sysroot 2>/dev/null)
  if [ -z "$toolchain" ] || [ -z "$sysroot" ]; then
    report WARN toolchain.rust-src "check failed: cannot determine the project toolchain (is it installed?)"
  elif [ -d "$sysroot/lib/rustlib/src/rust/library" ]; then
    report OK toolchain.rust-src "rust-src present for $toolchain"
  else
    report WARN toolchain.rust-src "rust-src missing for $toolchain; std types resolve poorly in rust-analyzer" \
      "rustup component add rust-src --toolchain $toolchain"
  fi
}

check_cli() {
  if ! command -v rust-ai-lean >/dev/null 2>&1; then
    report WARN cli.installed "rust-ai-lean CLI not on PATH" "$install_cmd"
    report INFO cli.version "skipped"
    cli_ok=false
    return
  fi
  cli_ok=true
  report OK cli.installed "$(command -v rust-ai-lean)"
  cli_version=$(rust-ai-lean --version 2>/dev/null | awk '{ print $2 }')
  if [ "$cli_version" = "$plugin_version" ]; then
    report OK cli.version "$cli_version"
  else
    report WARN cli.version "CLI $cli_version does not match plugin $plugin_version" "$install_cmd --force"
  fi
}

check_env() {
  if [ "${CARGO_TERM_QUIET:-}" = true ]; then
    report OK env.quiet "CARGO_TERM_QUIET=true"
  elif [ -n "${CARGO_TERM_QUIET+set}" ]; then
    report INFO env.quiet "CARGO_TERM_QUIET=$CARGO_TERM_QUIET set by the user; respected"
  else
    report WARN env.quiet "CARGO_TERM_QUIET not set; this session started before the plugin hook ran" \
      "start a new Claude Code session"
  fi
}

check_lock_and_crate_src() {
  if ! $is_rust; then
    report INFO project.lock "skipped"
    report INFO project.crate-src "skipped"
    return
  fi
  manifest=$(cd "$project" 2>/dev/null && cargo locate-project --workspace --message-format plain 2>/dev/null)
  if [ -z "$manifest" ]; then
    report INFO project.lock "skipped: $project is not inside a Cargo package; run doctor from a crate directory"
    report INFO project.crate-src "skipped"
    return
  fi
  ws_root=$(dirname -- "$manifest")
  if [ -f "$ws_root/Cargo.lock" ]; then
    report OK project.lock "$ws_root/Cargo.lock"
  else
    report WARN project.lock "no Cargo.lock in $ws_root; crate-src needs one (never generated automatically)"
    report INFO project.crate-src "skipped"
    return
  fi
  if ! $cli_ok; then
    report INFO project.crate-src "skipped: CLI not installed"
    return
  fi
  dep=$(awk '
    /^\[(workspace\.)?dependencies\]/ { in_deps = 1; next }
    /^\[/ { in_deps = 0 }
    in_deps && /^[A-Za-z0-9_-]+[[:space:]]*=/ { sub(/[[:space:]]*=.*/, ""); print; exit }
  ' "$manifest")
  if [ -z "$dep" ]; then
    report INFO project.crate-src "skipped: no direct dependencies in $manifest"
    return
  fi
  if resolved=$(cd "$project" && rust-ai-lean crate-src "$dep" --path-only 2>&1); then
    report OK project.crate-src "$dep -> $(printf '%s\n' "$resolved" | head -n 1)"
  else
    report WARN project.crate-src "crate-src $dep failed: $(printf '%s\n' "$resolved" | head -n 1)"
  fi
}

has_rustup=false
is_rust=false
cli_ok=false
check_rustup
check_ra
check_project_detected
check_rust_src
check_cli
check_env
check_lock_and_crate_src
exit 0
