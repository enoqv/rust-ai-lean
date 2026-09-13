# Test helpers. POSIX sh; sourced by tests/shell/*.test.sh.

fails=0

assert_contains() { # haystack needle label
  case $1 in
    *"$2"*) ;;
    *) printf 'FAIL %s: expected to contain [%s]\n--- got ---\n%s\n' "$3" "$2" "$1"; fails=$((fails + 1)) ;;
  esac
}

assert_not_contains() { # haystack needle label
  case $1 in
    *"$2"*) printf 'FAIL %s: expected NOT to contain [%s]\n--- got ---\n%s\n' "$3" "$2" "$1"; fails=$((fails + 1)) ;;
  esac
}

assert_eq() { # actual expected label
  [ "$1" = "$2" ] || { printf 'FAIL %s: expected [%s] got [%s]\n' "$3" "$2" "$1"; fails=$((fails + 1)); }
}

# make_stub <dir> <name> <body>: executable script on a fake PATH.
make_stub() {
  mkdir -p "$1"
  printf '#!/bin/sh\n%s\n' "$3" >"$1/$2"
  chmod +x "$1/$2"
}

finish() {
  if [ "$fails" -eq 0 ]; then echo "ok: $1"; else echo "$fails failure(s): $1"; exit 1; fi
}
