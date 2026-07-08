# Convenience targets for the Rust port (repo root).

.PHONY: help build release test dry-run install clean fmt clippy

help:
	@echo "Targets:"
	@echo "  make build     - debug cargo build"
	@echo "  make release   - optimized release binary"
	@echo "  make test      - cargo test"
	@echo "  make dry-run   - list 10 slides from tests/images/numbers"
	@echo "  make install   - cargo install --path .  (~/.cargo/bin)"
	@echo "  make clean     - cargo clean"
	@echo "  make fmt       - cargo fmt (if rustfmt available)"
	@echo "  make clippy    - cargo clippy -D warnings (if available)"

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

install:
	cargo install --path .

clean:
	cargo clean

fmt:
	cargo fmt

clippy:
	cargo clippy --all-targets -- -D warnings
