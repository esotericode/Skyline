# Systems and format coverage

Skyline is a multi-system ROM manager. Game Boy/Game Boy Color were the first
verification fixtures, not the product's system boundary. The core compares local
bytes with imported reference evidence; it does not emulate systems or infer identity
from the extension. Catalog system labels are reference metadata, not authenticated
publisher statements.

## Available read-only workflow

Import a UTF-8 Logiqx XML DAT for each desired system. The default system label is
the DAT header name; an optional explicit override accepts any name. GUI suggestions
are conveniences, not an allowlist. Select all catalogs and scan one folder containing
multiple systems. Matching rows show their system, search includes system names, and
the saved system filter narrows browsing without resolving ambiguous identities.

Common loose extensions can be fingerprinted without a DAT (then unmatched bytes
are Unknown). An active DAT can additionally enable other loose extensions, including
extensionless entries. Known archives/disc containers/packages remain Unsupported
even when listed in a DAT: their contents require dedicated adapters.

| Loose representation family | Common extensions checked |
| --- | --- |
| NES / Famicom | `.nes` |
| Super Nintendo | `.sfc`, `.smc` |
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

These are **raw whole-file checks**, not format decoders or hardware validation.
Headered/headerless NES/SNES/Atari files, N64 byte orders, or interleaved Sega files
must match the exact representation in the selected catalog. Skyline performs no
stripping, swapping, decryption, padding removal, repair or other byte transformation.
If a different representation has no exact reference, its result is Unknown, not a
claim of corruption. Every supplied supported digest and expected size is checked.

One byte fingerprint can have multiple system/release identities. Those remain
Ambiguous with all candidates visible, including when a system filter is active.
Multi-file sets and unsupported comparison requirements stay inspection-only.

## Remaining adapters

| Capability | State |
| --- | --- |
| Streaming ZIP member verification | Planned next archive milestone; ZIP containers are not hashed as ROM members |
| 7z/RAR and other archives | Adapter required |
| Disc sets, CUE/BIN relationships and multi-disc grouping | Adapter required; a loose `.bin` fingerprint does not establish disc-set completeness |
| CHD/RVZ and compressed disc containers | Adapter required |
| Installable/encrypted content packages | Adapter required |
| MAME software-list XML | Separate schema adapter required; current importer accepts Logiqx `<datafile>` only |
| Full arcade merged/split set rebuilding | Set/dependency adapter and operation planning required |
| Copy/move/rename and recovery | Planner and journal milestones; current verification preserves originals |

Tests use tiny synthetic bytes across all 35 common extension aliases, custom and
extensionless DAT entries, multiple catalog/system identities, CLI/database roundtrips,
raw representation differences and blocked containers. They establish byte-comparison
behavior, not compatibility with every real-world dump or native Windows GUI usability.
No commercial ROMs or third-party DAT bundles are committed. Provider downloads remain
manual and offline verification makes no network requests.

Validation: [source 62ea323](https://github.com/esotericode/Skyline/commit/62ea32341ac7a138e2745d6ea7b901ab1563f9f9)
and [CI run 38307149725](https://github.com/esotericode/Skyline/actions/runs/38307149725):
44 tests on Windows and Linux, formatting, Clippy with warnings denied, locked optimized
desktop/CLI builds and shared-version checks. The change is in [draft PR #3](https://github.com/esotericode/Skyline/pull/3).
