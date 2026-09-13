#!/bin/sh
# Run all shell tests under the given shell (default: sh). Usage: run.sh [shell]
set -u
shell=${1:-sh}
here=$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd)
status=0
for t in "$here"/*.test.sh; do
  "$shell" "$t" || status=1
done
exit $status
