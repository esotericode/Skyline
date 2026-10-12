use crate::{
    formats::{LooseFilePolicy, unsupported_representation},
    model::*,
};
use std::{
    collections::{BTreeSet, HashMap},
    path::Path,
    sync::Arc,
};

pub struct CatalogIndex {
    catalogs: Arc<Vec<Catalog>>,
    by_digest: HashMap<DigestKey, References>,
    loose_files: LooseFilePolicy,
}

/// Fixed binary keys avoid a heap allocation for every indexed digest and lookup.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct DigestKey {
    algorithm: u8,
    bytes: [u8; 32],
}

impl DigestKey {
    fn parse(algorithm: &str, hex: &str) -> Option<Self> {
        let (algorithm, length) = match algorithm {
            "CRC32" => (0, 4),
            "MD5" => (1, 16),
            "SHA-1" => (2, 20),
            "SHA-256" => (3, 32),
            _ => return None,
        };
        if hex.len() != length * 2 {
            return None;
        }
        let nibble = |byte| match byte {
            b'0'..=b'9' => Some(byte - b'0'),
            b'a'..=b'f' => Some(byte - b'a' + 10),
            b'A'..=b'F' => Some(byte - b'A' + 10),
            _ => None,
        };
        let mut bytes = [0; 32];
        for (i, pair) in hex.as_bytes().as_chunks::<2>().0.iter().enumerate() {
            bytes[i] = nibble(pair[0])? * 16 + nibble(pair[1])?;
        }
        Some(Self { algorithm, bytes })
    }
}

/// Most digests identify one reference; allocate a list only for shared content.
enum References {
    One((usize, usize)),
    Many(Vec<(usize, usize)>),
}

impl References {
    fn push(&mut self, id: (usize, usize)) {
        match self {
            Self::One(first) => *self = Self::Many(vec![*first, id]),
            Self::Many(ids) => ids.push(id),
        }
    }

    fn insert_into(&self, ids: &mut BTreeSet<(usize, usize)>) {
        match self {
            Self::One(id) => {
                ids.insert(*id);
            }
            Self::Many(matches) => ids.extend(matches.iter().copied()),
        }
    }
}

impl CatalogIndex {
    pub fn new(catalogs: Vec<Catalog>) -> Self {
        Self::from_shared(Arc::new(catalogs))
    }

    /// The desktop and worker share immutable reference entries, avoiding a second
    /// full catalog copy for large multi-system folders.
    pub fn from_shared(catalogs: Arc<Vec<Catalog>>) -> Self {
        let mut by_digest = HashMap::<DigestKey, References>::new();
        for (catalog_id, catalog) in catalogs.iter().enumerate() {
            for (entry_id, entry) in catalog.entries.iter().enumerate() {
                if entry.quality == DumpQuality::Undumped {
                    continue;
                }
                for (algorithm, digest) in entry.digests.values() {
                    if let Some(key) = digest.and_then(|digest| DigestKey::parse(algorithm, digest))
                    {
                        by_digest
                            .entry(key)
                            .and_modify(|references: &mut References| {
                                references.push((catalog_id, entry_id))
                            })
                            .or_insert(References::One((catalog_id, entry_id)));
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

    pub fn collection_systems(
        &self,
        records: &[FileRecord],
    ) -> Vec<crate::systems::CollectionSystem> {
        crate::systems::collection_systems(records, &self.catalogs)
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
            if let Some(key) = digest.and_then(|digest| DigestKey::parse(algorithm, digest))
                && let Some(matches) = self.by_digest.get(&key)
            {
                matches.insert_into(&mut ids);
            }
        }
        let mut candidates = Vec::new();
        let mut rejected = Vec::new();
        for (catalog_id, entry_id) in ids {
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
