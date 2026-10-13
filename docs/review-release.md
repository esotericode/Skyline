# ZIP/reveal review and Windows testing release — 2026-10-13

Reviewed feat/skyline-zip at 097518d04818d5afff9ba75c68e5a8f71920b265, including the
DAT-folder follow-up 8aebfd3/6b60c9d, ZIP 5490862/74f85a6 and file-manager
cb55a65/version 097518d0. Concur with promotion: catalog lookups use indexed SQLite
rows and preserve digest contradictions; archive members use decompressed bytes,
CRC/size checks, bounded reads, cancellation and the existing header pipeline; no
archive contents are extracted or rewritten. File-manager launches pass paths as
arguments, including Explorer's select form, and ZIP actions target their container.

CI 38329065501 passes for that exact built source: 74 Windows and 79 Linux tests,
formatting, Clippy with warnings denied, locked optimized builds and shared-version
checks. Seven archive cases cover equivalent member verification, header content,
CRC/declared-size errors, unsupported encryption/compression/nesting, cancellation,
oversize refusal and unchanged archive bytes. Three additional Linux file-manager
unit tests cover URI escaping, fallback arguments and absent paths. This review did
not repeat Clade's interactive Linux tests or run native Windows GUI smoke checks.
The local execution environment is unavailable; verification uses exact repository
snapshots and the full GitHub Actions logs.

The Windows artifact 11781278519 already distributed version 0.0.3 from 097518d0.
The release publisher promotes those exact EXEs, verifies original artifact/package
checksums, corrects stale ZIP/version text in package documentation, and includes
build provenance and new package checksums. No rebuilt binaries are distributed
under 0.0.3. Main CI package uploads are manual-only to avoid accidental version reuse.

Remaining T005 work: unknown ZIP members currently use the container extension for
collection-family hints; they should use the member name. Whole-container fingerprints
and stronger member identity for duplicate names are needed before organization.
Archive directory parsing occurs before the member-count limit; hard bounds on that
metadata work and aggregate decompression budgets merit follow-up. These limitations
do not authorize archive mutation. File-manager startup failures after worker launch
are not surfaced to the GUI, and the initial existence check still runs on the UI
thread; verify behavior and latency on native Windows before declaring acceptance.

M0/M1/M5 and T001/T004/T009/T013/T014/T017 remain partial. Selected-entry mismatch,
structured metadata/preferences, native accessibility/scaling/resource measures and
planner/copy/move/journal/recovery remain unfinished. Code-license selection and full
notice review remain open for general release; the owner explicitly requested this
limited testing prerelease.
