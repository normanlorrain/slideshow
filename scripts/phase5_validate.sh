#!/usr/bin/env bash
# Phase 5 validation harness — run from repository root.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

BIN="${BIN:-target/debug/magic-lantern}"
PASS=0
FAIL=0

green() { printf '\033[32m%s\033[0m\n' "$*"; }
red() { printf '\033[31m%s\033[0m\n' "$*"; }

check() {
  local name="$1"
  shift
  printf '— %s … ' "$name"
  if "$@" >/tmp/ml-phase5-out.txt 2>/tmp/ml-phase5-err.txt; then
    green "PASS"
    PASS=$((PASS + 1))
  else
    red "FAIL"
    echo "  stdout:" && sed 's/^/    /' /tmp/ml-phase5-out.txt | tail -20
    echo "  stderr:" && sed 's/^/    /' /tmp/ml-phase5-err.txt | tail -20
    FAIL=$((FAIL + 1))
  fi
}

echo "=== Phase 5 validation ==="
echo "Root: $ROOT"

echo ""
echo "### Unit + integration tests"
check "cargo test" cargo test --quiet

echo ""
echo "### Build debug binary"
check "cargo build" cargo build --quiet
BIN="$ROOT/target/debug/magic-lantern"

echo ""
echo "### Dry-run (validation.toml golden prefixes)"
OUT="$("$BIN" -c tests/validation.toml --dry-run 12 2>/dev/null | grep -E '^[0-9]+ ')"
echo "$OUT" | head -6
echo "$OUT" | head -1 | grep -q 'numbers/1_' && green "PASS dry-run starts with numbers/1_" && PASS=$((PASS+1)) || {
  red "FAIL dry-run first line"
  FAIL=$((FAIL+1))
}
echo "$OUT" | sed -n '5p' | grep -q 'bitmaps/Mars' && green "PASS dry-run slide 4 is Mars.bmp" && PASS=$((PASS+1)) || {
  red "FAIL dry-run bitmaps slot"
  FAIL=$((FAIL+1))
}

echo ""
echo "### Kiosk config dry-run (includes PDF if pdftoppm present)"
check "kiosk dry-run" "$BIN" -c tests/kiosk.toml --dry-run 8

echo ""
echo "### Kiosk UI smoke (auto-quit 3s)"
check "kiosk UI smoke" env MAGIC_LANTERN_AUTO_QUIT_SECS=3 RUST_LOG=info \
  "$BIN" -c tests/kiosk.toml --interval 1

echo ""
echo "### SIGUSR1 reload during UI run"
# Use paintings-only (no PDF) so startup is fast; wait until first slide logged.
LOG=/tmp/ml-phase5-reload.log
rm -f "$LOG"
MAGIC_LANTERN_AUTO_QUIT_SECS=8 RUST_LOG=info \
  "$BIN" --interval 2 tests/images/paintings >"$LOG" 2>&1 &
PID=$!
# Wait until the event loop is up (first image log or timeout).
for _ in $(seq 1 40); do
  if grep -qE 'Slide count:|davinci|monet|rembrandt|Screen size' "$LOG" 2>/dev/null; then
    break
  fi
  kill -0 "$PID" 2>/dev/null || break
  sleep 0.25
done
sleep 1
if kill -0 "$PID" 2>/dev/null; then
  kill -USR1 "$PID" || true
  wait "$PID" || true
  if grep -qE 'Got signal|Reloading' "$LOG"; then
    green "PASS SIGUSR1 reload observed"
    PASS=$((PASS + 1))
  else
    red "FAIL no reload message in log"
    sed 's/^/    /' "$LOG" | tail -40
    FAIL=$((FAIL + 1))
  fi
else
  red "FAIL process exited before USR1"
  sed 's/^/    /' "$LOG" | tail -40
  FAIL=$((FAIL + 1))
fi

echo ""
echo "=== Summary: $PASS passed, $FAIL failed ==="
if [[ "$FAIL" -gt 0 ]]; then
  exit 1
fi
exit 0
