# DAT folders

Choose one reference folder and one ROM folder. DAT selection starts background
indexing; **Scan and verify** refreshes the source before hashing the collection.
Nested `.dat`/`.xml` files are discovered case-insensitively. Every supported catalog
participates; exact byte evidence selects the applicable entry and representation.
Encrypted/decrypted or BigEndian/ByteSwapped catalog names require no user choice.
Skyline neither decrypts nor converts byte order. Packages/sets needing adapters
remain Unsupported even when their catalogs load successfully.

The main sidebar shows only system families present in collection results. Variants
group for browsing, while exact source labels, versions, snapshot hashes, quality
flags and ambiguous identities remain in file details. Nintendo 64DD stays separate
from Nintendo 64. For unmatched files, known extensions provide explicitly labelled
family hints so a missing relevant DAT can be mentioned; `.bin`/`.rom` do not imply a
system. Unique non-generic extensions declared by new catalogs supply hints automatically.
A hint cannot verify, relabel, or resolve a file. Unrecognized/custom catalog
labels are accepted, with parenthesized suffixes grouped for presentation only.

Import problems and the complete catalog list live under **Reference details**.
A bad unrelated DAT does not interrupt other catalogs or open a popup. Missing
catalog feedback concerns families suggested by the user's actual collection.
Read errors, unsupported representations and identity conflicts remain visible on
the affected file rows. Provider licensing/origin information stays inspectable.

## Activation and persistence

- Fingerprint source bytes each refresh; size/mtime alone never skips changed input.
  Cache hits avoid XML parsing and full catalog JSON decoding during intake.
- Identical copies share one immutable catalog snapshot. Each source path records
  its current working snapshot; folder iteration order does not pick a winner.
- A successful replacement activates the new snapshot. Removed files retire from
  future scans. Existing snapshots and historical scan results are never deleted.
- A failed read/parse keeps that path's previous working snapshot, if available;
  other valid files activate. Incomplete directory enumeration conservatively keeps
  unseen previous members instead of assuming they were removed.
- Cancellation, aggregate limits or storage failures roll back the entire refresh,
  including newly inserted snapshots. The last saved collection is unaffected.
- A previously indexed source that is offline supplies saved references, with a
  visible warning. A new unavailable source cannot silently borrow another folder.
- Desktop matching uses the selected source only. CLI scans use all current folder
  memberships plus deliberate single-DAT imports. Schema-1/2 catalogs migrate as
  manual imports; selecting a folder isolates legacy overrides automatically.
- Settings revision 2 saves the source path, removes the obsolete selected-DAT scan
  restriction, and migrates old collection filters to display families. Restoring
  preferences never triggers a filesystem scan or indexing job.

Discovery and reads skip symlinks/junctions, including a linked root. Other Windows
reparse points retain the existing regular-file handling. Final-component no-follow,
size/mtime checks and Unix identity checks protect DAT reads. As with the ROM scanner,
a stable parent tree is assumed; hardened descriptor-relative traversal is open.

## Bounds and validation

Per-DAT limits remain 64 MiB, one million ROM entries, XML depth 32 and 8 KiB fields.
One folder refresh permits 4,096 DAT/XML files, 256 MiB successfully read input,
two million distinct active entries, directory depth 64 and 100,000 filesystem
entries. Reports retain at most 256 problems plus an omitted count. These limits
bound prototype jobs and can be revised with measured larger libraries. Unsupported
schemas/encodings are recorded per file; there is no automatic download or DTD fetch.

Synthetic tests cover mixed GBA/N64/3DS sources, nested/duplicate copies, both N64
byte layouts, unknown hints, preserved ambiguity, updates/removals, broken input,
rollback/cancellation, offline sources, legacy overrides/settings, CLI, links and
resource bounds. Supplied third-party examples are read locally for validation;
neither DAT bundles nor commercial ROMs enter the repository. Commands:

```sh
cargo test --locked --workspace
cargo run --locked --release -p skyline-core --example folder_baseline -- /path/to/dats
```

Windows native picker/scaling/accessibility and actual GUI responsiveness remain
open acceptance checks. Compiling or passing core tests does not establish them.

## Complete Fresh1G1R compatibility target

Owner-selected source: [daily-virgin-dat/no-intro](https://github.com/UnluckyForSome/Fresh1G1R/tree/main/daily-virgin-dat/no-intro), checked at commit
`f54f00d9b9f7b4157e0fbdb80da5e94f6b29a5f2` on 2026-10-12. The checkout has 250
UTF-8 Logiqx DATs, 222,965,900 bytes and 971,723 ROM entries. All are accepted,
including three valid empty catalogs and 86 entries with an unspecified size.
The parser preserves descriptive `header` metadata (hex/text/`!none`) without
turning it into a byte-skipping instruction. This corrects the former blanket
Unsupported treatment, which prevented ordinary NES Headered entries verifying.
Parser revision 2 refreshes existing derived caches from immutable snapshots.

`tests/fixtures/fresh1g1r_system_names.json` is a names-only inventory, **not** a
runtime allowlist. A synthetic regression proves each listed label and a future
unknown system can import/scan through DAT-driven extensions. Full local DAT intake
and 247 populated-catalog metadata lookups were also exercised separately; three
empty catalogs have no entries to look up. Neither those checks nor the names test
claim every package/set/container adapter is implemented or that commercial dumps
were used. These remain explicit Unsupported cases on relevant collection rows.

New systems, variants and ordinary extensions require no application update. A new
XML schema, compression method, content transformation or set relationship can
still require a tested format adapter; metadata cannot implement a decoder.
