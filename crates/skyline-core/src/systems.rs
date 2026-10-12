//! Presentation grouping only. Neither a DAT label nor an extension verifies a ROM.
use crate::model::{Catalog, FileRecord};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

pub fn system_family(label: &str) -> &str {
    let base = label.split(" (").next().unwrap_or(label).trim();
    match base {
        "Nintendo - Nintendo Entertainment System" | "NES" => "NES",
        "Nintendo - Super Nintendo Entertainment System" | "SNES" => "SNES",
        "Nintendo - Nintendo 64" | "Nintendo 64" => "Nintendo 64",
        "Nintendo - Nintendo 64DD" | "Nintendo 64DD" => "Nintendo 64DD",
        "Nintendo - Game Boy" | "Game Boy" => "Game Boy",
        "Nintendo - Game Boy Color" | "Game Boy Color" => "Game Boy Color",
        "Nintendo - Game Boy Advance" | "Game Boy Advance" | "GBA" => "Game Boy Advance",
        "Nintendo - Nintendo DS" | "Nintendo DS" => "Nintendo DS",
        "Nintendo - Nintendo 3DS" | "Nintendo 3DS" => "Nintendo 3DS",
        "Nintendo - Virtual Boy" | "Virtual Boy" => "Virtual Boy",
        "Sega - Mega Drive - Genesis" | "Mega Drive" | "Genesis" => "Mega Drive",
        "Sega - Master System - Mark III" | "Master System" => "Master System",
        "Sega - Game Gear" | "Game Gear" => "Game Gear",
        "Sega - SG-1000" | "SG-1000" => "SG-1000",
        "NEC - PC Engine - TurboGrafx-16" | "PC Engine" => "PC Engine",
        "NEC - PC Engine SuperGrafx" | "SuperGrafx" => "SuperGrafx",
        "Atari - 2600" | "Atari - Atari 2600" | "Atari 2600" => "Atari 2600",
        "Atari - 5200" | "Atari - Atari 5200" | "Atari 5200" => "Atari 5200",
        "Atari - 7800" | "Atari - Atari 7800" | "Atari 7800" => "Atari 7800",
        "Atari - Jaguar" | "Atari - Atari Jaguar" | "Atari Jaguar" => "Atari Jaguar",
        "Atari - Lynx" | "Atari - Atari Lynx" | "Atari Lynx" => "Atari Lynx",
        "SNK - NeoGeo Pocket" | "Neo Geo Pocket" => "Neo Geo Pocket",
        "SNK - NeoGeo Pocket Color" | "Neo Geo Pocket Color" => "Neo Geo Pocket Color",
        "Bandai - WonderSwan" | "WonderSwan" => "WonderSwan",
        "Bandai - WonderSwan Color" | "WonderSwan Color" => "WonderSwan Color",
        "Coleco - ColecoVision" | "ColecoVision" => "ColecoVision",
        "GCE - Vectrex" | "Vectrex" => "Vectrex",
        "Microsoft - MSX" | "MSX" => "MSX",
        "Microsoft - MSX2" | "MSX2" => "MSX2",
        "Watara - Supervision" | "Supervision" => "Supervision",
        "Sega - SG-1000 - SC-3000" => "SG-1000",
        _ => base,
    }
}

pub fn system_hint(path: &Path) -> Option<&'static str> {
    let extension = path.extension()?.to_str()?.to_ascii_lowercase();
    Some(match extension.as_str() {
        "gb" => "Game Boy",
        "gbc" => "Game Boy Color",
        "gba" => "Game Boy Advance",
        "nes" => "NES",
        "sfc" | "smc" | "swc" | "fig" => "SNES",
        "n64" | "z64" | "v64" => "Nintendo 64",
        "ndd" => "Nintendo 64DD",
        "nds" | "ids" => "Nintendo DS",
        "3ds" | "cia" => "Nintendo 3DS",
        "vb" => "Virtual Boy",
        "md" | "gen" | "smd" => "Mega Drive",
        "sms" => "Master System",
        "gg" => "Game Gear",
        "sg" => "SG-1000",
        "pce" => "PC Engine",
        "sgx" => "SuperGrafx",
        "a26" => "Atari 2600",
        "a52" => "Atari 5200",
        "a78" => "Atari 7800",
        "j64" | "jag" => "Atari Jaguar",
        "lnx" => "Atari Lynx",
        "ngp" => "Neo Geo Pocket",
        "ngc" => "Neo Geo Pocket Color",
        "ws" => "WonderSwan",
        "wsc" => "WonderSwan Color",
        "col" | "cv" => "ColecoVision",
        "vec" => "Vectrex",
        "sv" => "Supervision",
        _ => return None,
    })
}

pub fn row_in_family(record: &FileRecord, family: &str) -> bool {
    if record.result.candidates.is_empty() {
        system_hint(&record.path) == Some(family)
    } else {
        record
            .result
            .candidates
            .iter()
            .any(|candidate| system_family(&candidate.system) == family)
    }
}

#[derive(Clone, Debug, Default)]
pub struct CollectionSystem {
    pub name: String,
    pub matched_files: usize,
    pub suggested_files: usize,
    pub available_catalogs: usize,
    pub used_catalogs: BTreeSet<String>,
    pub rows: BTreeSet<usize>,
}

/// Only systems with collection rows appear. Unknown filenames supply labelled
/// hints so a missing GBA DAT can be useful feedback; they remain Unknown.
pub fn collection_systems(records: &[FileRecord], catalogs: &[Catalog]) -> Vec<CollectionSystem> {
    let mut systems: BTreeMap<String, CollectionSystem> = BTreeMap::new();
    // New/obscure catalogs supply hints automatically when an extension belongs
    // to exactly one family. Shared generic extensions never force a system guess.
    let mut hints: BTreeMap<String, BTreeSet<&str>> = BTreeMap::new();
    for catalog in catalogs {
        let family = system_family(&catalog.provenance.system);
        for entry in &catalog.entries {
            if let Some(extension) = Path::new(&entry.filename)
                .extension()
                .and_then(|s| s.to_str())
            {
                hints
                    .entry(extension.to_ascii_lowercase())
                    .or_default()
                    .insert(family);
            }
        }
    }
    for (id, record) in records.iter().enumerate() {
        let mut seen = BTreeSet::new();
        for candidate in &record.result.candidates {
            let family = system_family(&candidate.system).to_owned();
            let system = systems
                .entry(family.clone())
                .or_insert_with(|| CollectionSystem {
                    name: family.clone(),
                    ..Default::default()
                });
            if seen.insert(family) {
                system.matched_files += 1;
            }
            system.rows.insert(id);
            system
                .used_catalogs
                .insert(candidate.catalog_sha256.clone());
        }
        let hint = system_hint(&record.path).or_else(|| {
            let extension = record.path.extension()?.to_str()?.to_ascii_lowercase();
            // bin/rom/img/raw/wav are generic even when the folder only happens
            // to contain one system's DAT; do not guess from unrelated content.
            if matches!(extension.as_str(), "bin" | "rom" | "img" | "raw" | "wav") {
                return None;
            }
            let families = hints.get(&extension)?;
            (families.len() == 1).then(|| *families.first().unwrap())
        });
        if record.result.candidates.is_empty()
            && let Some(hint) = hint
        {
            let system = systems
                .entry(hint.into())
                .or_insert_with(|| CollectionSystem {
                    name: hint.into(),
                    ..Default::default()
                });
            system.suggested_files += 1;
            system.rows.insert(id);
        }
    }
    for catalog in catalogs {
        if !catalog.entries.is_empty()
            && let Some(system) = systems.get_mut(system_family(&catalog.provenance.system))
        {
            system.available_catalogs += 1;
        }
    }
    systems.into_values().collect()
}
