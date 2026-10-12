# Skyline ROM Manager: Project Plan and Agent Instructions

> Put this file at the repository root as `AGENTS.md`. Ask every coding agent to read it before making changes. It is the living project charter and the shared progress record. Rust, performance, and a minimalist, sleek GUI are owner requirements. Only choices explicitly labeled as proposed are provisional; the owner's instructions take precedence.

Plan created: 2026-10-12. Requirements last updated: 2026-10-12. Source policies reviewed: 2026-10-12.

## 1. Purpose

Build a desktop ROM manager that helps users understand, verify, rename, and organize their existing collections through a clear, attractive GUI.

Product name: **Skyline**, confirmed by the owner on 2026-10-12. Every published testing/general build must use a coherent incrementing version, starting at 0.0.1, and expose that version in the UI. The root Cargo workspace version is the single source; see `docs/versioning.md` for release rules. Source 0.0.1 is currently unreleased.

Deliver Windows support first, while keeping the core portable for a later Linux release. The program should work offline after reference catalogs have been imported. LLM agents may help develop the software; operating the application must not require an LLM or a paid cloud service.

Build the application entirely in Rust wherever feasible. Performance and a snappy, responsive GUI are major project goals. The visual direction is minimalist and sleek: lightweight, highly functional, readable, and polished, with restrained styling.

Preservation of user files and accurate identification are correctness requirements. Meet the Rust, responsiveness, organization, and visual goals within those requirements; performance work must preserve verification and recovery guarantees.

## 2. Goals and scope

### Owner requirements: Rust, performance, and visual direction

- **Rust throughout the application wherever feasible.** Implement the core, hashing, DAT parsing, matching, persistence integration, background jobs, file operations, and UI logic in Rust. Prefer a Rust-native desktop GUI. Supporting assets, configuration, declarative UI markup, packaging scripts, operating-system APIs, and necessary native dependencies are acceptable; they do not justify moving application logic to another language. Any unavoidable non-Rust runtime component must have a documented purpose, alternatives considered, minimal integration boundary, and impact on portability, packaging, performance, and maintenance. Do not quietly substitute another language or a web frontend for convenience.
- **Performance as a feature.** Startup, browsing, search, filtering, sorting, and command feedback should feel immediate. The GUI must remain usable during DAT imports, scans, hashing, and file operations. Keep memory use, idle CPU consumption, and distribution size proportionate to a lightweight desktop utility. Measure performance on representative collection sizes and record the reference hardware; do not claim responsiveness from language choice alone.
- **Minimalist, sleek GUI.** Use clear hierarchy, consistent spacing, readable typography, restrained colors, compact useful controls, and straightforward navigation. Favor useful collection information over decoration. Avoid distracting animation, heavy visual effects, and unnecessary panels. Minimalism must preserve discoverability, error details, keyboard access, and readable status indicators.

### Required capabilities

- Scan user-selected folders recursively, with exclusions, cancellation, and readable progress.
- Compute file sizes and hashes locally; compare them with trusted, versioned reference catalogs.
- Identify the system and cataloged release when the evidence supports it, including region, revision, language, and dump status when present.
- Show original names, catalog names, local paths, calculated and expected hashes, and the source of every match.
- Preview canonical names and folder layouts before applying changes.
- Create folders, copy files, and later move or rename files through a recoverable operation workflow.
- Show duplicates and unresolved files without automatically discarding them.
- Provide search, sorting, filters, readable verification indicators, and a game-detail panel.
- Persist the collection, source catalogs, settings, and operation history locally.

### Initial release boundary

Start with loose cartridge ROM files and ZIP archives, plus local imports of Logiqx-style XML DAT files. Use one or two cartridge systems with straightforward byte layouts to prove the full workflow. Game Boy and Game Boy Color are suggested starting points, subject to fixture validation.

Read ZIP members as decompressed streams. Do not rename members, repack archives, or alter ROM bytes in the first release. Offer organization of a ZIP only when its supported contents can be identified unambiguously; other archives remain available for read-only inspection.

Keep disc images, 7z/RAR, CHD/RVZ, header transformations, full MAME set rebuilding, and artwork downloads out of the initial release. Record unsupported formats visibly rather than silently treating them as verified.

### Later capabilities

- Provider-specific catalog updates when access rules and provenance are established.
- Disc sets with CUE/BIN tracks, sidecar files, and multi-disc grouping.
- Additional archive and disc-container formats through explicit format adapters.
- Region/language/revision preference profiles and optional curated subsets.
- Local cover-art selection, then separately reviewed metadata/artwork providers.
- Emulator frontend exports, playlists, and optional launch integration.
- Linux packaging and validation on named supported distributions.

ROM acquisition, sharing, emulation, byte repair, and automatic deletion of duplicates are outside the initial project scope. A metadata license does not license game binaries or artwork.

## 3. Reference catalogs and permitted access

A DAT is a catalog of expected names, sizes, hashes, and sometimes additional metadata. Import a catalog once, index it locally, and perform collection verification without per-ROM web requests.

The following source review is a starting point, not an assertion that every export or endpoint has identical rules. Check current terms again when implementing a downloader or redistributing a snapshot.

| Source | Useful coverage | Policy finding and implementation choice |
| --- | --- | --- |
| [No-Intro / DAT-o-MATIC](https://datomatic.no-intro.org/) | Cartridge and other cataloged software releases | Its [Data Usage License](https://datomatic.no-intro.org/terms.html), updated 2026-09-11, permits lawful reuse, modification, and redistribution, including commercial use, for rights it controls. Dataset-specific exceptions and third-party rights still apply. This does not establish permission for arbitrary automated downloads. Start with user-imported DATs. |
| [Redump-format disc catalogs](https://redump.info/downloads) | Optical-disc releases and tracks | The current [redump.info About page](https://redump.info/about) describes its metadata as public domain and provides XML DAT exports. This review did not establish an automated-access policy or confirm the relationship between current `.org` and `.info` hosting. Start with user-imported DATs; verify the exact provider and host before adding updates. |
| [MAME software lists](https://docs.mamedev.org/contributing/softlist.html) | Software/media for systems documented by MAME | XML lives in the MAME repository's `hash` directory. The reviewed [a5200.xml](https://github.com/mamedev/mame/blob/master/hash/a5200.xml) declares `CC0-1.0`; check each imported file's license. Treat MAME's software-list schema as a separate adapter. Arcade machine manifests and their set relationships require separate work. |
| [TOSEC](https://www.tosecdev.org/the-project) | Broad retrocomputing software and firmware catalogs | [DAT packs are published](https://www.tosecdev.org/downloads). A blanket reuse license and automation policy were not established in this review. Support local imports; resolve those questions before bundling data or enabling downloads. |

No-Intro's [navigation guide](https://wiki.no-intro.org/index.php?title=DAT-o-MATIC_Guide) describes system-specific and daily exports. The reviewed [download page](https://datomatic.no-intro.org/index.php?page=download&s=31) explicitly says its ClrMamePro WWW profile has no direct downloads available. Do not mistake a profile URL for a supported download API.

### Catalog provenance

Record provider, exact source URL or local-import origin, system, DAT name/version, retrieval/import time, format, license reference and review date, and SHA-256 of the imported catalog. Keep immutable snapshots so results can be reproduced. A catalog checksum identifies a snapshot; it does not authenticate an unknown publisher.

The GUI must show the source/version behind a result. Do not silently resolve conflicting catalogs by whichever one was imported last. Let users select a preferred catalog per system and inspect alternatives.

### Rules for future online updates

1. Prefer documented bulk exports or supported APIs over crawling individual game pages.
2. Establish endpoint permission and any limits before enabling an adapter. If unclear, request clarification from maintainers; keep manual import usable meanwhile.
3. Use HTTPS, caching, conditional requests where supported, a descriptive application user agent, and one active request per provider initially.
4. Offer explicit user-triggered updates first. If background updates are later enabled, make them opt-in. A proposed seven-day check interval is a conservative project default, not permission from a provider.
5. Respect documented limits, `Retry-After`, and access failures. Use bounded exponential backoff; do not retry indefinitely or bypass login, CAPTCHA, quotas, or other access restrictions.
6. Download only selected systems and reuse snapshots across scans. Never make a provider request merely because another local ROM was discovered.
7. Validate a new catalog before activating it. Retain the prior working snapshot if download or import fails.
8. Make scan/verification tests network-free. Test provider clients with fixtures and mocked responses rather than repeatedly hitting community servers.

Credit sources even where not required. Preserve applicable notices if data is redistributed. The application's eventual code license must be chosen separately from catalog and dependency licenses.

## 4. Verification semantics

Verification must be deterministic and explainable. Neither filename similarity nor an LLM's judgment is evidence that a dump is correct.

| Result | Meaning |
| --- | --- |
| Exact catalog match | Expected size, where supplied, and all supplied supported digests match the relevant byte representation. Show which algorithms were checked and the reference dump status. |
| CRC-only match | Size/CRC evidence matches, but there is no matching stronger reference digest. Show reduced confidence; do not label it a fully verified good dump. |
| Ambiguous identity | Matching bytes have multiple catalog identities. Preserve the candidates; do not invent a unique title or rename automatically. |
| Unknown | No suitable entry matches the active catalogs. This alone does not establish corruption. |
| Reference mismatch | The user selected an expected entry, and size or hashes differ. Explain the failed comparison; do not infer the cause. |
| Incomplete set | Some required members of a supported multi-file entry are absent or fail verification. |
| Read/archive error | Reading or decompression failed. Keep this separate from a catalog mismatch. |
| Unsupported | The representation, format, or required comparison algorithm is not supported. |

Store reference quality separately from comparison results: good, bad/suspect, undumped, or unspecified, as expressed by the catalog. A perfect match to a flagged bad dump must still display that flag. An undumped entry cannot verify a file. Preserve raw provider flags as well as normalized values.

Compute CRC32, MD5, SHA-1 as needed for catalog compatibility and SHA-256 for internal content fingerprints and operation receipts, preferably in one streaming pass. Legacy catalog digests are identification tools, not a security guarantee against adversarial input. Verify every supplied digest that the importer claims to support; contradictory digests must prevent a full match.

For archives, compare decompressed member bytes with ROM hashes. A ZIP container's hash is different from a member's hash. Track archive/member paths and container fingerprints separately.

ROM headers, padding, interleaving, encryption, and compressed disc containers affect the bytes being compared. Any later normalization must use a named, tested format policy and retain the original file fingerprint. Never silently strip headers or change bytes. Disc support must distinguish track hashes, container hashes, and logical-content hashes and preserve CUE relationships. CHD validation is deferred to a dedicated adapter.

Hash caching is an optimization: size and modification time alone cannot prove content is unchanged. Provide a force-rehash option and revalidate relevant content before applying a previously prepared operation plan.

## 5. File organization and recovery

Keep planning separate from execution. Every plan must list original and destination paths, action, reason, identity evidence, conflicts, expected byte counts, and disk-space requirements. Default to a read-only scan and a copy-to-new-folder workflow. Require a clear Apply action after preview; selecting move mode authorizes the source removal described in that plan.

Suggested templates include `{System}/{CanonicalFilename}` and, when reliable metadata exists, `{System}/{Region}/{CanonicalFilename}`. Preserve region, revision, language, and disc distinctions needed to avoid collisions. Keep display titles separate from filesystem names. Never overwrite a file to resolve a naming conflict.

The planner and executor must handle:

- Windows reserved names, invalid characters, trailing dots/spaces, path lengths, case-insensitive collisions, and names differing only by case.
- Unicode and normalization collisions without silently conflating releases.
- Source files already at their destinations, repeated runs, rename cycles, and source/destination overlap.
- Files changed after preview, missing files, locked files, permission failures, cancellation, and low disk space.
- Symlinks and Windows reparse points, which must not escape selected roots or cause recursive loops; skip following them by default.

A move across volumes is a copy, verify, then remove-source operation. Write to a temporary destination, verify content, finalize without replacing an existing destination, and only then remove the unchanged source. Same-volume renames must also use operations that refuse replacement and account for case-only names. Preflight checks alone cannot prevent races; execution must enforce these rules too.

Persist an operation journal before mutations and update it durably as each action proceeds. Record content fingerprints and enough state to reconcile an interrupted action after restart. Recheck uncertain filesystem state instead of blindly replaying an action. A database transaction cannot make filesystem changes atomic.

Provide recovery and guarded undo. Undo must stop for user-modified files, occupied original paths, or uncertain identity. Copy undo may remove only an unchanged file created by that operation. Recovery records are not a replacement for backups. Never automatically delete duplicates, unknown files, or the user's existing backups.

In v1, multi-file collections and archive contents must not be partially reorganized. Support them as read-only until grouping and recovery rules are implemented and tested.

## 6. Rust architecture, performance, and GUI

Required language: Rust. Use Cargo for the application build and dependency management, with an appropriate workspace if separate crates are useful. Choose and pin the Rust toolchain, a Rust-first desktop GUI toolkit, and supporting crates during M0. Evaluate the GUI choice with a small working collection-view prototype on Windows, using responsiveness, large-list behavior, memory use, keyboard/accessibility support, maintenance, licensing, and a credible Linux path as selection criteria. Rust is settled; the exact toolkit is not.

SQLite remains a proposed local storage choice, accessed from Rust. Native library dependencies are compatible with the owner's "as much Rust as feasibly possible" requirement when justified and kept behind small boundaries. Record dependency licenses and any non-Rust components. An embedded browser frontend is not the default architecture and would require a documented technical need and evaluation against the Rust and lightweight-GUI goals.

Keep hashing, parsing, matching, and operation planning independent of the GUI. Use a small internal CLI or test harness to exercise those services. Use platform abstractions for paths, safe file operations, and file identity; avoid introducing Windows-only dependencies into the core. Select dependencies after reviewing maintenance and licenses.

Suggested Rust modules or crates are Core (models and matching), Infrastructure (files, archives, persistence, provider adapters), Application (jobs and plans), Desktop (GUI and presentation state), and test/benchmark support. Create only the structure needed for the next milestone; module boundaries do not require separate crates immediately.

Model physical files, archive members, catalog entries, logical game/sets, match evidence, catalog snapshots, and operation receipts separately. Multiple paths can contain identical content; one digest can identify multiple releases. Do not force every file into exactly one game record.

### Responsiveness and resource use

Run scanning, hashing, DAT parsing, database work, and filesystem operations outside the GUI event/render thread. Use bounded workers, streaming I/O, cancellation, and batched progress messages. Tune parallelism for storage behavior rather than maximizing worker count. Make jobs cancel promptly at safe boundaries and keep enough journal state for recovery.

Render only visible collection rows and use indexed queries, pagination, or bounded view data as appropriate. Avoid loading ROM contents into memory, repeatedly parsing unchanged DATs, refreshing the entire collection for each progress event, or rehashing files merely to display or sort the table. Preserve force-rehash and operation preflight guarantees. Keep background queues bounded and limit temporary archive data. Update the UI when state changes; continuous idle repainting or decorative animation must not consume needless CPU.

Establish a performance baseline during M0/M1 and maintain reproducible measurements as the project grows. Initial engineering targets below are proposals to validate against a documented Windows reference machine, release build, and controlled fixture set; they are not measured results:

- Visible feedback to ordinary actions, including starting or cancelling a job: within 100 ms while background work is active. Finishing cancellation or cleanup may take longer and must show its progress.
- Local search/filter/sort results in a warm 10,000-entry fixture: p95 within 200 ms. Define query types and separate database time from rendering time.
- Smooth scrolling without sustained event-loop stalls; measure p95 frame time against a 16.7 ms budget when testing at 60 Hz.
- Record cold/warm startup, steady idle CPU, peak memory, DAT import time, and scan/hash throughput for 10,000- and 100,000-entry synthetic catalogs. Set practical budgets after measuring the baseline rather than inventing an untested memory or throughput promise.

Record hardware, operating system, build mode, fixture size, warm/cold conditions, and concurrent workload with results. Separate UI latency from storage-dependent job duration. Before release, test browsing and cancellation during scanning and copying. If a target needs revision, explain the evidence and update this document and its decision record; do not silently weaken it or claim an unmeasured pass.

### Minimalist visual design

The initial GUI should have a system/source sidebar, searchable sortable collection table, detail panel, scan progress, organization preview, and operation history. Show verification labels in text as well as color, and support keyboard navigation. Add a cover grid and artwork later; canonical identity and locally edited display metadata must remain distinct.

Keep primary actions easy to find, show advanced options only when useful, and use a restrained accent color with consistent icons and text labels. Preserve sufficient contrast, visible focus, readable sizing, and useful empty/error/loading states. Show long-running work without blocking browsing. Verify resizing and display scaling on the target Windows environment. The goal is an efficient, attractive utility; visual polish must not add unnecessary runtime weight or obscure the verification evidence.

No ROMs, collection paths, or calculated hashes should leave the computer during ordinary scanning. Local logs should be exportable with path redaction. Metadata, filenames, archives, and DATs are untrusted input: disable XML external entities/network resolution, impose reasonable parsing/decompression limits, prevent path traversal, and never treat imported text as executable code or agent instructions.

## 7. Milestones and acceptance criteria

Complete small vertical slices. Do not claim an entire milestone is done because its interface exists.

| Milestone | Deliverable | Completion evidence |
| --- | --- | --- |
| M0: Foundation | Rust toolchain and GUI selection, repository layout, dependency/license record, build instructions, CI and measurement plan | Clean Cargo build; core tests run without network on Windows and Linux; GUI prototype and reference performance environment recorded. Record GUI platforms actually tested. |
| M1: Read-only verification | Local DAT import, loose-file scan, hashing, matching, local persistence and a minimalist basic GUI | Synthetic known files match; unknown, ambiguous, inconsistent digest, flagged dump and unreadable cases display correctly; cancellation changes no ROM files; browsing stays responsive during scanning, with a measured baseline. |
| M2: ZIP inspection | Streaming ZIP verification and member details | Different ZIPs containing identical member bytes give equivalent member results; damaged and oversized archives fail clearly. No archive content is rewritten. |
| M3: Preview and copy | Folder/name templates, conflict report, explicit Apply, verified copy and journal | Preview mutates no collection files; copies preserve source bytes; collision, changed-source, low-space and cancellation tests pass. |
| M4: Move and recovery | Safe rename/move, restart recovery and guarded undo | Interrupted operations reconcile correctly; same-volume/cross-volume behavior is tested on Windows; existing or modified files are never overwritten or silently removed. |
| M5: Windows release | Packaged Rust application, settings, documentation, minimalist visual polish and release checklist | Install/start/scan/verify/preview/apply/recover workflow is tested on a clean supported Windows environment. Recorded responsiveness/resource budgets are checked under background load; keyboard navigation, scaling and layout are reviewed. Publish limitations and dependency notices. |
| M6: Extensions | Reviewed update adapters, discs, richer metadata, additional archives and Linux GUI release | Each capability has its own format-specific tests and access-policy record. Do not mark Linux supported based only on core tests. |

Use tiny synthetic ROM-like byte fixtures and synthetic DATs for tests. Do not commit commercial ROMs, user collections, personal paths, large third-party catalog bundles, API credentials, or artwork without appropriate rights. Tests involving file mutations must use disposable temporary directories. Failure injection and recovery tests are especially important before moving real files.

## 8. Instructions for LLM development agents

### At the start of every task

1. Read this file and any more specific repository instructions. Inspect the actual tree, current branch, worktree changes, build configuration, and relevant code before assuming anything is implemented.
2. Read the task ledger and handoff below. Select one unblocked task or follow the owner's explicit priority. Use a stable task ID; record ownership if multiple agents are already working.
3. State a short implementation plan and the evidence that will establish completion. Resolve routine implementation details using this charter; raise material scope or design questions with a concrete proposal.
4. Preserve unrelated changes. Keep edits focused and avoid unrequested dependency upgrades or broad rewrites.
5. Treat the Rust implementation, performance, and minimalist GUI requirements as acceptance criteria for relevant work. Keep unresolved toolkit choices separate from the owner's settled language requirement.

### During implementation

- Use deterministic code for verification and file operations. Do not add runtime AI dependency for naming, matching, or deciding whether files are safe to remove.
- Keep provider network calls outside the scanner and behind disabled-by-default reviewed adapters.
- Do not run mutation experiments on a real user collection. Use disposable fixtures and dry-run outputs.
- Add targeted tests for new behavior and data-loss risks. Use mocks for network access. Run relevant checks and report failures accurately.
- Document important architecture choices and verified source rules. Distinguish requirements, proposals, implemented behavior, and observed test results.
- Keep the goals and scope current when the owner adds or changes requirements. Update affected milestones, tasks, decision records, and handoffs in the same change. Preserve a dated record of what changed and why; label superseded choices so later agents do not restore them as current requirements.
- For changes to UI, scanning, matching, imports, persistence, or job scheduling that can materially affect performance, compare relevant measurements with the recorded baseline and report regressions or missing measurements. Optimize demonstrated bottlenecks without weakening file correctness.
- If work is blocked, explain the dependency and leave a small actionable next step. Never invent passing tests, source permissions, or feature completion.

### Before ending a task, including partial or interrupted work

Updating this document is part of finishing a task, not optional follow-up work. Update the goals/decisions if they changed, the task ledger, current-state record, dated work history, and handoff in the same change as the work. Include changed paths, behavior delivered, remaining work, commands run with outcomes, platform tested, and relevant commit/PR or issue references when they exist. If a commit has not yet been created, say `pending commit`; do not invent its hash.

Keep these questions answerable for the next agent: What are the current goals? What works now? What was completed previously? What remains unfinished or blocked? What should happen next? Preserve existing work-history entries and append a dated entry for each substantive task, including partial work and corrections. A current handoff must not erase earlier completion evidence.

Mark a task `done` only when its acceptance criteria are met and the relevant implementation and tests can be found. Partial work stays `in_progress`, with remaining criteria listed. A failed or skipped required check must be recorded. Documentation updates alone do not establish implementation completion.

Allowed statuses: `not_started`, `in_progress`, `blocked`, `done`, `deferred`. Keep the ledger concise; retain durable decisions and completed-task evidence. If it grows substantially, move detailed history to a linked progress file while keeping this file's current goals, state, tasks, and recent history synchronized. Never discard history simply to shorten the document. GitHub issues may mirror tasks, but must not contradict this record.

## 9. Live progress record — agents must maintain

Last updated: 2026-10-12.

### Current state

- Completed: charter/roadmap and initial Rust read-only source slice; product name and version policy confirmed. Local core checks are recorded below; milestones remain partial.
- Implemented: UTF-8 Logiqx DAT import with immutable snapshot provenance; recursive loose-file streaming CRC32/MD5/SHA-1/SHA-256; deterministic exact/CRC-only/ambiguous/unknown/read-error/unsupported evidence; cancellation/exclusions/link skipping; SQLite catalogs and last completed scan; CLI and native collection-view prototype with search/filter/sort, source details, duplicates, progress and visible version.
- Current milestones: M0 and M1 partially implemented; neither complete. No ZIP or file mutation features exist.
- Current focus: validate Windows CI and native GUI; measure input/frame/startup/resource behavior; add native dialogs and persisted UI settings, then finish the read-only slice before ZIP.
- Stack: Rust 1.99.0; two-crate Cargo workspace; egui/eframe 0.33.3 native prototype (provisional until Windows evaluation); rusqlite 0.37.0 with bundled SQLite. Direct pins and Cargo.lock committed. Code license pending owner selection; Windows 11 x64 and Windows 10 22H2 x64 are proposed test targets, not validated support.
- Performance: reproducible 10,000/100,000-entry synthetic core harness and initial release-build Linux measurements in `docs/performance.md`. Query/sort p95 1.00/21.38 ms in that run. Hardware model/RAM counters unavailable; Windows reference machine and all GUI/resource budgets remain unmeasured.
- Visual direction: minimalist, sleek, lightweight, highly functional GUI with clear information and restrained styling.
- Checks: local workspace check/build and 20 synthetic integration tests pass on Linux; direct rustfmt check passes. Clippy via the direct driver with -D warnings passes for all workspace targets. GitHub Windows/Linux CI started for implementation cdc6192; format, workspace tests and Clippy pass on both runners; optimized release builds are in progress. Native GUI validation remains pending.
- Windows GUI, filesystem executor, and Linux GUI runtime validation: not started. Linux desktop compilation does not establish supported desktop operation.
- Automated provider access: not enabled; endpoint permission and policy review pending.

### Task ledger

| ID | Task | Status | Dependencies | Evidence / remaining work |
| --- | --- | --- | --- | --- |
| DOC-001 | Establish charter, roadmap, source review and agent handoff | done | None | This document; policy review dated 2026-10-12. |
| DOC-002 | Incorporate owner's Rust, performance and GUI requirements; strengthen ongoing documentation | done | DOC-001 | Sections 1, 2, 6–9 revised; work history added; implementation status remains unstarted. |
| T001 | Pin Rust toolchain and GUI toolkit, target Windows versions and code license; scaffold Cargo build and CI | in_progress | None | Rust 1.99.0, native egui prototype, workspace/lockfile, build instructions, architecture/dependency records and Windows/Linux CI added. Local Linux build passes. Remaining: Windows toolkit evaluation, actual supported-version decision, owner code license and reference GUI measurements. |
| T002 | Implement safe local Logiqx XML DAT import and snapshot provenance | in_progress | T001 | Core importer and immutable SQLite snapshots implemented; bounded UTF-8 XML, inert external DOCTYPE, prohibited internal entities, invalid digest/schema and snapshot tests pass. Remains partial until foundation/Windows checks complete. |
| T003 | Implement cancellable loose-file scan, streaming hashes and deterministic match model | in_progress | T001, T002 | Read-only recursive/excluded/cancelled scans; four streaming hashes; indexed matching; flags/conflicts/unsupported sets; changed-source/link tests and CLI workflow pass locally. Remaining: Windows reparse/filesystem validation, readable-error GUI review and hardening against parent-path swaps. |
| T004 | Persist catalog and collection; expose read-only verification in minimalist Rust GUI | in_progress | T003 | Background DAT/scan/DB worker, bounded progress, SQLite completed-scan retention, visible rows/details/search/filter/sort/source selection and version. Remaining: native Windows end-to-end/latency tests, settings/preferences persistence, dialogs, structured metadata and expected-entry mismatch workflow. |
| T005 | Implement streaming ZIP inspection and archive/member model | not_started | T003, T004 | Member hashes, errors and resource limits; no repacking. |
| T006 | Implement organization planner and conflict preview | not_started | T004, T005 | Deterministic proposals, Windows edge cases, zero scan/preview mutations. |
| T007 | Implement verified copy executor and durable journal | not_started | T006 | Copy, failure/cancellation and restart tests; originals preserved. |
| T008 | Implement safe move/rename, recovery and guarded undo | not_started | T007 | Failure injection; real Windows same-/cross-volume checks. |
| T009 | Package and validate first Windows release | not_started | T008, T013, T014 | Clean environment smoke test; performance results, limitations and dependency notices. |
| T010 | Verify provider hosting/access policies and add opt-in catalog updates | deferred | T009 | Exact endpoints, license/access record, cache/backoff tests. |
| T011 | Add disc sets, sidecars and additional format adapters | deferred | T009 | New scoped task design and format-specific acceptance criteria. |
| T012 | Validate and package Linux desktop GUI | deferred | T009 | Named distributions; filesystem and GUI checks recorded. |
| T013 | Establish representative performance fixtures, measurements and regression checks | in_progress | T001, T004 | Synthetic release harness measures DAT/index/match, actual shared view query/sort, SQLite save/load and in-memory hash throughput. Initial Linux numbers recorded. Remaining: named Windows hardware, startup/input/frame/idle/memory/disk metrics and background-load browsing/cancellation. |
| T014 | Refine and review minimalist, sleek GUI and accessibility | not_started | T004 | Consistent visual style; usable keyboard navigation, scaling, resize behavior and empty/error/loading states. |
| T015 | Establish Skyline product name, shared version and release increment rules | done | Owner instruction | Root workspace 0.0.1 inherited by crates; header/About/title/CLI expose VERSION; CLI version tested; CI consistency check and docs/versioning.md. No release/tag published. |

### Decision record

| ID | Decision | State | Rationale |
| --- | --- | --- | --- |
| D001 | Windows first, Linux later | Owner requirement | Establish a useful Windows desktop app while retaining portable core services. |
| D002 | GUI browsing, hash checks, naming and folder/file organization | Owner requirement | Core requested user workflow. |
| D003 | Offline local DAT imports before online adapters | Owner-approved initial approach | Owner is comfortable importing DATs manually; useful verification without unresolved endpoint permissions or per-ROM traffic. |
| D004 | Rust throughout the application wherever feasible | Owner requirement | Major project goal; replaces the earlier proposed stack. Rust GUI toolkit and supporting dependencies remain to be selected. |
| D005 | Preview first, copy mode first, journal before move support | Recommended default | Make file operations inspectable and recoverable. |
| D006 | Loose cartridges and ZIP before disc/arcade reconstruction | Recommended default | Prove matching and preservation before dependent multi-file formats. |
| D007 | Performance and a snappy, responsive UI | Owner requirement | Measure user-visible latency and resource use; keep the GUI usable during background work. |
| D008 | Minimalist, sleek, lightweight and functional visual design | Owner requirement | Favor clear information and restrained polish while preserving usability and accessibility. |
| D009 | SQLite local persistence through Rust | Superseded by D012 | Original proposal; concrete rusqlite/bundled SQLite integration now implemented and recorded in D012. |
| D010 | Skyline name and incrementing release versions, visible in UI | Owner requirement (2026-10-12) | workspace.package.version is authoritative; initial source 0.0.1; every published testing/general release gets a new version. |
| D011 | Rust 1.99.0, egui/eframe 0.33.3 native prototype, two crates | Implemented technical default; GUI provisional | Pin builds, retain portable core, virtualize visible rows, no browser runtime. Windows evaluation and GUI/resource measurements remain open; rationale in docs/architecture.md. |
| D012 | Implement D009 with rusqlite/bundled SQLite | Implemented technical default; supersedes proposed storage state | Transactions and immutable snapshots; native C boundary isolated in store.rs. Dependency declarations/inventory in docs/dependencies.md. Application license remains pending. |

### Dated work history

Append entries here after substantive work. Preserve prior entries and identify corrections explicitly. Record completion only for work actually delivered.

| Date | Task | Status | Delivered / evidence | Remaining work |
| --- | --- | --- | --- | --- |
| 2026-10-12 | DOC-001 | done | Initial goals, source review, verification and file-operation rules, roadmap and agent workflow documented in AGENTS.md. | Application implementation and validation not started. |
| 2026-10-12 | DOC-002 | done | Updated AGENTS.md to require Rust wherever feasible, make performance and minimalist GUI major goals, add proposed measurable targets, revise roadmap/decisions, and require goals/progress/history updates after each task. | Select Rust toolkit/toolchain, implement application, and measure behavior; documentation changes provide no runtime validation. |
| 2026-10-12 | T001–T004, T013, T015 | in_progress; T015 done | First Rust read-only source slice: core/CLI/native desktop, immutable DAT snapshots, hashes/matching/scan/storage, version UI/policy, CI and dependency/architecture/performance records. Local workspace build and 20 network-free tests pass; synthetic core measurements recorded. Implementation commit cdc619283f6798bca2f055c120296ca7eb28f474; [draft PR #1](https://github.com/esotericode/Skyline/pull/1). | M0/M1 incomplete: Windows GUI/CI evidence, license/platform choices, settings/dialogs and full performance/accessibility checks. ZIP and all file mutations await later milestones. |

### Previous handoff — DOC-002 (preserved)

- Task: DOC-002.
- Delivered: Rust as an owner requirement; explicit performance/resource and minimalist GUI goals; proposed responsiveness targets; updated architecture, milestones, tasks and decisions; mandatory maintenance of goals, current status, remaining work and dated history.
- Changed paths: AGENTS.md.
- Code/tests delivered: none; no application build or runtime claims.
- Documentation validation: Python consistency check passed for owner requirements, removal of the previous stack, progress/history instructions, task IDs, Markdown table structure and unchanged unstarted implementation status. No application tests or performance measurements were run.
- Open decisions: product name; Rust toolchain version and Rust-first GUI toolkit; local database choice and supporting crates; minimum Windows version; code license; initial systems; installer versus portable distribution; reference performance environment and validated budgets.
- Blockers: none for local read-only development. Provider access questions block automated integrations only.
- Next action: start T001 with a Rust GUI prototype and measurement plan, record reasonable technical defaults, then build T002–T004 as a small read-only slice using synthetic fixtures. Language selection is already settled in favor of Rust.
- Suggested next-agent prompt: `Read AGENTS.md, inspect this repository, and begin the next unblocked task. Follow the Rust, performance and minimalist GUI requirements. Implement a small complete slice, run relevant checks, and update the current goals, task ledger, state, dated work history and handoff before finishing.`

### Latest handoff — 2026-10-12

- Task: T001–T004 / T013 partial; T015 done. Owner: current Codex session; no parallel agents.
- Delivered: Skyline 0.0.1 source foundation and first read-only verification slice; details in current state, README, CHANGELOG and docs/architecture.md.
- Changed paths: Cargo.toml/Cargo.lock/rust-toolchain.toml/.gitignore; crates/skyline-core (models, DAT/hash/matching/scan/store/view, CLI, integration tests, baseline example); crates/skyline-desktop/src/main.rs and manifest; .github/workflows/ci.yml; README.md, CHANGELOG.md, docs/*, AGENTS.md.
- Checks: workspace check/build pass locally; 20 tests pass, including CLI import/scan/version; direct rustfmt check passes. Clippy via the direct driver with -D warnings passes for all workspace targets; GitHub CI format, workspace tests and Clippy pass on Windows and Linux; optimized release builds are in progress as of this handoff. Tests: 20 on Linux and 19 on Windows (Unix-only symlink fixture excluded). Native GUI runtime remains untested. Platform: restricted Ubuntu 24.04.3 x86_64, Rust 1.99.0; local explicit sysroot/BFD/TMPDIR/low-parallelism workarounds recorded in docs/performance.md. Initial failures from absent /proc, unwritable /tmp and incremental object generation were environmental; core tests pass with writable temp paths and nonincremental builds. No native GUI runtime/platform support claim.
- Performance: release harness with 10k/100k mixed synthetic rows; actual shared query/sort p95 1.00/21.38 ms; DAT parse 11.08/116.82 ms; all conditions and missing GUI/resource measurements recorded in docs/performance.md. No prior application baseline existed.
- References: implementation commit [cdc619283f6798bca2f055c120296ca7eb28f474](https://github.com/esotericode/Skyline/commit/cdc619283f6798bca2f055c120296ca7eb28f474); branch feat/skyline-read-only-foundation; [draft PR #1](https://github.com/esotericode/Skyline/pull/1); [initial CI run](https://github.com/esotericode/Skyline/actions/runs/38291579968) in progress. The documentation handoff follows the implementation in the same PR. No release/tag or distributable build published.
- Open work: Windows checks/native GUI evaluation; named reference hardware; persisted UI settings/preferences, native dialogs, structured metadata and selected-expectation mismatches; code license. Filesystem executor, ZIP and recovery are not implemented. Current scanner assumes a stable parent directory tree; final-component no-follow and changed-source checks exist, but hardened parent traversal remains open.
- Next action: validate this slice on Windows with synthetic inputs; record actual toolkit/accessibility/scaling/latency results and finish T001/T004/T013 criteria, then advance to T005. Do not begin mutating real collections before planner/journal/executor milestones.


- Handoff publication: the tested local tree b15e3cbf85ee437a33d8d77b102b4323ed9d5d2a exactly matched the GitHub tree before the implementation ref was advanced. Command-line Git lacked write credentials, so the connected GitHub app created the identical tree/commit and draft PR. Local source was synchronized to that implementation commit; the original local verification commit is retained on local/verified-foundation.
