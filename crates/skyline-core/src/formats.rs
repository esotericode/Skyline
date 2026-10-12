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

pub(crate) struct LooseFilePolicy {
    extensions: BTreeSet<String>,
}

impl LooseFilePolicy {
    pub(crate) fn from_catalogs(catalogs: &[Catalog]) -> Self {
        let mut extensions: BTreeSet<_> = LOOSE_EXTENSIONS
            .iter()
            .map(|extension| (*extension).to_owned())
            .collect();
        for catalog in catalogs {
            for entry in &catalog.entries {
                let extension = extension(Path::new(&entry.filename));
                if unsupported_extension(&extension).is_none()
                    && !extensions.contains(extension.as_ref())
                {
                    // Unusual and extensionless filenames are valid DAT evidence.
                    extensions.insert(extension.into_owned());
                }
            }
        }
        Self { extensions }
    }

    pub(crate) fn accepts(&self, path: &Path) -> bool {
        let extension = extension(path);
        unsupported_extension(&extension).is_none() && self.extensions.contains(extension.as_ref())
    }
}
