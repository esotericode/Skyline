use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Digests {
    pub crc32: Option<String>,
    pub md5: Option<String>,
    pub sha1: Option<String>,
    pub sha256: Option<String>,
}

impl Digests {
    pub fn values(&self) -> [(&'static str, Option<&str>); 4] {
        [
            ("CRC32", self.crc32.as_deref()),
            ("MD5", self.md5.as_deref()),
            ("SHA-1", self.sha1.as_deref()),
            ("SHA-256", self.sha256.as_deref()),
        ]
    }
    pub fn strong(&self) -> bool {
        self.md5.is_some() || self.sha1.is_some() || self.sha256.is_some()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum DumpQuality {
    Good,
    Suspect,
    Undumped,
    Unspecified,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CatalogEntry {
    pub game: String,
    pub filename: String,
    pub size: Option<u64>,
    pub digests: Digests,
    pub quality: DumpQuality,
    pub raw_status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reference_header: Option<String>,
    pub group_members: usize,
    pub unsupported_algorithms: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Provenance {
    pub provider: String,
    pub origin: PathBuf,
    pub system: String,
    pub imported_unix_seconds: u64,
    pub format: String,
    pub license_reference: Option<String>,
    pub license_review_date: Option<String>,
    pub snapshot_sha256: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Catalog {
    pub name: String,
    pub version: String,
    pub provenance: Provenance,
    pub entries: Vec<CatalogEntry>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum Verification {
    Exact,
    CrcOnly,
    Ambiguous,
    Unknown,
    ReadError,
    Unsupported,
}

impl Verification {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Exact => "Exact catalog match",
            Self::CrcOnly => "CRC-only match",
            Self::Ambiguous => "Ambiguous identity",
            Self::Unknown => "Unknown",
            Self::ReadError => "Read error",
            Self::Unsupported => "Unsupported",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Candidate {
    pub catalog_sha256: String,
    pub catalog_name: String,
    pub catalog_version: String,
    pub system: String,
    pub entry: CatalogEntry,
    pub checked_algorithms: Vec<String>,
    pub failures: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MatchResult {
    pub status: Verification,
    pub candidates: Vec<Candidate>,
    pub rejected: Vec<Candidate>,
    pub explanation: String,
}

/// Digests of the bytes after a recognized copier/emulator header. The file itself
/// is never changed; this is a second view of the same read.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct HeaderEvidence {
    pub format: String,
    pub header_bytes: u64,
    pub size: u64,
    pub digests: Digests,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FileRecord {
    pub path: PathBuf,
    pub size: Option<u64>,
    pub digests: Option<Digests>,
    /// Absent in records saved before header detection existed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub header: Option<HeaderEvidence>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format: Option<FileFormat>,
    pub result: MatchResult,
}

/// Read-only observations, separate from catalog identity and verification.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FileFormat {
    pub name: String,
    pub notes: Vec<String>,
}

impl FileRecord {
    pub fn display_name(&self) -> String {
        self.path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned()
    }
}
