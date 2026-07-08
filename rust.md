# Porting magic-lantern to Rust

This document is a plan for reimplementing [magic-lantern](https://github.com/normanlorrain/magic-lantern) (Python) in Rust, based on analysis of the current codebase (`src/magic_lantern/`).

---

## 1. What the application does

**magic-lantern** is a kiosk / digital-signage slideshow player. It:

1. Loads images (and PDF pages rendered as images) from one directory or from a multi-album TOML config.
2. Displays them full-screen or windowed on a schedule, with keyboard and signal control.
3. Supports weighted random album selection, sequential/random/atomic album order, EXIF orientation and date overlay, and reload via `SIGUSR1`.

Typical deployment: a dedicated machine running continuously (e.g. GNOME autostart).

---

## 2. Architecture of the Python codebase

### 2.1 Module map

| Module | Role |
|--------|------|
| `cli.py` | Click CLI entry; validates exclusive config vs directory; reload loop |
| `config.py` | Merge CLI + TOML; defaults; album validation; module-level config attributes |
| `controller.py` | Event loop (pygame events), pause/year/next/prev, timer, SIGUSR1 reload |
| `slideshow.py` | Infinite slide generator over weighted albums; history for previous |
| `album.py` | Walk tree, filter images/PDFs, order (sequence/random/atomic) |
| `slide.py` | Lazy load, EXIF, scale-to-fit, pygame Surface |
| `pdf.py` | MuPDF (`pymupdf`) → temp PNGs per page |
| `screen.py` | Window / fullscreen init, display surface |
| `text.py` | Overlay text (PAUSE, filename, datetime, year) |
| `signal.py` | POSIX `SIGUSR1` → custom pygame event |
| `log.py` | Rotating debug/error files + console INFO |
| `__main__.py` | Optional `cProfile` via `MAGIC_LANTERN_PROFILE` |

### 2.2 Runtime flow

```
cli (click)
  └─ while reload:
       config.init(ctx)          # CLI + TOML → module attrs + albums[]
       controller.init()
         screen.init()           # pygame display
         slideshow.init()        # Album[] + generators
         text.init()
         signal.init()           # SIGUSR1
       controller.run()
         showNewSlide()
         set_timer(PHOTO_EVENT)
         loop: wait event
           PHOTO_EVENT → next slide (if not paused)
           keys: q/n/p/y/space
           WINDOWCLOSE/QUIT → exit
           SIGUSR1 → return True (reload)
```

### 2.3 Class / ownership model

```
Controller
  └── SlideShow
        └── Album[]  (weighted)
              └── Slide[]  (lazy image load)
```

- **SlideShow**: infinite generator; weighted `random.choices` when `shuffle`, else round-robin albums via `itertools.cycle`.
- **Album order**:
  - `sequence`: sorted paths, infinite wrap
  - `random`: shuffled once at load, infinite wrap
  - `atomic`: yield all slides in order, then `StopIteration` so the slideshow picks another album
- **History**: ring of last ~10 slides; previous navigates history; oldest unloads image to bound memory.

### 2.4 Configuration model

**Sources** (CLI wins over TOML over defaults):

| Key | Scope | Type | Default |
|-----|--------|------|---------|
| `config_file` / `directory` | CLI | path | mutually exclusive, one required |
| `exclude` | global | list[str] | `[]` |
| `fullscreen` | global | bool | `false` |
| `shuffle` | global | bool | `false` |
| `interval` | global / album | int seconds | `5` |
| `weight` | album (docs also mention global) | int | `1` |
| `order` | album | `sequence` \| `random` \| `atomic` | `sequence` |
| `folder` | album | path | required; relative to config file |
| `dry_run` | CLI | int ≥ 1 | off |

Single-directory mode synthesizes one album (`random` if `--shuffle`, else `sequence`).

### 2.5 User controls

| Input | Action |
|-------|--------|
| Space | Toggle pause (timer off; show PAUSE + filename + datetime) |
| `q` | Quit |
| `n` / Right | Next |
| `p` / Left | Previous |
| `y` | Toggle year overlay (top-right, large green) |
| Window close | Quit |
| `SIGUSR1` | Reload config + rebuild slideshow (outer CLI loop) |
| Per-slide `interval` | Timer reset after each slide |

### 2.6 Image pipeline

1. Discover files: recursive walk; skip `exclude` dir names; skip names with `~`.
2. Accept: `.bmp`, `.png`, `.jpg`, `.jpeg`; `.pdf` → convert each page to temp PNG (dpi 600).
3. On display: `pygame.image.load` → EXIF orientation (values 3/6/8) → fit-rect to screen → `smoothscale` → blit centered.
4. EXIF date (`DateTimeOriginal`) for overlays.

### 2.7 External dependencies (Python)

| Python | Purpose | Rust direction |
|--------|---------|----------------|
| `click` | CLI | `clap` |
| `pygame` | Display, events, fonts, images | `sdl2` / `sdl2-sys` or `pixels`+`winit`+`softbuffer`; image load via `image` |
| `exifread` | EXIF | `kamadak-exif` or `rexiv2` |
| `pymupdf` | PDF → raster | `mupdf-sys` / `pdfium-render` / `lopdf`+render crate (evaluate) |
| `tomllib` | Config | `toml` |
| stdlib logging | Logs | `tracing` + `tracing-appender` (rotating) |

---

## 3. Goals for the Rust port

1. **Feature parity** with Python CLI, config, slideshow semantics, controls, and reload.
2. **Drop-in replacement** for kiosk use: same TOML schema, same key bindings, same `pkill -USR1` workflow (or documented PID name change).
3. **Single binary** distribution (`cargo install` / release artifacts) instead of pipx.
4. **Bounded memory**: lazy load + unload history (match ~10-slide window).
5. **Linux-first** (primary deploy target); Windows secondary if SDL2 path works.
6. Preserve MIT license and project identity.

Non-goals for v1:

- Python interop / FFI wrapper
- GUI config editor
- Network remote control
- Video playback
- Pixel-perfect pygame font matching

---

## 4. Proposed Rust crate layout

```
magic-lantern/                 # or crates/magic-lantern
├── Cargo.toml
├── src/
│   ├── main.rs                # entry, optional profiling hook
│   ├── cli.rs                 # clap
│   ├── config.rs              # TOML + CLI merge, validation
│   ├── controller.rs          # event loop, pause/year/timer
│   ├── slideshow.rs           # generators, history
│   ├── album.rs               # discovery, ordering
│   ├── slide.rs               # load/scale/EXIF
│   ├── pdf.rs                 # page rasterization
│   ├── screen.rs              # window / fullscreen
│   ├── text.rs                # overlays
│   ├── signal_handler.rs      # SIGUSR1 (unix)
│   ├── log.rs                 # tracing setup
│   └── error.rs               # thiserror / anyhow
├── tests/
│   ├── config_tests.rs
│   ├── album_tests.rs
│   └── slideshow_tests.rs
└── docs/                      # reuse existing example.toml semantics
```

Optional later: workspace with `magic-lantern-core` (no SDL) + binary for headless dry-run testing.

---

## 5. Suggested crate choices

| Area | Recommended crates | Notes |
|------|-------------------|--------|
| CLI | `clap` (derive) | Mirror flags: `-c`, `-f`, `-s`, `-d`, `-i`, `-e`, `DIRECTORY` |
| Errors | `thiserror` + `anyhow` | Domain errors vs top-level |
| Config | `toml` + `serde` | Structs with defaults |
| Logging | `tracing`, `tracing-subscriber`, `tracing-appender` | Debug + error rotating files in CWD |
| Random | `rand` | Weighted album choice; shuffle |
| Paths | `std::path` | Absolute resolution relative to config file |
| Images | `image` | Load PNG/JPEG/BMP; resize |
| EXIF | `kamadak-exif` | Orientation + DateTimeOriginal |
| Graphics | **`sdl2`** (feature `image`, `ttf`) | Closest behavioral match to pygame |
| PDF | `mupdf` (if maintained bindings) or shell-out / `pdfium-render` | dpi 600 page PNGs to temp dir |
| Temp files | `tempfile` | PDF page cache; clean on drop |
| Signals | `signal-hook` + channel / `signal-hook-tokio` not needed | Post “reload” into event queue |
| Cross-platform fullscreen | via SDL2 | Match `(0,0) FULLSCREEN` vs 1280×720 window |

### 5.1 Why SDL2 first

Pygame is an SDL wrapper. Porting display, keyboard, timers, and fonts through `sdl2` minimizes semantic drift (fullscreen, key repeat, blit, font overlay). Alternatives (`winit` + `pixels`) are viable but require more event-loop redesign.

### 5.2 PDF rendering risk

`pymupdf` at 600 DPI is heavy and slow on large PDFs; behavior must be preserved or intentionally documented if lowered for performance. Spike early:

1. Bindings availability and license of MuPDF/PDFium.
2. Memory/time for multi-page decks.
3. Whether caching rendered pages on disk (as today) is enough.

---

## 6. Core type design (sketch)

```rust
// config.rs
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum Order { Sequence, Atomic, Random }

struct Config {
    fullscreen: bool,
    shuffle: bool,
    interval: u64,          // seconds
    exclude: Vec<String>,
    dry_run: Option<usize>,
    albums: Vec<AlbumConfig>,
}

struct AlbumConfig {
    folder: PathBuf,
    order: Order,
    weight: u32,
    interval: u64,
}

// slide.rs
struct Slide {
    path: PathBuf,
    interval: u64,
    // lazy: Option<PreparedImage> with dimensions, offset, RGBA/texture
}

// album.rs
struct Album {
    order: Order,
    weight: u32,
    interval: u64,
    slides: Vec<Slide>,
    index: usize,
}

// slideshow.rs
struct Slideshow {
    albums: Vec<Album>,
    weights: Vec<u32>,
    shuffle: bool,
    album_cursor: usize,
    history: VecDeque<SlideId>, // or Arc<Slide> / indices
    history_cursor: isize,
    current: Option<SlideId>,
}
```

Prefer **owned structs + explicit init** over Python’s module-level mutable globals (`config.*`, `slideshow._history`, `controller.pauseState`).

---

## 7. Behavioral fidelity checklist

Implement and test these carefully; they define “same app”:

### 7.1 Config merge

1. CLI `--interval` / `--fullscreen` / `--shuffle` / `--exclude` override TOML.
2. Falsy CLI flags must not clobber TOML (Python skips `False` and empty tuples for some params—**replicate that merge logic**).
3. Relative album `folder` resolved against config file parent.
4. Invalid album path: log error, skip album (Python continues); zero slides total → hard error.
5. Unknown top-level TOML keys → configuration error.

### 7.2 Slideshow generator

1. `shuffle=true`: each next slide (non-atomic path) picks album via weighted random; then one slide from that album (or all if atomic).
2. `shuffle=false`: cycle albums in config order; same per-album rules.
3. Atomic: emit entire album sequence then stop that “draw”; next album pick continues.
4. Empty album after filter → error logged / album omitted (match Python).
5. History: max 10 loaded; previous/next cursor semantics (`getPreviousSlide` special-case when cursor == 0).

### 7.3 Display

1. Default window **1280×720**; fullscreen uses display size; mouse hidden.
2. Letterbox / fit-rect (not stretch); black background.
3. EXIF orientation transforms before scale.
4. Smooth scale (linear/bilinear).
5. Per-slide interval in seconds → timer milliseconds.

### 7.4 Controls & reload

1. Key repeat: pygame sets `(1000, 100)` ms—match or approximate.
2. After handling a key, clear queued KEYDOWN/KEYUP (debounce).
3. `SIGUSR1` only on Unix; rebuild config + slideshow without process exit.
4. Dry-run: print `i parent/name` for N slides, no window.

### 7.5 Logging

1. CWD files: `magic-lantern-debug.log`, `magic-lantern-error.log` (truncate/recreate on start).
2. Rotating ~100_000 bytes, 1 backup.
3. Console at INFO (short format); files detailed with location.

---

## 8. Implementation phases

### Phase 0 — Spike (1–3 days) ✅ done

- [x] Window + blit scaled image + keyboard quit (`rs/examples/spike_display.rs`, via `minifb`).
- [x] EXIF orientation / DateTimeOriginal on test images (`rs/examples/spike_exif.rs`).
- [x] PDF: convert page 0 of `tests/pdfs/Example presentation.pdf` to PNG (`rs/examples/spike_pdf.rs`).
- [x] Decision record: final crate set for graphics + PDF (see §18 below).

**Run the spikes** (from repo root):

```bash
cargo run --example spike_exif
cargo run --example spike_pdf
cargo run --example spike_display
```

### Phase 1 — Core non-UI (parity unit tests)

- [ ] `config`: load `docs/example.toml` + CLI merge; reject `tests/bad.toml`.
- [ ] `album`: walk `tests/images/*`, exclude dirs, PDF expansion, order modes.
- [ ] `slideshow`: deterministic tests with fixed RNG seed for weights/shuffle.
- [ ] Dry-run path prints same style as Python.

### Phase 2 — Slide pipeline

- [ ] Lazy load/unload.
- [ ] Fit-to-screen math (port pygame `Rect.fit` algorithm exactly).
- [ ] Temp dir lifecycle for PDF pages.

### Phase 3 — Controller / UI

- [ ] Event loop: timer, pause, year, next, previous.
- [ ] Text overlays (system sans-serif, two sizes, green).
- [ ] `SIGUSR1` reload loop around config + controller.

### Phase 4 — CLI polish & packaging

- [ ] `clap` help/version matching README flags.
- [ ] Binary name `magic-lantern` for `pkill -USR1 magic-lantern`.
- [ ] Release profile, optional static/SDL dynamic linking notes for Debian.
- [ ] Document build deps (SDL2, SDL2_image, SDL2_ttf, PDF lib).

### Phase 5 — Validation

- [ ] Manual kiosk run with `docs/example.toml` (adjust paths under `tests/`).
- [ ] Compare sequence of dry-run output Python vs Rust (seed-controlled where random).
- [ ] Memory: long run with large albums; confirm history unload.
- [ ] Reload with file changes.

---

## 9. Mapping Python modules → Rust work items

| Python | Rust work | Priority |
|--------|-----------|----------|
| `config.py` | Serde structs, validation, path resolve | P0 |
| `album.py` | `walkdir` / `std::fs`, filters, PDF hook | P0 |
| `slideshow.py` | Weighted selection, history deque | P0 |
| `slide.py` | `image` + exif + scale + SDL texture/surface | P0 |
| `screen.py` / `text.py` | SDL2 video + TTF | P0 |
| `controller.py` | Main loop | P0 |
| `cli.py` | clap + reload while | P0 |
| `pdf.py` | PDF renderer spike | P0 (risk) |
| `log.py` | tracing | P1 |
| `signal.py` | signal-hook | P1 |
| `__main__.py` profiling | optional `pprof` / env flag | P2 |
| `tests/test_config.py` | expand into real unit tests | P1 |

---

## 10. API surface (CLI) to preserve

```text
magic-lantern [OPTIONS] [DIRECTORY]

Options:
  --version
  -c, --config-file <FILE>
  -f, --fullscreen
  -s, --shuffle
  -d, --dry-run <N>        # N >= 1
  -i, --interval <SECS>    # >= 1
  -e, --exclude <DIR>      # repeatable
  -h, --help
```

Epilog: document `pkill -USR1 magic-lantern`.

Exit codes:

- 0: normal quit / dry-run complete
- non-zero: configuration error, no images, fatal I/O

---

## 11. Testing strategy

| Layer | Approach |
|-------|----------|
| Unit | Config merge, order enums, rect-fit math, history cursor |
| Integration | Album discovery over `tests/images/`; dry-run counts |
| Golden | Fixed seed: first N slide basenames match fixture list |
| Manual | Fullscreen, pause overlay, year, SIGUSR1, bad files skipped |
| Property | Random walks never panic; empty dirs fail cleanly |

Prefer **headless** tests for core; gate SDL tests behind `#[ignore]` or feature `ui-tests`.

---

## 12. Risks and mitigations

| Risk | Mitigation |
|------|------------|
| PDF crate quality / license | Spike Phase 0; fallback: keep Python helper only for PDF (not preferred) or document “images only” MVP then add PDF |
| SDL2 system deps on target machines | Document `libsdl2-dev` etc.; consider `sdl2` bundling strategy for releases |
| EXIF edge cases | Test against real photos in repo; map orientation tags 1–8 fully if needed |
| Weighted random differs from CPython | Document; use golden tests with seeded RNG; optional “compatibility mode” not required |
| pygame `Rect.fit` subtlety | Port algorithm from pygame source; unit test known rectangles |
| Config global `weight` in README vs code | Match **actual Python behavior** (album default 1; verify whether global `weight` is accepted) |
| Large PDF at 600 DPI OOM | Stream pages; same tempfile approach; consider configurable DPI later |
| Windows SIGUSR1 | No-op + log; document Linux reload only (same as Python `os.name == "posix"`) |

---

## 13. Migration path for users

1. Keep Python package published during port.
2. Ship Rust binary as `magic-lantern` (or `magic-lantern-rs` until parity).
3. Config TOML **unchanged** — no schema break.
4. Update README install section: cargo / deb / release tarball.
5. Autostart units: swap binary path only.
6. Deprecate Python when parity checklist is signed off.

---

## 14. Suggested implementation order (PR-sized)

1. **Scaffold**: crate, clap stub, tracing, error types.
2. **Config + dry-run (no SDL)**: load albums, generate sequence, print paths.
3. **Slide load + EXIF + fit math** (CPU only, write debug PNG optional).
4. **SDL display + timer + keys**.
5. **Overlays + pause/year**.
6. **PDF conversion**.
7. **SIGUSR1 reload**.
8. **Packaging docs + parity tests**.

Each PR should leave `cargo test` and dry-run usable.

---

## 15. Definition of done (v1 Rust)

- [ ] All CLI options behave as documented in README.
- [ ] `docs/example.toml` semantics work with `tests/` assets.
- [ ] Keyboard controls: space, q, n/p, arrows, y.
- [ ] History previous/next with memory unload.
- [ ] PDF pages appear as slides.
- [ ] EXIF orientation and year/date overlays.
- [ ] Unix `SIGUSR1` reloads config.
- [ ] Dry-run mode without opening a window.
- [ ] Logging to rotating files + console.
- [ ] Build and run instructions for Debian-like systems.
- [ ] No regression on empty/bad path handling (clear errors).

---

## 16. Appendix — Python source reference summary

| File | LOC (approx) | Complexity notes |
|------|----------------|------------------|
| `controller.py` | ~180 | Central event loop; global pause/year state |
| `config.py` | ~200 | Module setattr pattern; careful CLI/TOML merge |
| `slideshow.py` | ~110 | History cursor edge cases |
| `album.py` | ~80 | Filesystem walk + PDF |
| `slide.py` | ~100 | EXIF + scale |
| `cli.py` | ~110 | Outer reload loop |
| `pdf.py` | ~30 | Temp dir singleton |
| `screen.py` / `text.py` / `signal.py` / `log.py` | small | Thin adapters |

Overall surface area is **small (~1k LOC)**; the port is dominated by **dependency choice** (SDL, PDF, EXIF) and **faithful generator/history semantics**, not by algorithmic complexity.

---

## 17. Next concrete step

Phase 0 is complete (scaffold + spikes + decisions below). **Next: Phase 1** — config, album discovery, slideshow generator, dry-run path, with unit tests against `tests/`.

---

## 18. Phase 0 decision record

Recorded after running the spikes on this machine (2026-07-08). Environment notes: `libsdl2-2.0-0` runtime present, **`libsdl2-dev` not installed** (no sudo); `poppler-utils` (`pdftoppm`) available; `DISPLAY=:0` available.

### 18.1 Layout

| Choice | Decision |
|--------|----------|
| Crate location | Repo-root `Cargo.toml`; Rust sources under **`rs/`** so they do not collide with Python `src/magic_lantern/` |
| Binary name | `magic-lantern` (preserves `pkill -USR1 magic-lantern`) |
| Package version | Start at `0.1.0` until feature parity with Python `0.0.20` |

### 18.2 Crate set (locked in for the port)

| Concern | Crate / tool | Rationale |
|---------|--------------|-----------|
| CLI | **`clap`** (derive) | Phase 1+; not needed for spikes |
| Config | **`toml` + `serde`** | Phase 1+ |
| Errors | **`thiserror` + `anyhow`** | Phase 1+ |
| Logging | **`tracing` + `tracing-subscriber` + `tracing-appender`** | Phase 1+ |
| Random | **`rand`** | Phase 1 slideshow weights |
| Images | **`image` 0.25** | Loads JPEG/PNG/BMP used in `tests/`; resize with `FilterType::Triangle` (bilinear stand-in for pygame smoothscale) |
| EXIF | **`kamadak-exif` 0.6** | Reads `Orientation` + `DateTimeOriginal`; works on sample paintings (dates present). Apply pygame-compatible CCW rotations: tag 3→180°, 6→270°, 8→90° |
| Display (spike) | **`minifb` 0.28** | Proved 1280×720 window, letterbox blit, quit on `q`/Esc **without** SDL headers |
| Display (product) | **`sdl2`** with features `image`, `ttf` | Still the production target for pygame parity (fullscreen, fonts, timers, key repeat). Requires `libsdl2-dev`, `libsdl2-image-dev`, `libsdl2-ttf-dev` at build time. Fall back to minifb-only path only if packaging SDL is unacceptable |
| PDF (spike) | **`pdftoppm`** (Poppler CLI) | Page 0 @ 600 DPI → PNG `4410×2481` for `Example presentation.pdf`; zero extra Rust deps |
| PDF (product) | **Prefer in-process `pdfium-render`**, fallback **subprocess `pdftoppm`** | Avoid system MuPDF `-dev` dependency; PDFium is widely used and license-friendly for distribution. Keep tempfile-per-page cache like Python. If `pdfium-render` integration is painful, ship the Poppler CLI fallback behind a feature flag `pdf-poppler` |
| Temp files | **`tempfile`** | PDF page cache + spike outputs |
| Signals | **`signal-hook`** | Phase 3+; Unix `SIGUSR1` only |
| Walk dirs | **`walkdir`** | Phase 1 album discovery |

### 18.3 Spike results

| Spike | Result |
|-------|--------|
| `spike_exif` | All `tests/images/**` load via `image`. Paintings expose `DateTimeOriginal`; no orientation tags in current fixtures (code path for 3/6/8 still implemented). Thumbnail written to `/tmp/magic-lantern-spike-exif.png`. |
| `spike_pdf` | `pdftoppm -png -r 600 -singlefile` succeeded; output `/tmp/magic-lantern-spike-pdf.png`. |
| `spike_display` | Window opened, image fitted (e.g. rembrandt 689×899 → 552×720 at offset (364,0)), quit on key works. |

### 18.4 Open follow-ups (not blocking Phase 1)

1. Install SDL2 **dev** packages on build hosts and add a gated `spike_sdl2` example before Phase 3.
2. Spike `pdfium-render` (download/link) before implementing production `pdf.rs`.
3. Port pygame `Rect.fit` exactly (unit tests) — current fit math is aspect-correct letterbox, not yet line-for-line.
4. Test images lack orientation tags 3/6/8; add a fixture or synthetic EXIF sample in Phase 2 tests.

### 18.5 Dependency footprint (Phase 0 Cargo.toml)

Only what the spikes need today:

- `image`, `kamadak-exif`, `tempfile`, `minifb`

Phase 1 will add `clap`, `serde`, `toml`, `rand`, `thiserror`, `anyhow`, `tracing*`, `walkdir` without pulling SDL/PDF into the default build until later phases.
