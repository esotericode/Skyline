# Performance record

## Initial core baseline — 2026-10-12

Command: `cargo run --locked --release -p skyline-core --example baseline`.
Rust 1.99.0, thin-LTO release build, synthetic inputs, no provider access.
Environment: Ubuntu 24.04.3 / Linux 6.18.44, x86_64, glibc 2.39; nine logical CPUs
reported. CPU model, RAM/storage hardware and resource counters are not exposed
by this restricted development environment. **This is not the Windows reference
machine or a GUI responsiveness acceptance result.**

The local toolchain was installed under scratch. This environment lacks /proc/self/exe,
so rustc/Clippy were invoked with an explicit sysroot and BFD linker override;
rustfmt was invoked directly. Temporary directories were placed in the writable
workspace. Debug checks used two Cargo workers, no incremental compilation, no debug
symbols and one codegen unit to avoid environment-specific zero-length object files.
These are local environment accommodations, not repository build requirements.

The fixture generates 10,000/100,000 unique single-ROM catalog entries, measures one
parse/index build, 1,000 warm match lookups, and 100 warm queries using the same
query/sort function as the desktop. Rows are reverse ordered: 80% have exact
synthetic evidence, 20% are unknown. Queries alternate an empty query with
`Fixture 00`, sorting by filename with alternating direction. Timings exclude
fixture construction and rendering. SQLite save/load uses a new disposable local
database, one sample each; hash throughput reads 64 MiB from memory with all four
algorithms. Concurrent workload: desktop workspace build/lint checks, bounded to
two build workers. Results are a development observation, not a storage benchmark.

| Entries | DAT parse (ms) | Index build (ms) | Match p95 (µs) | Query/sort p95 (ms) | Save (ms) | Load (ms) |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| 10,000 | 11.08 | 2.57 | 0.37 | 1.00 | 41.28 | 32.88 |
| 100,000 | 116.82 | 37.10 | 0.71 | 21.38 | 460.23 | 436.23 |

Four-digest in-memory throughput: 307.80 MiB/s. DAT fixture sizes: 1,490,089 and
14,900,089 bytes. These measurements do not establish disk scan throughput, peak
memory, startup, idle CPU, frame latency or input/cancellation latency.

## Required Windows measurements

Choose and record one Windows 11 x64 machine: CPU, RAM, GPU, storage, OS/build,
display scaling and refresh rate. Repeat with Windows 10 22H2 if it remains a
supported technical target. Use the locked release build and record its version,
commit, enabled features, concurrent workload and warm/cold state.

- Record cold/warm startup, steady idle CPU and peak working set.
- Record p50/p95 input feedback, query time separately from rendering, frame time,
  and scroll behavior at 10,000/100,000 rows, including matched/ambiguous/error rows.
- Run a disposable loose-file scan while browsing and typing; measure cancellation
  feedback and completion at 64 KiB safe boundaries. Repeat during copies after M3.
- Measure disk-dependent hash/scan/import throughput and SQLite costs separately.
- Check resizing, 100/150/200% scaling, keyboard focus/row navigation and text labels.
- Compare with the proposed charter targets (100 ms feedback, 200 ms warm-query p95,
  16.7 ms frame budget at 60 Hz); explain any revision before changing requirements.

No GUI performance target or Linux desktop support is claimed by this baseline.

## T004 follow-up comparison — 2026-10-12

Re-ran the unchanged release harness against reviewed commit fe1f58d (isolated
worktree) and this follow-up. Same restricted Ubuntu/x86_64 environment, Rust 1.99.0,
thin LTO, BFD, two build workers, same 80% exact/20% unknown fixture and 100 warm
query/sort samples. The before run overlapped the desktop test build; the after run
overlapped lint/metadata checks. These single-run observations have uncontrolled
concurrent workload and do not establish GUI latency or a statistically significant
regression. External dependency artifacts were shared for the after run; fixture
construction and compilation are outside the measured intervals.

| Version | Entries | DAT parse (ms) | Match p95 (µs) | Query/sort p95 (ms) | SQLite save/load (ms) |
| --- | ---: | ---: | ---: | ---: | ---: |
| Reviewed fe1f58d | 10,000 | 11.90 | 0.40 | 1.93 | 42.59 / 33.45 |
| Follow-up | 10,000 | 11.52 | 0.45 | 2.36 | 46.41 / 37.92 |
| Reviewed fe1f58d | 100,000 | 119.97 | 0.48 | 20.18 | 488.00 / 350.95 |
| Follow-up | 100,000 | 111.25 | 0.45 | 17.04 | 488.72 / 370.78 |

Four-hash in-memory throughput: 310.34 before / 303.11 MiB/s after. The 10k query
observation increased by 0.43 ms (22%); 100k decreased by 3.14 ms (16%). Query logic
is unchanged in this slice; the mixed change is consistent with run/workload variation,
not enough evidence to attribute an effect to preferences. Controlled Windows repeats
are still required. Settings debounce, legacy-cache startup refresh, native dialog
opening/focus, parent-window close behavior, idle CPU and rendering costs are not
measured by the core harness and remain open.

A first after-run failed compiling bundled SQLite because its temporary assembly
file was truncated in this environment. Reusing the same successfully built locked
SQLite dependency from the before target allowed the full after harness to run.

## T016 multi-system comparison — 2026-10-12

Unchanged synthetic harness and fixture, reviewed 7992abe versus multi-system source.
Same restricted Ubuntu/x86_64/Rust 1.99.0 environment and flags as above. Optimized
before/after runs use thin LTO, BFD, two build workers and shared locked dependency
artifacts. The before run overlapped final local workspace tests; the after run had
no concurrent compiler. These single runs have different background load and do not
isolate the cost of an individual change or establish GUI acceptance.

| Source | Entries | DAT parse (ms) | Index build (ms) | Match p95 (µs) | Query/sort p95 (ms) | SQLite save/load (ms) |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Reviewed 7992abe | 10,000 | 11.72 | 2.73 | 0.43 | 2.34 | 41.83 / 37.56 |
| Multi-system | 10,000 | 11.06 | 3.12 | 0.49 | 2.20 | 43.56 / 33.24 |
| Reviewed 7992abe | 100,000 | 106.09 | 27.36 | 0.47 | 17.58 | 467.49 / 345.19 |
| Multi-system | 100,000 | 105.32 | 31.41 | 0.55 | 21.79 | 469.27 / 341.49 |

In this run the 100k index increased 4.05 ms (15%), match p95 0.08 µs (17%) and
query/sort p95 4.21 ms (24%); the 10k query decreased 0.14 ms. New index work derives
DAT extensions once, and matches exclude container entries. Search also checks system
labels when path/title do not match. These added checks preserve verification semantics;
controlled repeats on the Windows reference machine are still needed. Four-hash
in-memory throughput was 266.03 / 298.67 MiB/s, not a disk-scan measurement.

The first attempt to switch the shared release target between worktrees reused the
old binary. That sample was discarded; `cargo clean --release -p skyline-core`
forced recompilation of the changed core/example before the reported after sample.
No dependency or build-flag changes were made to produce the comparison.

An earlier debug-only comparison (unoptimized, no debug symbols, one codegen unit)
observed before/after index 13.58/16.76 ms at 10k and 129.74/160.00 ms at 100k;
query p95 33.11/27.90 ms and 209.34/207.59 ms. Avoiding repeated allocations of
already-known lowercase DAT extensions reduced the first multi-system debug index
observation from 173.21 to 160.00 ms at 100k. Debug numbers are not comparable to the
release baseline and do not satisfy a release or GUI latency budget.

The harness still measures filename queries and sort with no active system filter.
Matched-system filtering/search at representative catalog mixes, native GUI rendering,
startup/idle CPU/memory, disk scanning and interactive Windows behavior remain unmeasured.
