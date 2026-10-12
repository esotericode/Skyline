use crate::model::{Catalog, FileFormat};
use std::{borrow::Cow, collections::BTreeSet, path::Path};

/// Import labels are suggestions, not a restriction on supported systems. An empty
/// override uses the DAT header; a custom label is accepted by the same importer.
pub const SYSTEM_PRESETS: &[&str] = &[
    "Nintendo - Nintendo Entertainment System",
    "Nintendo - Super Nintendo Entertainment System",
    "Nintendo - Nintendo 64",
    "Nintendo - Game Boy",
    "Nintendo - Game Boy Color",
    "Nintendo - Game Boy Advance",
    "Nintendo - Nintendo DS",
    "Nintendo - Virtual Boy",
    "Sega - Mega Drive - Genesis",
    "Sega - Master System - Mark III",
    "Sega - Game Gear",
    "Sega - SG-1000",
    "NEC - PC Engine - TurboGrafx-16",
    "NEC - PC Engine SuperGrafx",
    "Atari - 2600",
    "Atari - 5200",
    "Atari - 7800",
    "Atari - Jaguar",
    "Atari - Lynx",
    "SNK - NeoGeo Pocket",
    "SNK - NeoGeo Pocket Color",
    "Bandai - WonderSwan",
    "Bandai - WonderSwan Color",
    "Coleco - ColecoVision",
    "GCE - Vectrex",
    "Microsoft - MSX",
    "Microsoft - MSX2",
];

/// Common loose representations can be fingerprinted even without an imported
/// catalog. Extensions only decide what to read; identity comes from byte evidence.
pub const LOOSE_EXTENSIONS: &[&str] = &[
    "gb", "gbc", "gba", "nes", "sfc", "smc", "swc", "fig", "n64", "z64", "v64", "nds", "vb", "md",
    "gen", "smd", "sms", "gg", "sg", "pce", "sgx", "a26", "a52", "a78", "j64", "jag", "lnx", "ngp",
    "ngc", "ws", "wsc", "col", "cv", "vec", "sv", "rom", "bin",
];

fn extension(path: &Path) -> Cow<'_, str> {
    let extension = path.extension().unwrap_or_default().to_string_lossy();
    if extension.bytes().any(|byte| byte.is_ascii_uppercase()) {
        Cow::Owned(extension.to_ascii_lowercase())
    } else {
        extension
    }
}

/// A catalog cannot turn a container/sidecar into a loose ROM. These need adapters
/// that distinguish container, member, track, and logical-content hashes.
pub fn unsupported_representation(path: &Path) -> Option<&'static str> {
    unsupported_extension(&extension(path))
}

fn unsupported_extension(extension: &str) -> Option<&'static str> {
    match extension {
        "zip" | "7z" | "rar" | "gz" | "bz2" | "xz" | "zst" | "tar" | "tgz" => Some(
            "Archive inspection needs a format adapter; container bytes are not ROM member bytes.",
        ),
        "iso" | "cue" | "gdi" | "ccd" | "mds" | "mdf" | "nrg" | "chd" | "rvz" | "gcz" | "cso"
        | "ciso" | "zso" | "ecm" | "wbfs" | "wud" | "wux" => {
            Some("Disc/container/sidecar inspection needs a format adapter and set relationships.")
        }
        "cia" | "nsp" | "nsz" | "xci" | "xcz" | "pkg" | "wad" | "pbp" => Some(
            "Package/container inspection needs a format adapter; installed content is not container bytes.",
        ),
        _ => None,
    }
}

/// Enough bytes to recognize every header below.
pub const MAX_HEADER_BYTES: usize = 512;

/// A header that copiers/emulators prepend to cartridge content. Reference catalogs
/// such as No-Intro list the content without it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HeaderRule {
    pub format: &'static str,
    pub bytes: usize,
}

/// Recognizes headers by their signature; the SNES copier header has none, so it is
/// recognized only for SNES extensions whose size is a 1 KiB multiple plus 512 bytes.
/// Detection only adds a second, header-free comparison; it never changes the file.
pub fn detect_header(prefix: &[u8], extension: &str, size: u64) -> Option<HeaderRule> {
    let rule = |format, bytes| Some(HeaderRule { format, bytes });
    if prefix.starts_with(b"NES\x1a") {
        rule("iNES/NES 2.0 header", 16)
    } else if prefix.get(1..10) == Some(b"ATARI7800") {
        rule("Atari 7800 A78 header", 128)
    } else if prefix.starts_with(b"LYNX\0") {
        rule("Atari Lynx LNX header", 64)
    } else if prefix.starts_with(b"FDS\x1a") {
        rule("Famicom Disk System file header", 16)
    } else if matches!(extension, "smc" | "sfc" | "swc" | "fig") && size % 1024 == 512 {
        rule("SNES copier header", 512)
    } else {
        None
    }
}

pub fn inspect_format(prefix: &[u8], extension: &str, size: u64) -> Option<FileFormat> {
    let result = |name: &str, notes: Vec<&str>| {
        Some(FileFormat {
            name: name.into(),
            notes: notes.into_iter().map(str::to_owned).collect(),
        })
    };
    if prefix.starts_with(b"NES\x1a") {
        let mut notes = vec![
            "16-byte emulator header present. NES emulators commonly need this metadata; a headerless catalog comparison is not a recommendation to remove it.",
        ];
        if prefix.get(6).is_some_and(|flags| flags & 4 != 0) {
            notes.push("Header declares a 512-byte trainer. Trainer bytes are retained in the content comparison; no trainer or payload is rewritten.");
        }
        if size <= 16 {
            notes.push("File has no ROM payload after the header.");
        }
        return result(
            if prefix.get(7).is_some_and(|flags| flags & 12 == 8) {
                "NES 2.0"
            } else {
                "iNES"
            },
            notes,
        );
    }
    if prefix.get(1..10) == Some(b"ATARI7800") {
        return result(
            "Atari 7800 A78",
            vec![
                "128-byte emulator header present; retain it when required by the target emulator.",
            ],
        );
    }
    if prefix.starts_with(b"LYNX\0") {
        return result(
            "Atari Lynx LNX",
            vec![
                "64-byte emulator header present. LNX and headerless LYX representations can describe the same cartridge content.",
            ],
        );
    }
    if prefix.starts_with(b"FDS\x1a") {
        return result(
            "Famicom Disk System",
            vec![
                "16-byte FDS file header present. Headerless catalog comparison preserves disk-side data; disk structure is not rebuilt.",
            ],
        );
    }
    if matches!(extension, "smc" | "sfc" | "swc" | "fig") {
        return if size % 1024 == 512 {
            result(
                "SNES with probable copier header",
                vec![
                    "Size suggests an extra 512-byte copier header. This is a heuristic; a matching content digest confirms the comparison.",
                    "The copier header is not the cartridge's internal ROM header. Headerless catalog content can be identified without modifying either.",
                ],
            )
        } else {
            result(
                "SNES loose file",
                vec![
                    "No extra copier header detected by the size rule. Internal cartridge metadata is part of the ROM and is preserved.",
                ],
            )
        };
    }
    if matches!(extension, "n64" | "z64" | "v64") {
        let name = match prefix.get(..4) {
            Some([0x80, 0x37, 0x12, 0x40]) => "Nintendo 64 big-endian layout",
            Some([0x37, 0x80, 0x40, 0x12]) => "Nintendo 64 byte-swapped layout",
            Some([0x40, 0x12, 0x37, 0x80]) => "Nintendo 64 little-endian layout",
            _ => "Nintendo 64 filename hint",
        };
        return result(
            name,
            vec![
                "Byte order is representation metadata, not a removable copier header. Matching checks exact bytes against every available variant; no conversion is applied.",
            ],
        );
    }
    match extension {
        "nes" => result(
            "NES filename hint",
            vec![
                "No iNES/NES 2.0 header signature found. A headerless match identifies content; most NES emulators need an appropriate header to load it.",
            ],
        ),
        "a78" => result(
            "Atari 7800 filename hint",
            vec![
                "No A78 header signature found. Headerless content may match a BIN catalog; emulator requirements depend on the loader.",
            ],
        ),
        "lnx" | "lyx" => result(
            "Atari Lynx filename hint",
            vec![
                "No LNX header signature found. Headerless content can match a LYX catalog; emulator requirements depend on the loader.",
            ],
        ),
        "fds" => result(
            "FDS filename hint",
            vec![
                "No extra FDS file header signature found. Headerless file checks do not imply disk-set completeness.",
            ],
        ),
        _ => None,
    }
}

/// Advice uses recorded observations and explicit catalog representation labels;
/// absence of evidence never establishes that an arbitrary format is headerless.
pub fn header_notes(record: &crate::model::FileRecord) -> Vec<String> {
    let mut notes = record.format.as_ref().map_or_else(|| vec!["No format/header assessment recorded for this file; generic byte verification still applies.".into()], |format| {
        let mut notes = vec![format.name.clone()];
        notes.extend(format.notes.clone());
        notes
    });
    let mut policies = BTreeSet::new();
    for candidate in &record.result.candidates {
        let label = candidate.catalog_name.to_ascii_lowercase();
        let policy = if label.contains("(headered)") {
            Some("Matched catalog expects headered file bytes.")
        } else if label.contains("(headerless)") || label.contains("(unheadered)") {
            Some(
                "Matched catalog expects headerless ROM content; that does not determine emulator requirements.",
            )
        } else if crate::systems::system_family(&candidate.system) == "SNES" {
            Some(
                "SNES catalog match describes the ROM content. An extra copier header is separate from the cartridge's internal metadata.",
            )
        } else {
            None
        };
        if let Some(policy) = policy {
            policies.insert(policy);
        }
    }
    notes.extend(policies.into_iter().map(str::to_owned));
    if record.header.is_some() && record.result.explanation.starts_with("Compared without") {
        notes.push("Only the header-free content matched. The original file and its whole-file fingerprints are preserved.".into());
    }
    notes
}

pub(crate) struct LooseFilePolicy {
    extensions: BTreeSet<String>,
    extensionless_sizes: BTreeSet<u64>,
    unsized_names: BTreeSet<String>,
}

impl LooseFilePolicy {
    pub(crate) fn from_catalogs(catalogs: &[Catalog]) -> Self {
        let mut extensions: BTreeSet<_> = LOOSE_EXTENSIONS
            .iter()
            .map(|extension| (*extension).to_owned())
            .collect();
        let mut extensionless_sizes = BTreeSet::new();
        let mut unsized_names = BTreeSet::new();
        for catalog in catalogs {
            for entry in &catalog.entries {
                let extension = extension(Path::new(&entry.filename));
                if extension.is_empty() {
                    if let Some(size) = entry.size {
                        extensionless_sizes.insert(size);
                    } else if let Some(name) = entry.filename.rsplit(['/', '\\']).next() {
                        unsized_names.insert(name.to_lowercase());
                    }
                    continue;
                }
                if unsupported_extension(&extension).is_none()
                    && !extensions.contains(extension.as_ref())
                {
                    // Unusual extensions declared by a DAT are valid evidence.
                    extensions.insert(extension.into_owned());
                }
            }
        }
        Self {
            extensions,
            extensionless_sizes,
            unsized_names,
        }
    }

    pub(crate) fn accepts(&self, path: &Path, size: u64) -> bool {
        let extension = extension(path);
        if extension.is_empty() {
            return self.extensionless_sizes.contains(&size)
                || path.file_name().is_some_and(|name| {
                    self.unsized_names
                        .contains(&name.to_string_lossy().to_lowercase())
                });
        }
        unsupported_extension(&extension).is_none() && self.extensions.contains(extension.as_ref())
    }
}
