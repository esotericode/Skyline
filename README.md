# Skyline

A Rust desktop ROM manager for offline identification, explainable verification and
file preservation across many systems. **0.0.2 is a read-only development prototype**
for loose cartridge files and local Logiqx XML DATs. NES, SNES, Nintendo 64, GBA,
DS, Sega, Atari, PC Engine, Neo Geo Pocket, WonderSwan and other loose representations
share the same byte-verification pipeline. See [format coverage](docs/formats.md)
for the supported workflow and remaining adapters. All 250 DATs in the owner-selected
Fresh1G1R No-Intro folder were tested for catalog intake; new system labels/extensions
are discovered automatically. Read [AGENTS.md](AGENTS.md)
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
`SHA256SUMS.txt`, and are development testing snapshots. Each different distributed
testing build advances the shared version visible in the UI; commit labels identify
the source, and no tagged GitHub Release is created. Builds are unsigned, so
Windows SmartScreen may warn before the first launch. Windows: extract and run
`Skyline.exe` (needs OpenGL 2 or newer). Linux: extract the `.tar.gz` and run `Skyline`
(X11 or Wayland, OpenGL and libxkbcommon). Package locally after a release build with
`scripts/package-dev-build.sh <windows-x64|linux-x64>`.

## Try it

1. Click **Choose DAT folder…** and point Skyline at your reference collection.
   It finds `.dat` and `.xml` files recursively and indexes them in the background.
   There is no system or encryption/byte-order picker. DATs are user-supplied.
2. Choose your ROM folder, then click **Scan and verify**. Skyline refreshes the
   reference folder and checks every active catalog automatically. **Refresh** can
   update references separately; manual path entry also works.
3. **Your systems** shows only families found in your files. A GBA/N64 collection
   shows GBA/N64 even if the reference folder also contains DS, 3DS and other catalogs.
   Variants group under one heading; file details retain their exact source identities.
4. Browse/search/filter while the worker runs. **Cancel job** or Escape cancels.
   Select a row for calculated/expected hashes, reference flags and source evidence.
   **Reference details** holds optional catalog lists and per-file intake problems.
5. Check the version in the header or **About** window.

Unmatched files may supply a clearly labelled system hint from an extension, allowing
Skyline to mention a missing relevant catalog. Hints never verify a file. Hashes and
expected sizes select actual matches; conflicting identities remain Ambiguous.
Unrelated catalog variants or broken reference files do not generate popups.

All scans reread bytes and preserve originals. Unknown does not mean corrupt;
CRC-only has reduced confidence; matching a bad dump preserves its flag. Shared bytes
across system identities remain Ambiguous. ZIP, dedicated containers and unsupported
sets are visibly Unsupported. Recognized NES/Atari 7800/Lynx/SNES copier headers are
excluded for a second comparison when the whole file has no match, and the details
say so; the file is never changed. **Format & headers** shows detected headers, catalog expectations and loader notes.
Byte order (N64 .v64/.n64) is identified by signature when available and is not converted. File
organization and mutations are deferred.

Catalogs, exact snapshots and the last completed scan persist in
`%LOCALAPPDATA%\Skyline` on Windows; Linux development uses `$XDG_DATA_HOME/Skyline`
or `~/.local/share/Skyline`. Override with `SKYLINE_DATA_DIR`. Cancelled/failed scans
retain saved data; partial cancelled rows are temporary. Evidence is historical:
rescan after file/reference changes. The DAT folder, ROM path, exclusions, search,
system/status filters and sorting preferences are saved locally and restored at startup.
Choosing a DAT folder starts indexing; a ROM scan still requires **Scan and verify**.
Manual path entry remains available if a native picker is unavailable.

Unchanged DATs are fingerprinted without reparsing. Duplicate copies share a snapshot.
A broken replacement keeps that path's last working catalog; other valid DATs still load.
Removed/replaced files stop supplying future scans, while immutable snapshots and
historical results remain stored. Cancellation and storage/limit failures roll back
folder activation. Saved references work when a previously indexed folder is offline;
Skyline shows when it uses them. Intake limits are documented in [catalog folders](docs/catalog-folders.md).

Older databases upgrade automatically from their preserved DAT snapshots. Catalog
provenance and historical scan results are retained; rescan to use refreshed catalog
entries. There is no need to delete the database or import the same DAT again.

## Internal CLI

Keep the database outside the ROM folder when possible:

```sh
cargo run --locked -p skyline-core --bin skyline-cli -- --version
cargo run --locked -p skyline-core --bin skyline-cli -- import-folder ./skyline.sqlite3 ./dats
cargo run --locked -p skyline-core --bin skyline-cli -- scan ./skyline.sqlite3 ./roms Backups
```

CLI output is JSON evidence. Tests use disposable synthetic bytes and no network.
`auto` uses the DAT header as the system label; an explicit custom label also works.
The CLI scan uses current folder members and deliberate individual imports. Retired
folder snapshots remain available as history, rather than entering future scans.

See [architecture](docs/architecture.md), [versioning](docs/versioning.md),
[dependency notices](docs/dependencies.md), [performance](docs/performance.md), and
[changelog](CHANGELOG.md). Windows GUI validation and distribution remain pending;
Linux core checks do not establish desktop support. Code license awaits owner choice.
