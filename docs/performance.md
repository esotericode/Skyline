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
