# Review and T004 follow-up — 2026-10-12

Reviewed commits 7369517, 1c75639 and fe1f58d against a14397c, including the full core
and desktop diffs and updated charter/handoff. Concur with the fixes: descriptive DAT
attributes do not change digest byte coverage; required transformations/unknown digest
requirements still fail closed. Rust 1.99's Windows FileType classifier uses the
name-surrogate reparse tag bit, preserving link/junction skipping without blanket
rejection of data-bearing reparse points. The table, fixed row height, sorting,
keyboard navigation, Wayland feature and visible startup errors address concrete
compatibility and usability problems. Existing review CI passes on Windows/Linux
([run 38295304408](https://github.com/esotericode/Skyline/actions/runs/38295304408)).

Follow-up correction: the review recommended deleting/reimporting old databases
because parsed entries were stale. That workaround is superseded by transactional
schema/cache refresh from preserved DAT bytes. Snapshots, provenance and historical
scan results survive; scan results are refreshed only by a new read-only scan.

T004 delivers native DAT/folder pickers, saved UI preferences and safe cache refresh.
Regression fixtures cover provenance/history retention, rollback/cancellation, stale
schemas/parser revisions, settings bounds/defaults, stable catalog identity and
concurrent migrations. Shutdown tests cover the A→B→A preference race: an outstanding
B save must finish before final A is saved, even if A equals the last acknowledged
value. A Windows junction fixture validates skipping the selected link and its root
without reading or changing the target's synthetic bytes.

Local verification uses the same restricted Linux environment and pinned toolchain
workarounds recorded in performance.md. Native Windows interactive GUI, OneDrive
hydration, scaling/accessibility and dialog focus/cancellation require platform smoke
tests; passing compilation or a junction fixture does not establish those results.
The review's Linux GNOME screenshots/runtime observations were not repeated here.

Source remains 0.0.1 unreleased. This work does not publish a testing binary or tag.

Final follow-up [CI run 38301720896](https://github.com/esotericode/Skyline/actions/runs/38301720896)
passes on Windows/Linux for source revision 13e19c2: 34 tests per platform, formatting,
Clippy with warnings denied, optimized desktop/CLI builds and version consistency.
Later commits update this validation record only.

## Second review — T004 follow-up (2026-10-12)

Reviewed 7bd74ba..ec72079 (store migration, settings, preferences flush, native pickers).
Fresh local checks pass: format, 34 tests, Clippy -D warnings, locked release build and
version output. End-to-end migration check: a database created by the original a14397c
CLI (schema 1, stale parse) was opened by this build; entries with `serial` attributes
moved from Unsupported to Exact (Exact 14 → 17), parser_revision became 1 and the
original import time/provenance were retained. In the Linux GUI (GNOME Wayland), a
search edit saved after the debounce and was restored on relaunch; the picker buttons
match the sidebar style. No defects requiring changes were found.

Notes recorded for later (owner asked to note, not fix, for now):

1. **Refresh failure locks the database.** `refresh_catalog_caches` runs inside
   `Store::open`; if any preserved snapshot fails to parse under a new parser revision,
   every open fails and the app cannot load, import or scan. There is no in-app way to
   remove a catalog. Not reachable today (revision 0 → 1 only relaxes parsing). Before
   any revision that tightens parsing, keep the previous derived entries (or mark that
   catalog stale/excluded) on failure instead of failing the open.
2. **Preference saves share the single job slot.** `Job::SaveSettings` uses the same
   receiver as import/scan, so while a debounced save runs the action buttons are
   disabled and the spinner shows. Locally the save finished within one frame, so no
   flicker was observed; on slower Windows storage (synchronous=FULL, WAL fsync,
   antivirus) it may last several frames and drop a click on Scan/Import. Consider a
   separate settings writer that never disables user actions.
3. **Native pickers are not interactively tested.** Dialog open/cancel/quit-while-open
   on Windows and on Linux xdg-portal still need a hands-on smoke test.

## Third review — multi-system follow-up (2026-10-12)

Reviewed 62ea323..e811730. Checks passed before changes (44 tests, fmt, Clippy,
release build; CI green). Scenario tests with the CLI found:

- **Fixed — stale import override after upgrade.** Preferences saved by the PR #2
  build store `"system":"Game Boy"` (the old required label). The new build treated it
  as an override, so every later import was silently labelled Game Boy. Settings now
  carry a revision; revision 0 clears `system` on load. Deliberate overrides persist.
- **Fixed — extensionless files.** One extensionless DAT entry made every extensionless
  file hashable (LICENSE, Makefile, .DS_Store, .git/HEAD were read). Now only files
  whose size matches an extensionless entry are read (any size if such an entry has
  no size).
- **Fixed at the owner's request — headered dumps.** A headered `.nes` file was Unknown
  against a headerless DAT with a generic message. Recognized headers (iNES, A78, LNX,
  SNES copier) now get header-free digests in the same read; they are compared only
  when the whole file has no candidate, and the result says so. See docs/formats.md.

Recorded for later (owner asked to note, not fix):

1. **Label differences split identities.** Matching treats (system label, game,
   filename) as identity. The same DAT, or two versions of it, imported under different
   labels (e.g. legacy "Game Boy" vs automatic "Nintendo - Game Boy") makes every match
   Ambiguous: 19 of 26 rows in the fixture. There is no in-app catalog removal or
   relabel; selecting a single catalog per scan is the only workaround. Needs an owner
   decision: catalog remove/relabel, and/or treating same-game candidates whose only
   difference is the label as one identity.
2. **Two controls for one value.** The import system override has a text box and a
   "Choose a system…" dropdown whose label never reflects the current override.
3. **N64 byte order.** `.v64` (16-bit swapped) and `.n64` (little-endian) files are
   accepted but not converted, so they are Unknown against big-endian `.z64` catalogs.
   The header mechanism does not cover this; it needs a streaming byte-order adapter.
