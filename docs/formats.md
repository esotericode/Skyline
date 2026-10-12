# Systems and format coverage

Skyline is a multi-system ROM manager. Game Boy/Game Boy Color were the first
verification fixtures, not the product's system boundary. The core compares local
bytes with imported reference evidence; it does not emulate systems or infer identity
from the extension. Catalog system labels are reference metadata, not authenticated
publisher statements.

## Available read-only workflow

Choose a folder containing UTF-8 Logiqx XML DATs, including nested subfolders.
Skyline considers all current catalog variants automatically. Each file keeps its DAT
header label; the CLI still accepts optional manual overrides for individual imports.
The collection view groups variant labels into system families and lists only systems
present in collection evidence or clearly labelled unmatched-file hints. Hints never
verify bytes. Filters narrow browsing without resolving ambiguous identities. See
[catalog folders](catalog-folders.md) for cache, errors and activation behavior.

Common loose extensions can be fingerprinted without a DAT (then unmatched bytes
are Unknown). Extensions declared by an active DAT are ROM-like too, so a new
system's own extension works without an update, except everyday document, media,
save and program types (txt, png, pdf, sav, exe, json, …) that PC and digital-store
DATs also list. Any other file, including extensionless ones, is read only when its
size equals an active catalog entry's size (or its name equals an unsized entry's).
With a large folder, small unrelated files can share an entry's size and be read;
that is cheap, and an unmatched file that is not ROM-like or starts with text is an
**Other file**, not Unknown. Other files are hidden unless the status filter selects
them, and never receive a system hint. Known archives/disc containers/packages remain
Unsupported even when listed in a DAT: their contents require dedicated adapters.

| Loose representation family | Common extensions checked |
| --- | --- |
| NES / Famicom | `.nes` |
| Super Nintendo | `.sfc`, `.smc`, `.swc`, `.fig` |
| Nintendo 64 byte layouts | `.n64`, `.z64`, `.v64` |
| Game Boy / Color / Advance | `.gb`, `.gbc`, `.gba` |
| Nintendo DS / Virtual Boy | `.nds`, `.vb` |
| Mega Drive / Genesis | `.md`, `.gen`, `.smd` |
| Master System / Game Gear / SG-1000 | `.sms`, `.gg`, `.sg` |
| PC Engine / SuperGrafx | `.pce`, `.sgx` |
| Atari cartridges | `.a26`, `.a52`, `.a78`, `.j64`, `.jag`, `.lnx` |
| Neo Geo Pocket / Color | `.ngp`, `.ngc` |
| WonderSwan / Color | `.ws`, `.wsc` |
| ColecoVision / Vectrex / Supervision | `.col`, `.cv`, `.vec`, `.sv` |
| MSX / generic loose bytes | `.rom`, `.bin` |
| Other cataloged loose representations | Extensions declared by active DAT entries |

These are whole-file checks, not format decoders or hardware validation, with one
read-only addition for copier/emulator headers. Reference sets such as No-Intro list
cartridge content without these headers; some loose dump formats carry them:

| Header | Recognized by | Excluded bytes |
| --- | --- | --- |
| iNES / NES 2.0 | `NES\x1A` at offset 0 | 16 |
| Atari 7800 (A78) | `ATARI7800` at offset 1 | 128 |
| Atari Lynx (LNX) | `LYNX\0` at offset 0 | 64 |
| Famicom Disk System | `FDS\x1A` at offset 0 | 16 |
| SNES copier | `.smc/.sfc/.swc/.fig` whose size is a 1 KiB multiple plus 512 | 512 |

The same single read produces whole-file digests and, when a header is recognized,
digests of the bytes after it. Whole-file matches take precedence, so catalogs that
list headered files behave as before. Only when the whole file has no candidate is
the header-free content compared; a match then says which header was excluded, and
the details show both digest sets. The file on disk is never changed. The No-Intro
`header` attribute is never a transform instruction. When it holds header bytes as
spaced hex (NES Headered, C64 CRT) those bytes are compared with the file: a
whole-file match confirms the header; content matched without a header is compared
with the same game's canonical header from a same-family catalog, reporting matching
bytes, the differing offsets, or a missing header with the canonical bytes. Free-text
values (serials, `!none`) are shown but not compared. N64 byte orders (.v64/.n64),
interleaved Sega files, decryption and padding removal are not handled: such files
must match the catalog's exact bytes, or they are Unknown, not a claim of corruption.
Every supplied supported digest and expected size is checked.

One byte fingerprint can have multiple system/release identities. Those remain
Ambiguous with all candidates visible, including when a system filter is active.
Each file is verified on its own; when its game lists several files (alternatives or
set parts) the explanation says set completeness is not checked. Unsupported
comparison requirements stay inspection-only.

## ZIP archives

Each `.zip` is opened read-only and every member is treated like a loose file: the same
ROM-like/size read rule, header handling, header checks and Other classification. A
member gets its own row; a one-ROM ZIP named after its ROM shows the archive's name,
otherwise `Archive.zip › member`. Nothing is extracted to disk. Stored, Deflate and
Deflate64 members are read; encrypted members, other compression methods and nested
archives are Unsupported, and damaged members (failed CRC, size mismatch) are read
errors without stopping the scan. A member is never decompressed past its declared
size; members over 16 GiB, or over 1 GiB claiming more than 1000× their stored size,
are refused as possible decompression bombs. Archives listing more than 65,536 members
are not inspected. A file named `.zip` that is not a ZIP is a read error, never hashed
as a loose ROM. Matching a ZIP file *as a whole* against DAT entries that list `.zip`
files stays inspection-only.

## Remaining adapters

| Capability | State |
| --- | --- |
| Streaming ZIP member verification | Implemented: members are hashed and matched like loose files, read-only and never extracted; stored, Deflate and Deflate64 methods; encrypted, other methods, nested archives and damaged members are reported per member |
| 7z/RAR and other archives | Adapter required |
| Disc sets, CUE/BIN relationships and multi-disc grouping | Adapter required; a loose `.bin` fingerprint does not establish disc-set completeness |
| CHD/RVZ and compressed disc containers | Adapter required |
| Installable/encrypted content packages | Adapter required |
| MAME software-list XML | Separate schema adapter required; current importer accepts Logiqx `<datafile>` only |
| Full arcade merged/split set rebuilding | Set/dependency adapter and operation planning required |
| Copy/move/rename and recovery | Planner and journal milestones; current verification preserves originals |

Tests use tiny synthetic bytes across all 37 common extension aliases, custom and
extensionless DAT entries, multiple catalog/system identities, CLI/database roundtrips,
raw representation differences and blocked containers. They establish byte-comparison
behavior, not compatibility with every real-world dump or native Windows GUI usability.
No commercial ROMs or third-party DAT bundles are committed. Provider downloads remain
manual and offline verification makes no network requests.

Validation: [main source 5553c4b](https://github.com/esotericode/Skyline/commit/5553c4bc779dddfb965f7621dfb125b06a1c90ba)
and [CI run 38312007058](https://github.com/esotericode/Skyline/actions/runs/38312007058):
49 tests on Windows and Linux, formatting, Clippy with warnings denied, locked optimized
desktop/CLI builds, shared-version checks and initial 0.0.1 testing artifact uploads.
The scan fixture covers 37 common aliases; header tests cover SNES aliases, whole-file
precedence and short reads across hashing-buffer boundaries. Prior PR histories are retained.

## Format and header advice

Fresh scans record recognized iNES/NES 2.0/A78/LNX/FDS signatures, probable SNES
copier headers or their absence in supported filename representations, and N64
byte-layout signatures. **Format & headers** explains those observations, the
explicit Headered/Headerless catalog expectation, and relevant loader considerations.
For NES, identifying headerless content does not recommend deleting the emulator
header. Declared trainer bytes are retained in the comparison. SNES copier headers
are distinct from internal cartridge metadata, which stays part of the ROM. Unknown
formats/old rows do not receive invented header-absence claims. FDS content matching
does not imply disk-structure or set completeness. No files or headers are rewritten.

Technical references checked 2026-10-12: [NESdev iNES](https://www.nesdev.org/wiki/INES),
[NES 2.0](https://www.nesdev.org/wiki/NES_2.0),
[FDS file format](https://www.nesdev.org/wiki/FDS_file_format),
[SNESdev ROM formats](https://snes.nesdev.org/wiki/ROM_file_formats),
[N64 ROM layouts](https://n64dev.org/romformats.html), and the
[No-Intro headered/headerless record](https://datomatic.nointro.org/index.php?n=2957&page=show_record&s=45).
Search-cached primary-source excerpts were used where direct loads returned 403/timeouts.
The actual No-Intro export provides header metadata alongside whole-file sizes/hashes.

Folder/header follow-up source [c9ba0ac](https://github.com/esotericode/Skyline/commit/c9ba0ac6ffb12beab635b3c6596697c89abdebae)
passes [CI 38318851217](https://github.com/esotericode/Skyline/actions/runs/38318851217):
66 Linux / 64 Windows tests, formatting, lint, optimized builds and version checks.
All 250 actual Fresh1G1R No-Intro catalogs plus the supplied variants were checked
locally; details and precise adapter boundaries are in [catalog folders](catalog-folders.md).
