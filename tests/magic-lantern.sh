#!/usr/bin/env bash
# Kiosk / autostart helper for testing.
# Copy to ~/.local/bin or reference from .config/autostart/.
# See: https://man7.org/linux/man-pages/man7/file-hierarchy.7.html

set -euo pipefail

# cd so that log files end up in the home directory
cd ~

# Prefer an installed binary; fall back to a release build in a checkout.
if command -v magic-lantern >/dev/null 2>&1; then
  exec magic-lantern "$@"
fi

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
if [[ -x "$ROOT/target/release/magic-lantern" ]]; then
  exec "$ROOT/target/release/magic-lantern" "$@"
fi

echo "magic-lantern not found on PATH or at $ROOT/target/release/magic-lantern" >&2
echo "Build with: cargo build --release  (see docs/build_rust.md)" >&2
exit 1
