#!/bin/sh
# rust-ai-lean SessionStart/SubagentStart hook.
# Usage: session-start.sh <SessionStart|SubagentStart>
# Must never break a session: every failure path exits 0 silently.

event=${1:-SessionStart}
case $event in
  SessionStart | SubagentStart) ;;
  *) exit 0 ;;
esac

plugin_root=$(CDPATH='' cd -- "$(dirname -- "$0")/.." 2>/dev/null && pwd) || exit 0
# `.` on a missing file aborts a POSIX shell, so test for it first.
[ -f "$plugin_root/scripts/lib.sh" ] || exit 0
# shellcheck source=../scripts/lib.sh
. "$plugin_root/scripts/lib.sh"

if [ "$event" = SessionStart ] && [ -n "${CLAUDE_ENV_FILE:-}" ] && [ -z "${CARGO_TERM_QUIET+set}" ]; then
  printf 'export CARGO_TERM_QUIET=true\n' >>"$CLAUDE_ENV_FILE" 2>/dev/null || :
fi

project=${CLAUDE_PROJECT_DIR:-$PWD}
ral_is_rust_project "$project" || exit 0

ra_ok=false
ral_resolve_ra >/dev/null 2>&1 && ra_ok=true
cli_ok=false
command -v rust-ai-lean >/dev/null 2>&1 && cli_ok=true

# Lines are joined with a literal backslash-n so the value is valid JSON.
# Keep every line free of double quotes and backslashes.
nl='\n'
hint='[rust-ai-lean] Rust project: keep context small.'
if $cli_ok; then
  hint="$hint${nl}Outline before reading: rust-ai-lean outline <file|dir> gives signatures + line ranges; Read only those ranges, never whole large .rs files."
fi
if $ra_ok && [ "$event" = SessionStart ]; then
  hint="$hint${nl}Semantic lookups: LSP tool (goToDefinition, findReferences, hover, incomingCalls); load it with ToolSearch query select:LSP if deferred."
fi
if $cli_ok; then
  hint="$hint${nl}Crate APIs: rust-ai-lean crate-src <crate> = exact Cargo.lock version source. Do not use Context7/web docs for Rust crate APIs."
  hint="$hint${nl}Compile errors: rust-ai-lean diag check -p <pkg>. Never --message-format=short."
fi
missing=
$ra_ok || missing='LSP unavailable'
if ! $cli_ok; then
  missing="${missing:+$missing, }CLI missing"
fi
if [ -n "$missing" ]; then
  hint="$hint${nl}Setup incomplete ($missing): ask the user to run /rust-ai-lean:doctor."
fi
hint="$hint${nl}Guide: skill rust-ai-lean:rust-lean."

printf '{"hookSpecificOutput":{"hookEventName":"%s","additionalContext":"%s"}}\n' "$event" "$hint"
exit 0
