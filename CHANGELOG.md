# Changelog

## Unreleased multi-system follow-up

- Broad loose cartridge verification, using common extensions plus those declared
  by active DATs; custom and extensionless files no longer need system-specific code.
- DAT header system labels by default, unrestricted custom overrides and broad GUI
  suggestions; mixed-system collection search, saved system filter and system labels.
- Known archives/disc containers/packages stay Unsupported even when declared by a
  DAT. Raw representations must match exact reference bytes; no normalization.

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
- Headered NES/Atari 7800/Lynx/SNES copier dumps match headerless catalogs: the
  header is excluded for a second comparison only; files are never changed.
- Preferences saved before the system-label change no longer pre-fill "Game Boy" as
  an import override; extensionless files are read only at catalog entry sizes.
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
