# Source this before cargo build when libsdl2-dev is not installed system-wide:
#   source scripts/env-build.sh
#   cargo build --release
#
# Preferred fix (system-wide):
#   sudo apt install libsdl2-dev

# Prefer system pkg-config if present.
if pkg-config --exists sdl2 2>/dev/null; then
  return 0 2>/dev/null || true
  exit 0
fi

# Fallback: SDL2 extracted under ~/.local/sdl2-prefix (see docs/build_rust.md)
_SDL_PREFIX="${MAGIC_LANTERN_SDL_PREFIX:-$HOME/.local/sdl2-prefix}"
_SDL_LIB="$_SDL_PREFIX/usr/lib/x86_64-linux-gnu"
_SDL_PC="$_SDL_LIB/pkgconfig"

if [[ -f "$_SDL_PC/sdl2.pc" ]]; then
  export PKG_CONFIG_PATH="$_SDL_PC${PKG_CONFIG_PATH:+:$PKG_CONFIG_PATH}"
  export LIBRARY_PATH="$_SDL_LIB${LIBRARY_PATH:+:$LIBRARY_PATH}"
  export LD_LIBRARY_PATH="$_SDL_LIB${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
  echo "env-build: using SDL2 from $_SDL_PREFIX" >&2
else
  echo "env-build: SDL2 not found via pkg-config and no prefix at $_SDL_PREFIX" >&2
  echo "  Install:  sudo apt install libsdl2-dev" >&2
  echo "  Or extract libsdl2-dev to ~/.local/sdl2-prefix (see docs/build_rust.md)" >&2
fi

# PDFium (runtime) — optional convenience for run-after-build
if [[ -z "${PDFIUM_LIB_PATH:-}" && -f "$HOME/.local/pdfium/lib/libpdfium.so" ]]; then
  export PDFIUM_LIB_PATH="$HOME/.local/pdfium/lib/libpdfium.so"
  export LD_LIBRARY_PATH="$HOME/.local/pdfium/lib${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
fi
