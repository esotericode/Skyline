# Changelog

## 0.0.1 — Unreleased

- Rust workspace, pinned toolchain, native GUI prototype and Windows/Linux CI.
- Local UTF-8 Logiqx DAT import with immutable catalog provenance.
- Recursive cancellable read-only loose-ROM hashing and deterministic matching.
- Exact, CRC-only, ambiguous, unknown, read-error and unsupported evidence, including
  contradictory digests and reference dump flags.
- SQLite catalogs/snapshots and last completed scan; native search/filter/sort,
  details, progress and About with a shared application version.

- Review fixes: DAT descriptive attributes such as `serial` and `mia` no longer mark
  entries Unsupported; Windows OneDrive/deduplicated files are no longer skipped as
  links; the collection is a sortable, status-colored table with keyboard navigation
  and responsive columns; readable sizes, dates and Windows paths in details.
- Commit-labelled Windows/Linux development builds from CI on `main`.
- Wayland sessions are supported natively; a failed start shows an error dialog on
  Windows instead of exiting silently.

- Native DAT/folder pickers run outside the render thread, with manual path entry
  available. Paths, exclusions, catalog choice, search/filter/sort preferences
  persist locally; choosing a path does not start an import or scan.
- Legacy parsed catalog caches refresh transactionally from immutable DAT snapshots,
  preserving import provenance and historical scans. Failed/cancelled refreshes keep
  existing data. Database schema 2 rejects older readers rather than downgrading.
- Debounced preferences save in background jobs and flush the newest edit on quit;
  an earlier save cannot overwrite the final choice. Windows junction skipping has
  a dedicated disposable-filesystem regression fixture.

Limits: Windows GUI and responsiveness baseline pending; no ZIP, organization,
copy/move/recovery or structured release metadata. Code license pending.
