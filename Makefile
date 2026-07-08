# Convenience targets for the Rust port (repo root).
#
# If you see:  rust-lld: error: unable to find library -lSDL2
# then either:  sudo apt install libsdl2-dev
# or:           make will auto-use ~/.local/sdl2-prefix when present.

SDL_PREFIX ?= $(HOME)/.local/sdl2-prefix
SDL_LIB    := $(SDL_PREFIX)/usr/lib/x86_64-linux-gnu
SDL_PC     := $(SDL_LIB)/pkgconfig

# Only inject local prefix if system sdl2.pc is missing and local exists.
ifneq ($(shell pkg-config --exists sdl2 2>/dev/null && echo yes),yes)
  ifneq ($(wildcard $(SDL_PC)/sdl2.pc),)
    export PKG_CONFIG_PATH := $(SDL_PC):$(PKG_CONFIG_PATH)
    export LIBRARY_PATH    := $(SDL_LIB):$(LIBRARY_PATH)
    export LD_LIBRARY_PATH := $(SDL_LIB):$(LD_LIBRARY_PATH)
  endif
endif

.PHONY: help build release test dry-run validate install clean fmt clippy env-check

help:
	@echo "Targets:"
	@echo "  make build     - debug cargo build"
	@echo "  make release   - optimized release binary"
	@echo "  make test      - cargo test"
	@echo "  make dry-run   - list 10 slides from tests/images/numbers"
	@echo "  make validate  - Phase 5 validation harness"
	@echo "  make env-check - show whether SDL2/PDFium are discoverable"
	@echo "  make install   - cargo install --path ."
	@echo ""
	@echo "SDL2 link error?  sudo apt install libsdl2-dev"

env-check:
	@echo -n "pkg-config sdl2: "; pkg-config --exists sdl2 && pkg-config --modversion sdl2 || echo "MISSING (need libsdl2-dev or local prefix)"
	@echo "PKG_CONFIG_PATH=$(PKG_CONFIG_PATH)"
	@echo "LIBRARY_PATH=$(LIBRARY_PATH)"
	@echo -n "PDFium: "; test -f "$(HOME)/.local/pdfium/lib/libpdfium.so" && echo "$(HOME)/.local/pdfium/lib/libpdfium.so" || echo "not in ~/.local/pdfium/lib (set PDFIUM_LIB_PATH)"

build:
	cargo build

release:
	cargo build --release
	@echo "Binary: target/release/magic-lantern"
	@ls -lh target/release/magic-lantern

test:
	cargo test

dry-run:
	cargo run -q -- --dry-run 10 tests/images/numbers

validate:
	./scripts/phase5_validate.sh

install:
	cargo install --path .

clean:
	cargo clean

fmt:
	cargo fmt

clippy:
	cargo clippy --all-targets -- -D warnings
