# Skyline foundation

Status: first read-only development slice, version 0.0.1. M0/M1 remain partial
until the platform and performance evidence in AGENTS.md is recorded.

## Decisions

- Rust 1.99.0 is pinned; Cargo.lock pins the graph and direct versions are exact.
- skyline-core owns deterministic services/models/storage; skyline-desktop owns
  presentation/background jobs. skyline-cli exercises the same services offline.
- egui/eframe 0.33.3 is the initial native prototype (MIT OR Apache-2.0), using
  OpenGL/glow, bundled fonts and AccessKit. It renders visible rows and wakes on
  input/worker notifications. It has a Linux/X11 path and no browser runtime.
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

Users choose the system explicitly, never from filename inference. Import preserves
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

Recursive scans stream .gb/.gbc/.rom/.bin through CRC32/MD5/SHA-1/SHA-256 in one pass.
No header transformation or source writes occur. ZIP/other formats stay Unsupported.
Relative-prefix exclusions and cancellation are supported. Links/reparse points
are skipped; recursion is capped at 128, records at 100,000. Size/mtime changes and
Unix file identity are checked. The prototype assumes a stable directory tree;
hardened descriptor-relative traversal against concurrent parent swaps remains
required before an executor. Every scan rehashes; no size/mtime hash cache exists.

One background job includes database I/O. A bounded 16-message channel throttles
progress to 100 ms. Cancellation checks parser events, entries and 64 KiB reads.
Completed scans transactionally replace the last saved collection. Cancelled/failed
scans retain saved data; partial rows are temporary. SQLite uses WAL/FULL synchronous.
Results are historical evidence. Search/sort runs on bounded in-memory rows on the
GUI thread; measure it and move/index/page it if the target is exceeded.

## Open work

Native file dialogs, saved UI settings/catalog preferences, structured metadata,
expected-entry selection, richer table columns, Windows GUI/accessibility/scaling
validation and resource/latency baselines. ZIP, planning, copy/move, journals/history,
recovery and undo are not implemented. Provider download adapters remain disabled.
