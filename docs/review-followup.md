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
