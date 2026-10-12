# Dependency record

Direct versions are exact pins in Cargo.toml; Cargo.lock pins all transitive
versions. The [inventory](dependency-inventory.tsv) records 402 registry package
versions and their declared SPDX license expressions/repositories from Cargo
metadata on 2026-10-12, including optional/other-platform dependencies. This is an
inventory, not a completed legal review or a binary-specific notices bundle.

| Component | Purpose | Declared license |
| --- | --- | --- |
| eframe / egui 0.33.3 | Native Rust UI, virtualized list, accessibility integration | MIT OR Apache-2.0 |
| quick-xml 0.38.3 | Local XML event parsing, no resolver | MIT |
| rusqlite 0.37.0 | Small Rust storage boundary | MIT |
| libsqlite3-sys 0.35.0 | Bundled SQLite C integration | MIT (wrapper); SQLite public domain |
| sha2 0.10.9 / sha1 0.10.6 / md-5 0.10.6 | Streaming digest compatibility and fingerprints | MIT OR Apache-2.0 |
| crc32fast 1.5.0 | CRC32 catalog compatibility | MIT OR Apache-2.0 |
| serde 1.0.228 / serde_json 1.0.145 | Local record serialization | MIT OR Apache-2.0 |
| anyhow 1.0.100 | Contextual local errors | MIT OR Apache-2.0 |
| libc 0.2.190 (Unix) | O_NOFOLLOW file-open constant | MIT OR Apache-2.0 |
| windows-sys 0.61.2 (Windows) | Startup-failure message box (already in the graph via winit) | MIT OR Apache-2.0 |
| rfd 0.15.4 | Native DAT/folder selection; Windows API and Linux XDG portal boundaries | MIT |
| tempfile 3.23.0 (development only) | Disposable test/measurement directories | MIT OR Apache-2.0 |

Source references: the versioned crates.io/Cargo metadata and upstream repository
URLs in the inventory; [egui](https://github.com/emilk/egui),
[quick-xml](https://github.com/tafia/quick-xml),
[rusqlite](https://github.com/rusqlite/rusqlite), and
[SQLite licensing](https://www.sqlite.org/copyright.html).

Native boundaries: bundled SQLite is C; MSVC/Windows SDK and system graphics/
windowing libraries support Windows; Linux uses X11 or Wayland (loaded at runtime)/XKB/OpenGL and platform
accessibility libraries. Rust UI logic does not require a browser or scripting
runtime. Native pickers use Windows dialogs and the Linux desktop portal through
local D-Bus; manual path entry remains available. macOS picker behavior is outside
the current validation targets. No application logic was moved out of Rust.
Rust/compiler tooling and OS libraries are build/runtime dependencies
distinct from application logic. AccessKit and default fonts require their own
notices in a distribution; preserve upstream bundled font notices.

Before distribution, review the selected binary's actual enabled dependencies,
upstream license text and font/native notices, run a current vulnerability audit,
and produce the required notice bundle. Resolve the application code license
separately. No third-party catalog, commercial ROM or artwork is bundled.
