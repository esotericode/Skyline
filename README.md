# Skyline

A Rust desktop ROM manager for offline identification, explainable verification and
file preservation. **0.0.1 is an unreleased read-only development prototype** for
loose Game Boy/Game Boy Color ROMs and local Logiqx XML DATs. Read [AGENTS.md](AGENTS.md)
for project requirements and progress.

## Build

Install [Rust with rustup](https://rustup.rs/); the repo pins Rust 1.99.0. Windows
needs MSVC C++ build tools and the Windows SDK. Linux development needs a C compiler,
pkg-config, X11/XKB/OpenGL libraries (Ubuntu: `build-essential pkg-config libx11-dev
libxkbcommon-dev libxi-dev libgl1-mesa-dev`). SQLite is bundled.

```sh
cargo build --locked --workspace
cargo test --locked --workspace
cargo fmt --all -- --check
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo run --locked -p skyline-desktop
```

## Try it

1. Select Game Boy or Game Boy Color in the sidebar, browse to a local UTF-8 XML DAT
   path and click **Import catalog**. DATs are user-supplied; none are bundled.
2. Select all catalogs or one snapshot for the next scan.
3. Choose a ROM folder and optional relative exclusion prefixes (one per line), then
   click **Scan and verify**. Supported loose extensions: .gb, .gbc, .rom, .bin.
4. Browse/search/filter while the worker runs. **Cancel job** or Escape cancels.
   Select a row for calculated/expected hashes, reference flags and source evidence.
5. Check the version in the header or **About** window.

All scans reread bytes and preserve originals. Unknown does not mean corrupt;
CRC-only has reduced confidence; matching a bad dump preserves its flag. ZIP/other
formats are visibly Unsupported. File organization and mutations are deferred.

Catalogs, exact snapshots and the last completed scan persist in
`%LOCALAPPDATA%\Skyline` on Windows; Linux development uses `$XDG_DATA_HOME/Skyline`
or `~/.local/share/Skyline`. Override with `SKYLINE_DATA_DIR`. Cancelled/failed scans
retain saved data; partial cancelled rows are temporary. Evidence is historical:
rescan after file/reference changes. Paths, exclusions, source selection, search,
filter and sorting preferences are saved locally after editing and restored at
startup. Picking a path selects it; import and scan still require their buttons.
Manual path entry remains available if a native picker is unavailable.

Older databases upgrade automatically from their preserved DAT snapshots. Catalog
provenance and historical scan results are retained; rescan to use refreshed catalog
entries. There is no need to delete the database or import the same DAT again.

## Internal CLI

Keep the database outside the ROM folder when possible:

```sh
cargo run --locked -p skyline-core --bin skyline-cli -- --version
cargo run --locked -p skyline-core --bin skyline-cli -- import ./skyline.sqlite3 "Game Boy" ./reference.dat
cargo run --locked -p skyline-core --bin skyline-cli -- scan ./skyline.sqlite3 ./roms Backups
```

CLI output is JSON evidence. Tests use disposable synthetic bytes and no network.

See [architecture](docs/architecture.md), [versioning](docs/versioning.md),
[dependency notices](docs/dependencies.md), [performance](docs/performance.md), and
[changelog](CHANGELOG.md). Windows GUI validation and distribution remain pending;
Linux core checks do not establish desktop support. Code license awaits owner choice.
