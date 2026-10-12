use crate::{
    archive::scan_zip,
    formats::{extension, unsupported_representation},
    hash::{Cancellation, Hashed, hash_file, is_link},
    matching::CatalogIndex,
    model::*,
};
use anyhow::{Context, Result, bail};
use std::{
    fs,
    path::{Component, Path, PathBuf},
};

pub const MAX_SCAN_RECORDS: usize = 100_000;

#[derive(Default)]
pub struct ScanOptions {
    /// Literal relative path prefixes, not glob patterns.
    pub exclusions: Vec<PathBuf>,
}

#[derive(Clone, Debug)]
pub struct ScanProgress {
    pub path: PathBuf,
    pub completed_files: usize,
    pub hashed_bytes: u64,
}

pub struct ScanReport {
    pub records: Vec<FileRecord>,
    pub cancelled: bool,
    pub hashed_bytes: u64,
}

pub(crate) fn issue(path: PathBuf, status: Verification, explanation: String) -> FileRecord {
    FileRecord {
        path,
        member: None,
        size: None,
        digests: None,
        header: None,
        format: None,
        header_check: None,
        result: MatchResult {
            status,
            candidates: vec![],
            rejected: vec![],
            explanation,
        },
    }
}

/// Matches hashed bytes and classifies the result. `name` is the file's (or ZIP
/// member's) name, used to judge whether an unmatched file looks like a ROM.
pub(crate) fn hashed_record(
    path: PathBuf,
    member: Option<String>,
    name: &Path,
    hashed: Hashed,
    index: &CatalogIndex,
) -> Result<FileRecord> {
    let (mut result, header_check) =
        index.assess(hashed.size, &hashed.digests, hashed.header.as_ref())?;
    // Unknown is for files that look like ROMs. A readme or artwork that was read
    // only because its size matched some entry is just another file.
    if result.candidates.is_empty()
        && result.rejected.is_empty()
        && (!index.is_rom_like(name) || hashed.textual)
    {
        result.status = Verification::Other;
        result.explanation =
            "No catalog entry matches, and this does not look like a ROM file by type or content."
                .into();
    }
    Ok(FileRecord {
        path,
        member,
        size: Some(hashed.size),
        digests: Some(hashed.digests),
        header: hashed.header,
        format: hashed.format,
        header_check,
        result,
    })
}

pub fn scan_folder(
    root: &Path,
    index: &CatalogIndex,
    options: &ScanOptions,
    cancel: &Cancellation,
    mut progress: impl FnMut(ScanProgress),
) -> Result<ScanReport> {
    let metadata = fs::symlink_metadata(root).context("Inspecting selected folder")?;
    if is_link(&metadata) || !metadata.is_dir() {
        bail!("Select a regular directory; root links/reparse points are not followed");
    }
    for prefix in &options.exclusions {
        if prefix.as_os_str().is_empty()
            || prefix
                .components()
                .any(|c| !matches!(c, Component::Normal(_)))
        {
            bail!("Exclusions must be relative path prefixes without '.' or '..'");
        }
    }
    let root = root.canonicalize()?;
    // Keep only one read_dir iterator per recursion level; do not collect a huge directory.
    let mut directories = vec![(
        root.clone(),
        fs::read_dir(&root).context("Reading selected folder")?,
    )];
    let mut records = Vec::new();
    let mut hashed_bytes = 0;
    while let Some((directory, entries)) = directories.last_mut() {
        if cancel.is_cancelled() {
            break;
        }
        let Some(next) = entries.next() else {
            directories.pop();
            continue;
        };
        if records.len() >= MAX_SCAN_RECORDS {
            bail!("Scan exceeds the 100,000-record prototype limit; select a smaller folder");
        }
        let item = match next {
            Ok(item) => item,
            Err(error) => {
                records.push(issue(
                    directory.clone(),
                    Verification::ReadError,
                    error.to_string(),
                ));
                continue;
            }
        };
        let path = item.path();
        let relative = path.strip_prefix(&root)?;
        if options
            .exclusions
            .iter()
            .any(|prefix| relative.starts_with(prefix))
        {
            continue;
        }
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) => {
                records.push(issue(path, Verification::ReadError, error.to_string()));
                continue;
            }
        };
        if is_link(&metadata) {
            records.push(issue(
                path,
                Verification::Unsupported,
                "Symlink/reparse point skipped; no target bytes were intentionally read".into(),
            ));
            continue;
        }
        if metadata.is_dir() {
            if directories.len() >= 128 {
                records.push(issue(
                    path,
                    Verification::Unsupported,
                    "Directory nesting limit (128) exceeded".into(),
                ));
                continue;
            }
            match fs::read_dir(&path) {
                Ok(entries) => directories.push((path, entries)),
                Err(error) => records.push(issue(path, Verification::ReadError, error.to_string())),
            }
            continue;
        }
        if !metadata.is_file() {
            records.push(issue(
                path,
                Verification::Unsupported,
                "Only regular loose files can be hashed.".into(),
            ));
            continue;
        }
        if extension(&path) == "zip" {
            let completed_files = records.len();
            progress(ScanProgress {
                path: path.clone(),
                completed_files,
                hashed_bytes,
            });
            let base = hashed_bytes;
            match scan_zip(&path, index, cancel, |bytes| {
                progress(ScanProgress {
                    path: path.clone(),
                    completed_files,
                    hashed_bytes: base + bytes,
                })
            }) {
                Ok((members, bytes)) => {
                    hashed_bytes += bytes;
                    records.extend(members);
                    if records.len() > MAX_SCAN_RECORDS {
                        bail!(
                            "Scan exceeds the 100,000-record prototype limit; select a smaller folder"
                        );
                    }
                }
                Err(_) if cancel.is_cancelled() => break,
                Err(error) => {
                    records.push(issue(path, Verification::ReadError, format!("{error:#}")))
                }
            }
            continue;
        }
        if let Some(explanation) = unsupported_representation(&path) {
            records.push(issue(path, Verification::Unsupported, explanation.into()));
            continue;
        }
        if !index.should_read(&path, metadata.len()) {
            records.push(issue(
                path,
                Verification::Other,
                "Not a ROM file type, and no active catalog lists a file of this size; not read."
                    .into(),
            ));
            continue;
        }
        let completed_files = records.len();
        progress(ScanProgress {
            path: path.clone(),
            completed_files,
            hashed_bytes,
        });
        let hashed = hash_file(&path, cancel, |bytes| {
            progress(ScanProgress {
                path: path.clone(),
                completed_files,
                hashed_bytes: hashed_bytes + bytes,
            })
        });
        match hashed {
            Ok(hashed) => {
                hashed_bytes += hashed.size;
                let name = path.clone();
                records.push(hashed_record(path, None, &name, hashed, index)?);
            }
            Err(_) if cancel.is_cancelled() => break,
            Err(error) => records.push(issue(path, Verification::ReadError, format!("{error:#}"))),
        }
    }
    records.sort_by(|a, b| a.path.cmp(&b.path).then_with(|| a.member.cmp(&b.member)));
    Ok(ScanReport {
        records,
        cancelled: cancel.is_cancelled(),
        hashed_bytes,
    })
}
