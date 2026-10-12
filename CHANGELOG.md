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
- Wayland sessions are supported natively; a failed start shows an error dialog on
  Windows instead of exiting silently.

Limits: Windows GUI and responsiveness baseline pending; no ZIP, organization,
copy/move/recovery, native dialogs or persisted UI settings. Code license pending.
