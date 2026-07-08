# Building magic-lantern (Rust)

The Rust port lives alongside the Python package on the `rust` branch. Sources are under `rs/`; the Cargo package root is the repository root.

The installed / release **binary name is `magic-lantern`**, so reload still works:

```bash
pkill -USR1 magic-lantern
```

---

## Prerequisites (Debian / Ubuntu)

### Always required

| Package / tool | Why |
|----------------|-----|
| `build-essential` / `rustc` + `cargo` | Compile the project (Rust 1.70+ recommended; CI uses stable) |
| A C toolchain (`gcc`, `pkg-config`) | Native deps of crates such as `minifb` |
| X11 or Wayland session | Windowed UI (`minifb`) |

Install a toolchain (pick one):

```bash
# Distro packages (versions vary)
sudo apt install cargo rustc build-essential pkg-config

# Or rustup (recommended for current stable)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source "$HOME/.cargo/env"
```

### Runtime features

| Feature | Debian packages | Notes |
|---------|-----------------|-------|
| **PDF slides** | `poppler-utils` | Provides `pdftoppm` (used to rasterize PDF pages at 600 DPI) |
| **Text overlays** | fonts with a sans-bold face | Tries FreeSans Bold, DejaVu Sans Bold, Liberation Sans Bold, Ubuntu Bold |
| **Images** | none extra | JPEG/PNG/BMP via pure-Rust `image` crate |

```bash
sudo apt install poppler-utils \
  fonts-freefont-ttf fonts-dejavu-core fonts-liberation
```

### Display (SDL2)

```bash
sudo apt install libsdl2-dev
```

Runtime also needs the SDL2 shared library (`libsdl2-2.0-0`), usually pulled in by `-dev`.

Optional (not required to link today — images go through the `image` crate, text via `fontdue`):

```bash
sudo apt install libsdl2-image-dev libsdl2-ttf-dev
```

**pkg-config:** build needs `sdl2.pc` on `PKG_CONFIG_PATH`. If SDL2 is installed in a custom prefix:

```bash
export PKG_CONFIG_PATH="/path/to/prefix/lib/pkgconfig:$PKG_CONFIG_PATH"
export LD_LIBRARY_PATH="/path/to/prefix/lib:$LD_LIBRARY_PATH"
```

---

## Build

From the repository root:

```bash
# Debug (fast compile)
cargo build
./target/debug/magic-lantern --help

# Release (LTO, stripped — for kiosks)
cargo build --release
./target/release/magic-lantern --version
```

### Tests

```bash
cargo test
# Headless dry-run (no window)
cargo run -- --dry-run 10 tests/images/numbers
```

### Install into `~/.cargo/bin`

```bash
cargo install --path .
# ensure ~/.cargo/bin is on PATH
magic-lantern --help
```

System-wide (example):

```bash
cargo build --release
sudo install -m 755 target/release/magic-lantern /usr/local/bin/magic-lantern
```

---

## Release profile notes

`Cargo.toml` `[profile.release]`:

| Setting | Value | Intent |
|---------|--------|--------|
| `lto` | `true` | Cross-crate optimization |
| `codegen-units` | `1` | Better optimization, slower compile |
| `opt-level` | `3` | Speed |
| `strip` | `true` | Smaller binary |
| `panic` | `"abort"` | Smaller binary; process exits on panic |

For crash diagnosis while still optimized:

```bash
cargo build --profile release-with-debug
```

### Static linking

Fully static Linux binaries are **not** the default. SDL2 is linked dynamically.

- Prefer distro packages + `cargo build --release` on the target machine or matching glibc.
- Cross-compiling musl static builds with SDL2 is non-trivial; not supported out of the box.
- PDF conversion shells out to `pdftoppm` (dynamic system tool), so a “static” binary still needs Poppler at runtime for PDFs.

---

## CLI parity (Python README)

| Flag | Meaning |
|------|---------|
| `-c`, `--config-file FILE` | TOML config |
| `-f`, `--fullscreen` | Full screen / borderless mode |
| `-s`, `--shuffle` | Weighted/random album selection |
| `-d`, `--dry-run N` | Print N slide names only (`N >= 1`) |
| `-i`, `--interval SECS` | Seconds between images (`>= 1`) |
| `-e`, `--exclude DIR` | Exclude directory name (repeatable) |
| `DIRECTORY` | Single image folder (exclusive with `-c`) |
| `-V`, `--version` | Version |
| `-h`, `--help` | Help |

---

## Autostart / kiosk

Same pattern as the Python app: point GNOME autostart (or a systemd user unit) at the **binary path**. Only the path changes from the pipx script to e.g. `/usr/local/bin/magic-lantern` or `~/.cargo/bin/magic-lantern`.

Reload after editing the TOML or image folders:

```bash
pkill -USR1 magic-lantern
```

---

## Coexistence with Python

| | Python | Rust |
|--|--------|------|
| Install | `pipx install magic-lantern` | `cargo install --path .` |
| Package layout | `src/magic_lantern/` | `rs/` + root `Cargo.toml` |
| Config TOML | same schema | same schema |
| Binary name | `magic-lantern` | `magic-lantern` |

Do not install both into the same `PATH` entry without renaming one; the last install wins for the name `magic-lantern`.
