use crate::{
    formats::{LooseFilePolicy, unsupported_representation},
    model::*,
};
use std::{
    collections::{BTreeSet, HashMap},
    path::Path,
};

pub struct CatalogIndex {
    catalogs: Vec<Catalog>,
    by_digest: HashMap<(&'static str, String), Vec<(usize, usize)>>,
    loose_files: LooseFilePolicy,
}

impl CatalogIndex {
    pub fn new(catalogs: Vec<Catalog>) -> Self {
        let mut by_digest = HashMap::<_, Vec<_>>::new();
        for (catalog_id, catalog) in catalogs.iter().enumerate() {
            for (entry_id, entry) in catalog.entries.iter().enumerate() {
                if entry.quality == DumpQuality::Undumped {
                    continue;
                }
                for (algorithm, digest) in entry.digests.values() {
                    if let Some(digest) = digest {
                        by_digest
                            .entry((algorithm, digest.to_owned()))
                            .or_default()
                            .push((catalog_id, entry_id));
                    }
                }
            }
        }
        let loose_files = LooseFilePolicy::from_catalogs(&catalogs);
        Self {
            catalogs,
            by_digest,
            loose_files,
        }
    }

    pub fn accepts_loose_file(&self, path: &Path, size: u64) -> bool {
        self.loose_files.accepts(path, size)
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
        let mut whole = self.identify(size, digests);
        let Some(header) = header else {
            return whole;
        };
        if !whole.candidates.is_empty() {
            return whole;
        }
        let mut content = self.identify(header.size, &header.digests);
        if !content.candidates.is_empty() {
            content.explanation = format!(
                "Compared without the {} ({} bytes); the file on disk is unchanged. {}",
                header.format, header.header_bytes, content.explanation
            );
            return content;
        }
        whole.explanation = format!(
            "{} Also compared without the {} ({} bytes); no catalog entry matched either way.",
            whole.explanation, header.format, header.header_bytes
        );
        whole
    }

    pub fn identify(&self, size: u64, digests: &Digests) -> MatchResult {
        let mut ids = BTreeSet::new();
        for (algorithm, digest) in digests.values() {
            if let Some(digest) = digest
                && let Some(matches) = self.by_digest.get(&(algorithm, digest.to_owned()))
            {
                ids.extend(matches);
            }
        }
        let mut candidates = Vec::new();
        let mut rejected = Vec::new();
        for &(catalog_id, entry_id) in ids {
            let catalog = &self.catalogs[catalog_id];
            let entry = &catalog.entries[entry_id];
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
                entry: entry.clone(),
                checked_algorithms: checked,
                failures,
            };
            if candidate.failures.is_empty() {
                candidates.push(candidate);
            } else {
                rejected.push(candidate);
            }
        }
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
            c.entry.group_members > 1
                || !c.entry.unsupported_algorithms.is_empty()
                || unsupported_representation(Path::new(&c.entry.filename)).is_some()
        }) {
            (
                Verification::Unsupported,
                "Candidate belongs to a multi-file set, requires a format adapter or an unsupported comparison. Read-only inspection only.",
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
        MatchResult {
            status,
            candidates,
            rejected,
            explanation: explanation.into(),
        }
    }
}
