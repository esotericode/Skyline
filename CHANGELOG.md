# Changelog

## 0.0.2 — Unreleased

- One recursive DAT-folder source replaces the per-file/system picker. Choosing the
  folder indexes it; scans refresh it automatically and consider all active variants.
- Collection-only system families, automatic variant matching and relevant missing
  catalog hints; exact identities/ambiguities and full evidence remain in file details.
- Unchanged fingerprints reuse parsed caches; duplicate files deduplicate; broken
  replacements retain working references. Batch cancellation and storage/limit errors
  roll back; removed/updated files retire from active scans while snapshots stay stored.
- Schema 3 adds folder membership and compact automatic-cache metadata. Existing
  manual imports and historical evidence survive; settings revision 2 saves the DAT
  folder and retires the obsolete single-catalog selection.
- Full 250-DAT Fresh1G1R No-Intro compatibility check; valid empty catalogs and
  unspecified sizes; descriptive No-Intro header metadata no longer blocks NES.
- Format/header observations, headered/headerless catalog expectations, FDS header
  comparison and N64 signature notes; no payload/header rewriting.
- Catalog entries live in indexed database rows and are looked up per scanned file
  (schema 4, parser 3): with all 250 Fresh1G1R No-Intro DATs the desktop idles at
  ~93 MB instead of ~700 MB and a scan peaks at ~17 MB instead of ~1 GB.
- Only ROM-like files are read, plus other files whose size matches a catalog entry.
  Readmes, artwork, saves and tools are **Other files**, hidden by default, never
  hinted to a system; text content with a ROM-like extension (`notes.md`) is Other.
- NES/C64 header metadata is compared with the file: matching, differing offsets, or a
  missing header with No-Intro's canonical bytes.
- Each file in a multi-file game entry is verified on its own; set completeness noted.
- Superseded DAT versions no longer used by a folder, manual import or saved scan are
  pruned; the database is compacted. Folder limits raised to 2 GiB / 10M entries.
- A catalog that cannot be re-derived keeps its entries and warns instead of blocking
  the database from opening. CLI `scan --dats <folder>` scans one DAT folder only.
- ROMs inside ZIP files are verified read-only, one row per member, with the same
  header handling; damaged, encrypted and unusual members are reported per member and
  decompression bombs are refused. Adds `zip` 8.6.0 (MIT; Deflate via pure-Rust
  zlib-rs, Deflate64).
- Right-click a collection row to show the file in the file manager (Explorer selects
  it on Windows; the freedesktop FileManager1 interface on Linux, falling back to
  opening the folder) or to copy its path; the details panel has the same button.
- CLI `import-folder`, targeted synthetic regressions and local benchmarks.
- Shared UI/CLI version advances to 0.0.2 for the next different testing distribution.

## Previous multi-system follow-up (included in 0.0.1 testing builds)

- Broad loose cartridge verification, using common extensions plus those declared
  by active DATs; custom and extensionless files no longer need system-specific code.
- DAT header system labels by default, unrestricted custom overrides and broad GUI
  suggestions; mixed-system collection search, saved system filter and system labels.
- Known archives/disc containers/packages stay Unsupported even when declared by a
  DAT. Raw representations must match exact reference bytes; no normalization.

## 0.0.1 — Development testing builds (2026-10-12)

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
