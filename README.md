# Skyline

A Rust desktop ROM manager for offline identification, explainable verification and
file preservation across many systems. **0.0.1 is a read-only development prototype**
for loose cartridge files and local Logiqx XML DATs. NES, SNES, Nintendo 64, GBA,
DS, Sega, Atari, PC Engine, Neo Geo Pocket, WonderSwan and other loose representations
share the same byte-verification pipeline. See [format coverage](docs/formats.md)
for the supported workflow and remaining adapters. Read [AGENTS.md](AGENTS.md)
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

## Development builds

Every push to `main` that passes CI uploads Windows x64 and Linux x64 builds as
artifacts on that [Actions run](https://github.com/esotericode/Skyline/actions/workflows/ci.yml)
(signed-in GitHub users; kept 30 days). They are named
`Skyline-v<version>-dev.<commit>-<platform>`, include `BUILD-INFO.txt` and
`SHA256SUMS.txt`, and are development snapshots, not releases. Builds are unsigned, so
Windows SmartScreen may warn before the first launch. Windows: extract and run
`Skyline.exe` (needs OpenGL 2 or newer). Linux: extract the `.tar.gz` and run `Skyline`
(X11 or Wayland, OpenGL and libxkbcommon). Package locally after a release build with
`scripts/package-dev-build.sh <windows-x64|linux-x64>`.

## Try it

1. Browse to a local UTF-8 XML DAT and click **Import catalog**. Leave the optional
   system override blank to use the DAT header name, or enter/select any system label.
   DATs are user-supplied; none are bundled. Repeat for the systems in your collection.
2. Select **All imported catalogs** to identify a mixed collection, or one snapshot
   to restrict reference evidence for the next scan.
3. Choose a ROM folder and optional relative exclusion prefixes (one per line), then
   click **Scan and verify**. Common loose cartridge extensions and additional
   extensions declared by active DATs are checked. File extensions do not establish
   the system or identity; expected sizes and hashes do.
4. Browse/search/filter while the worker runs. **Cancel job** or Escape cancels.
   Search system names or use **Matched system** to browse a mixed collection.
   Select a row for calculated/expected hashes, reference flags and source evidence.
5. Check the version in the header or **About** window.

All scans reread bytes and preserve originals. Unknown does not mean corrupt;
CRC-only has reduced confidence; matching a bad dump preserves its flag. Shared bytes
across system identities remain Ambiguous. ZIP, dedicated containers and unsupported
sets are visibly Unsupported. Recognized NES/Atari 7800/Lynx/SNES copier headers are
excluded for a second comparison when the whole file has no match, and the details
say so; the file is never changed. Byte order (N64 .v64/.n64) is not converted. File
organization and mutations are deferred.

Catalogs, exact snapshots and the last completed scan persist in
`%LOCALAPPDATA%\Skyline` on Windows; Linux development uses `$XDG_DATA_HOME/Skyline`
or `~/.local/share/Skyline`. Override with `SKYLINE_DATA_DIR`. Cancelled/failed scans
retain saved data; partial cancelled rows are temporary. Evidence is historical:
rescan after file/reference changes. Paths, exclusions, source selection, search,
system/status filters and sorting preferences are saved locally after editing and restored at
startup. Picking a path selects it; import and scan still require their buttons.
Manual path entry remains available if a native picker is unavailable.

Older databases upgrade automatically from their preserved DAT snapshots. Catalog
provenance and historical scan results are retained; rescan to use refreshed catalog
entries. There is no need to delete the database or import the same DAT again.

## Internal CLI

Keep the database outside the ROM folder when possible:

```sh
cargo run --locked -p skyline-core --bin skyline-cli -- --version
cargo run --locked -p skyline-core --bin skyline-cli -- import ./skyline.sqlite3 auto ./nes.dat
cargo run --locked -p skyline-core --bin skyline-cli -- import ./skyline.sqlite3 auto ./megadrive.dat
cargo run --locked -p skyline-core --bin skyline-cli -- scan ./skyline.sqlite3 ./roms Backups
```

CLI output is JSON evidence. Tests use disposable synthetic bytes and no network.
`auto` uses the DAT header as the system label; an explicit custom label also works.
The scan compares against all imported systems in the database.

See [architecture](docs/architecture.md), [versioning](docs/versioning.md),
[dependency notices](docs/dependencies.md), [performance](docs/performance.md), and
[changelog](CHANGELOG.md). Windows GUI validation and distribution remain pending;
Linux core checks do not establish desktop support. Code license awaits owner choice.
