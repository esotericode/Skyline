# Skyline foundation

Status: first read-only development slice, version 0.0.1. M0/M1 remain partial
until the platform and performance evidence in AGENTS.md is recorded.

## Decisions

- Rust 1.99.0 is pinned; Cargo.lock pins the graph and direct versions are exact.
- skyline-core owns deterministic services/models/storage; skyline-desktop owns
  presentation/background jobs. skyline-cli exercises the same services offline.
- egui/eframe 0.33.3 is the initial native prototype (MIT OR Apache-2.0), using
  OpenGL/glow, bundled fonts and AccessKit. It renders visible rows and wakes on
  input/worker notifications. It has Linux/X11 and Wayland paths and no browser runtime.
  This documented API pin is sufficient for the prototype; it is not the latest
  available release. Windows evaluation must confirm this provisional selection.
- Alternatives: Iced's application/message structure adds setup to this small
  prototype; Slint's declarative UI/accessibility merits evaluation but its licensing
  options require owner review. egui trades native appearance for a small Rust UI;
  keyboard/accessibility/scaling and resource use must be measured on Windows.
- SQLite through rusqlite uses bundled SQLite (C, public domain), isolated in
  store.rs. Transactions avoid inventing crash-safe replacement rules for a JSON
  store. Native windowing/graphics/compiler/OS dependencies are permitted boundaries;
  all application logic stays in Rust. No JavaScript or web frontend is used.
- Proposed validation targets: Windows 11 x64 and Windows 10 22H2 x64, not a claim
  of tested compatibility or Microsoft support. Linux GUI support remains deferred.
- The code license awaits owner selection; distribution requires a notices review.

## Workflow and boundaries

Imports use the DAT header name as the system label by default, with any explicit
custom override accepted. System identity requires matching catalog byte evidence,
never filename/extension inference. Import preserves
exact XML bytes in an immutable SHA-256 snapshot with local origin, timestamp, DAT
name/version, format, and unknown license status. External DOCTYPE identifiers are
inert; internal DTD subsets/entities are rejected. UTF-8 Logiqx only, capped at
64 MiB, one million entries, 32 levels and 8192 bytes per attribute/header field.
Failed imports retain prior catalogs. No provider requests or ROM uploads exist.

Digest indexes find candidates; comparisons check all supplied supported digests
and expected size. CRC-only results have reduced confidence, nodump cannot verify,
and suspect flags survive exact matches. Conflicting identities stay ambiguous.
Contradictory partial evidence stays Unknown with rejection details; reference
mismatch requires a future user-selected expectation. Multi-file entries and
unsupported digest requirements remain read-only/Unsupported. Catalog names retain
raw region/revision/language text; structured metadata extraction is pending.

Recursive scans stream common loose cartridge representations and extensions declared
by active DAT entries through CRC32/MD5/SHA-1/SHA-256 in one pass. A policy derived
once per index enables custom extensions, and extensionless files of catalog sizes,
without a system-specific code change. Known archives/disc containers/packages cannot opt into loose checks through
a DAT. Matching also refuses to verify a container entry by whole-file hashes.
Recognized copier/emulator headers get a second, header-free digest set in the same
read, compared only when the whole file has no candidate; no byte-order conversion
or source writes occur. See docs/formats.md.
Relative-prefix exclusions and cancellation are supported. Symlinks and junctions
are skipped (other Windows reparse points such as OneDrive placeholders are hashed); recursion is capped at 128, records at 100,000. Size/mtime changes and
Unix file identity are checked. The prototype assumes a stable directory tree;
hardened descriptor-relative traversal against concurrent parent swaps remains
required before an executor. Every scan rehashes; no size/mtime hash cache exists.

One background job includes database I/O. A bounded 16-message channel throttles
progress to 100 ms. Cancellation checks parser events, entries and 64 KiB reads.
Completed scans transactionally replace the last saved collection. Cancelled/failed
scans retain saved data; partial rows are temporary. SQLite uses WAL/FULL synchronous.
Results are historical evidence. Search/sort runs on bounded in-memory rows on the
GUI thread; measure it and move/index/page it if the target is exceeded.

Database schema 2 records a parser revision for each derived catalog cache and one
bounded preferences row. Opening schema 1 acquires an immediate transaction, checks
each preserved snapshot's SHA-256/size and reparses its entries, keeping original
origin/provider/license/time. Snapshot bytes and saved scan evidence remain unchanged.
Failures/cancellation roll back the entire migration. No original DAT file, network
request or database deletion is needed. Increment PARSER_REVISION for future semantic
parser changes; newer schemas/parser caches reject older readers.

Preferences include paths, exclusions, import system override, chosen source,
query, matched-system/status filters and
sort. The added optional system filter defaults to All systems in older settings;
existing explicit import labels are retained. Parser revision stays 1: automatic
labels apply only to new imports with an empty override, and no derived entries or
existing provenance need reparsing/relabeling. Sources use SHA-256 plus system instead
of unstable array indices. Changes save
after a 500 ms idle debounce via the job worker. Jobs delay that save until idle; quit
flushes the latest edit on a worker after an outstanding save finishes. Broken settings
fall back visibly to defaults without replacing the stored payload on startup. Settings
never authorize imports, scans or source mutations. Preference controls wait for the initial restore before accepting edits, so startup cannot overwrite newly typed values. JSON payloads are capped at 1 MiB.

rfd 0.15.4 supplies native pickers, parented to the eframe window and invoked on a
worker. Selection fills a path only; Import/Scan remain explicit. Cancelling preserves
the old path. While a picker holds native parent handles, quitting is deferred until
it returns, then the quit request is honored. Windows/Linux interactive dialog smoke
tests and portal failure/focus behavior still need platform validation.

## Open work

Structured metadata, expected-entry selection, per-system preference profiles,
Windows GUI/accessibility/scaling, interactive dialog validation and resource/latency
baselines. ZIP, planning, copy/move, journals/history,
recovery and undo are not implemented. Provider download adapters remain disabled.
