use crate::model::Catalog;
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
    "gb", "gbc", "gba", "nes", "sfc", "smc", "n64", "z64", "v64", "nds", "vb", "md", "gen", "smd",
    "sms", "gg", "sg", "pce", "sgx", "a26", "a52", "a78", "j64", "jag", "lnx", "ngp", "ngc", "ws",
    "wsc", "col", "cv", "vec", "sv", "rom", "bin",
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
    } else if matches!(extension, "smc" | "sfc" | "swc" | "fig") && size % 1024 == 512 {
        rule("SNES copier header", 512)
    } else {
        None
    }
}

/// Which extensionless files to read. Accepting all of them because one catalog entry
/// has no extension would also hash every LICENSE, Makefile and .git file in a folder.
enum Extensionless {
    Never,
    /// Only files whose size matches an extensionless catalog entry.
    Sizes(BTreeSet<u64>),
    /// An extensionless entry has no size, so size cannot narrow the candidates.
    Any,
}

pub(crate) struct LooseFilePolicy {
    extensions: BTreeSet<String>,
    extensionless: Extensionless,
}

impl LooseFilePolicy {
    pub(crate) fn from_catalogs(catalogs: &[Catalog]) -> Self {
        let mut extensions: BTreeSet<_> = LOOSE_EXTENSIONS
            .iter()
            .map(|extension| (*extension).to_owned())
            .collect();
        let mut extensionless = Extensionless::Never;
        for catalog in catalogs {
            for entry in &catalog.entries {
                let extension = extension(Path::new(&entry.filename));
                if extension.is_empty() {
                    extensionless = match (extensionless, entry.size) {
                        (Extensionless::Any, _) | (_, None) => Extensionless::Any,
                        (Extensionless::Never, Some(size)) => {
                            Extensionless::Sizes(BTreeSet::from([size]))
                        }
                        (Extensionless::Sizes(mut sizes), Some(size)) => {
                            sizes.insert(size);
                            Extensionless::Sizes(sizes)
                        }
                    };
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
            extensionless,
        }
    }

    pub(crate) fn accepts(&self, path: &Path, size: u64) -> bool {
        let extension = extension(path);
        if extension.is_empty() {
            return match &self.extensionless {
                Extensionless::Never => false,
                Extensionless::Sizes(sizes) => sizes.contains(&size),
                Extensionless::Any => true,
            };
        }
        unsupported_extension(&extension).is_none() && self.extensions.contains(extension.as_ref())
    }
}
