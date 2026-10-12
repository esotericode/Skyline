use crate::{
    formats::{
        extension, header_bytes, is_common_non_rom, is_rom_extension, spaced_hex,
        unsupported_representation,
    },
    model::*,
    reference,
    systems::{CollectionSystem, ReferenceSummary, system_family},
};
use anyhow::{Context, Result};
use rusqlite::Connection;
use std::{
    collections::{BTreeMap, BTreeSet, HashSet},
    path::Path,
};

/// Lengths of the headers `formats::detect_header` recognizes. A headerless file's
/// canonical header is looked up at its content size plus each length.
const HEADER_LENGTHS: [u64; 4] = [16, 64, 128, 512];

/// Looks up catalog entries by digest in an indexed reference database. Only entry
/// sizes and extension summaries are kept in memory, never the entries themselves.
pub struct CatalogIndex {
    db: Connection,
    catalogs: BTreeMap<i64, CatalogInfo>,
    sizes: HashSet<u64>,
    unsized_names: HashSet<String>,
    extension_families: BTreeMap<String, BTreeSet<String>>,
}

impl CatalogIndex {
    /// An in-memory index over parsed catalogs, for tools and tests.
    pub fn new(catalogs: Vec<Catalog>) -> Self {
        Self::in_memory(catalogs).expect("building an in-memory reference index")
    }

    fn in_memory(catalogs: Vec<Catalog>) -> Result<Self> {
        let db = Connection::open_in_memory()?;
        db.execute_batch(reference::SCHEMA)?;
        let transaction = db.unchecked_transaction()?;
        let mut infos = Vec::with_capacity(catalogs.len());
        for (position, catalog) in catalogs.iter().enumerate() {
            let id = position as i64 + 1;
            reference::insert_entries(&transaction, id, catalog)?;
            infos.push((id, CatalogInfo::of(catalog)));
        }
        transaction.commit()?;
        Self::open(db, infos)
    }

    /// An index over the given catalogs, whose entries are stored in `db` under their
    /// ids. Entries of other catalogs in the same database are ignored.
    pub(crate) fn open(db: Connection, catalogs: Vec<(i64, CatalogInfo)>) -> Result<Self> {
        let catalogs: BTreeMap<_, _> = catalogs.into_iter().collect();
        let ids = serde_json::to_string(&catalogs.keys().collect::<Vec<_>>())?;
        let active = "catalog_id IN (SELECT value FROM json_each(?1))";
        let sizes = db
            .prepare(&format!(
                "SELECT DISTINCT size FROM entries WHERE size IS NOT NULL AND {active}"
            ))?
            .query_map([&ids], |row| row.get::<_, i64>(0))?
            .map(|size| size.map(|size| size as u64))
            .collect::<rusqlite::Result<_>>()?;
        let mut unsized_names = HashSet::new();
        let mut statement = db.prepare(&format!(
            "SELECT filename FROM entries WHERE size IS NULL AND {active}"
        ))?;
        for filename in statement.query_map([&ids], |row| row.get::<_, String>(0))? {
            let filename = filename?;
            if reference::filename_extension(&filename).is_empty() {
                let name = filename.rsplit(['/', '\\']).next().unwrap_or(&filename);
                unsized_names.insert(name.to_lowercase());
            }
        }
        drop(statement);
        let mut extension_families = BTreeMap::<String, BTreeSet<String>>::new();
        let mut statement = db.prepare(&format!(
            "SELECT catalog_id,extension FROM catalog_extensions WHERE extension<>'' AND {active}"
        ))?;
        for row in statement.query_map([&ids], |row| Ok((row.get::<_, i64>(0)?, row.get(1)?)))? {
            let (id, extension): (i64, String) = row?;
            if let Some(info) = catalogs.get(&id) {
                extension_families
                    .entry(extension)
                    .or_default()
                    .insert(system_family(&info.provenance.system).to_owned());
            }
        }
        drop(statement);
        Ok(Self {
            db,
            catalogs,
            sizes,
            unsized_names,
            extension_families,
        })
    }

    pub fn catalogs(&self) -> impl Iterator<Item = &CatalogInfo> {
        self.catalogs.values()
    }

    /// Catalog metadata and extension hints for presentation; no entries.
    pub fn summary(&self) -> ReferenceSummary {
        ReferenceSummary {
            catalogs: self.catalogs.values().cloned().collect(),
            extension_families: self.extension_families.clone(),
        }
    }

    pub fn collection_systems(&self, records: &[FileRecord]) -> Vec<CollectionSystem> {
        crate::systems::collection_systems(records, &self.summary())
    }

    /// Common ROM extensions, and extensions an active catalog declares (so a new
    /// system's own extension works without an update), except everyday document,
    /// media, save and program types that some PC/digital DATs also list.
    pub fn is_rom_like(&self, path: &Path) -> bool {
        let extension = extension(path);
        is_rom_extension(path)
            || (!extension.is_empty()
                && !is_common_non_rom(&extension)
                && self.extension_families.contains_key(extension.as_ref()))
    }

    /// Read ROM-like files, and any other file only when its size equals a catalog
    /// entry's size. Folders of artwork, manuals, saves and readmes are not hashed
    /// just because some DAT somewhere lists a file of that type.
    pub fn should_read(&self, path: &Path, size: u64) -> bool {
        if unsupported_representation(path).is_some() {
            return false;
        }
        self.is_rom_like(path)
            || self.sizes.contains(&size)
            || (extension(path).is_empty()
                && path.file_name().is_some_and(|name| {
                    self.unsized_names
                        .contains(&name.to_string_lossy().to_lowercase())
                }))
    }

    fn candidates(&self, size: u64, digests: &Digests) -> Result<(Vec<Candidate>, Vec<Candidate>)> {
        let mut candidates = Vec::new();
        let mut rejected = Vec::new();
        for (id, entry) in reference::by_digests(&self.db, digests)? {
            let Some(catalog) = self.catalogs.get(&id) else {
                continue;
            };
            if entry.quality == DumpQuality::Undumped {
                continue;
            }
            let mut failures = Vec::new();
            let mut checked = Vec::new();
            if let Some(expected) = entry.size.filter(|&expected| expected != size) {
                failures.push(format!("Size: expected {expected}, calculated {size}"));
            }
            for ((algorithm, expected), (_, actual)) in
                entry.digests.values().into_iter().zip(digests.values())
            {
                if let Some(expected) = expected {
                    checked.push(algorithm.into());
                    if actual != Some(expected) {
                        failures.push(format!("{algorithm} differs or was not calculated"));
                    }
                }
            }
            let candidate = Candidate {
                catalog_sha256: catalog.provenance.snapshot_sha256.clone(),
                catalog_name: catalog.name.clone(),
                catalog_version: catalog.version.clone(),
                system: catalog.provenance.system.clone(),
                entry,
                checked_algorithms: checked,
                failures,
            };
            if candidate.failures.is_empty() {
                candidates.push(candidate);
            } else {
                rejected.push(candidate);
            }
        }
        Ok((candidates, rejected))
    }

    pub fn identify(&self, size: u64, digests: &Digests) -> MatchResult {
        self.try_identify(size, digests)
            .expect("reading the reference index")
    }

    pub fn try_identify(&self, size: u64, digests: &Digests) -> Result<MatchResult> {
        let (candidates, rejected) = self
            .candidates(size, digests)
            .context("Looking up catalog references")?;
        // Snapshot revisions describing the same identity are alternatives, not distinct games.
        let identities: BTreeSet<_> = candidates
            .iter()
            .map(|c| (&c.system, &c.entry.game, &c.entry.filename))
            .collect();
        let (status, explanation) = if candidates.is_empty() {
            (
                Verification::Unknown,
                if rejected.is_empty() {
                    "No suitable imported catalog entry matches. Unknown does not mean corrupt."
                } else {
                    "Some reference digests matched, but size or another supplied digest disagreed. No complete match."
                },
            )
        } else if identities.len() > 1 {
            (
                Verification::Ambiguous,
                "The byte evidence supports multiple catalog identities. All candidates are retained.",
            )
        } else if candidates.iter().any(|c| {
            !c.entry.unsupported_algorithms.is_empty()
                || unsupported_representation(Path::new(&c.entry.filename)).is_some()
        }) {
            (
                Verification::Unsupported,
                "Candidate requires a format adapter or an unsupported comparison. Read-only inspection only.",
            )
        } else if candidates.iter().any(|c| c.entry.digests.strong()) {
            (
                Verification::Exact,
                "Size (when supplied) and every supported digest supplied by each listed matching entry agree. Check the reference dump flag separately.",
            )
        } else {
            (
                Verification::CrcOnly,
                "Only CRC32 reference evidence is available. Reduced confidence; this is not a fully verified good dump.",
            )
        };
        let mut explanation = explanation.to_owned();
        // Each file is verified on its own. A game may list alternatives (e.g. two
        // revisions of one cartridge) or the parts of a set; neither changes whether
        // this file's bytes match its entry.
        if let Some(members) = candidates
            .iter()
            .map(|c| c.entry.group_members)
            .filter(|&members| members > 1)
            .max()
        {
            explanation.push_str(&format!(
                " This file is one of {members} listed for its game; whether the others are present is not checked."
            ));
        }
        Ok(MatchResult {
            status,
            candidates,
            rejected,
            explanation,
        })
    }

    /// Whole-file evidence first, so catalogs that list headered files still match as
    /// before. Only when the whole file has no candidate is the header-free content
    /// compared, and the explanation then says which bytes were compared.
    pub fn identify_file(
        &self,
        size: u64,
        digests: &Digests,
        header: Option<&HeaderEvidence>,
    ) -> MatchResult {
        self.assess(size, digests, header)
            .expect("reading the reference index")
            .0
    }

    /// `identify_file` plus a comparison of the file's header with the canonical
    /// header a catalog records for the same game.
    pub fn assess(
        &self,
        size: u64,
        digests: &Digests,
        header: Option<&HeaderEvidence>,
    ) -> Result<(MatchResult, Option<HeaderCheck>)> {
        let mut whole = self.try_identify(size, digests)?;
        let (result, content_matched) = match header {
            Some(header) if whole.candidates.is_empty() => {
                let mut content = self.try_identify(header.size, &header.digests)?;
                if content.candidates.is_empty() {
                    whole.explanation = format!(
                        "{} Also compared without the {} ({} bytes); no catalog entry matched either way.",
                        whole.explanation, header.format, header.header_bytes
                    );
                    (whole, false)
                } else {
                    content.explanation = format!(
                        "Compared without the {} ({} bytes); the file on disk is unchanged. {}",
                        header.format, header.header_bytes, content.explanation
                    );
                    (content, true)
                }
            }
            _ => (whole, false),
        };
        let check = self.header_check(&result, size, header, content_matched)?;
        Ok((result, check))
    }

    fn header_check(
        &self,
        result: &MatchResult,
        size: u64,
        header: Option<&HeaderEvidence>,
        content_matched: bool,
    ) -> Result<Option<HeaderCheck>> {
        if !content_matched {
            // A whole-file match with an entry that records its header proves the
            // file starts with exactly those bytes.
            if let Some((candidate, expected)) = result.candidates.iter().find_map(|c| {
                Some((
                    c,
                    c.entry.reference_header.as_deref().and_then(header_bytes)?,
                ))
            }) {
                let expected = spaced_hex(&expected);
                return Ok(Some(HeaderCheck {
                    actual: Some(expected.clone()),
                    expected,
                    differences: vec![],
                    catalog: candidate.catalog_name.clone(),
                }));
            }
            if header.is_some() {
                // Whole-file match of a headered file whose entry records no header.
                return Ok(None);
            }
        }
        // Content matched without a header (stripped, or the file has none). Find the
        // same game's canonical header in a catalog of the same system family.
        let present = header.filter(|_| content_matched);
        let content_size = present.map_or(size, |header| header.size);
        let sizes: Vec<u64> = match present {
            Some(header) => vec![content_size + header.header_bytes],
            None => HEADER_LENGTHS
                .iter()
                .map(|length| content_size + length)
                .collect(),
        };
        let actual = present
            .and_then(|header| header.bytes.as_deref())
            .and_then(header_bytes);
        for candidate in &result.candidates {
            let family = system_family(&candidate.system);
            for (id, entry) in
                reference::headered_variants(&self.db, &candidate.entry.game, &sizes)?
            {
                let Some(catalog) = self.catalogs.get(&id) else {
                    continue;
                };
                let Some(expected) = entry.reference_header.as_deref().and_then(header_bytes)
                else {
                    continue;
                };
                if system_family(&catalog.provenance.system) != family
                    || entry.size.and_then(|size| size.checked_sub(content_size))
                        != Some(expected.len() as u64)
                {
                    continue;
                }
                let differences = actual
                    .as_ref()
                    .map(|actual| {
                        (0..expected.len().max(actual.len()))
                            .filter(|&i| actual.get(i) != expected.get(i))
                            .collect()
                    })
                    .unwrap_or_default();
                return Ok(Some(HeaderCheck {
                    expected: spaced_hex(&expected),
                    actual: actual.as_deref().map(spaced_hex),
                    differences,
                    catalog: catalog.name.clone(),
                }));
            }
        }
        Ok(None)
    }
}
