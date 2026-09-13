# Shared helpers for rust-ai-lean shell scripts. POSIX sh; source, do not execute.

# Print the rust-analyzer binary the LSP launcher would run; return 1 if none.
# Keep in sync with the launcher in plugin/.claude-plugin/plugin.json.
ral_resolve_ra() {
  if command -v rustup >/dev/null 2>&1; then
    rustup which --toolchain stable rust-analyzer 2>/dev/null
  else
    command -v rust-analyzer 2>/dev/null
  fi
}

# Succeed if $1 is inside a Cargo project (Cargo.toml in it or an ancestor up
# to the git root) or has a Cargo.toml within two levels below it.
ral_is_rust_project() {
  ral_dir=$1
  ral_d=$ral_dir
  while :; do
    [ -f "$ral_d/Cargo.toml" ] && return 0
    [ -e "$ral_d/.git" ] && break
    ral_parent=$(dirname -- "$ral_d")
    [ "$ral_parent" = "$ral_d" ] && break
    ral_d=$ral_parent
  done
  for ral_f in "$ral_dir"/*/Cargo.toml "$ral_dir"/*/*/Cargo.toml; do
    [ -f "$ral_f" ] || continue
    case $ral_f in
      */target/*) continue ;;
    esac
    return 0
  done
  return 1
}
