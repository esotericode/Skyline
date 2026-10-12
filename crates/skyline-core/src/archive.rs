//! Read-only ZIP inspection. Each member is hashed and matched like a loose file;
//! nothing is extracted to disk and the archive is never written.
use crate::{
    formats::{extension, unsupported_representation},
    hash::{Cancellation, OpenedSource, hash_rom},
    matching::CatalogIndex,
    model::{FileRecord, MatchResult, Verification},
    scan::hashed_record,
};
use anyhow::{Result, anyhow};
use std::{
    io::{BufReader, Read},
    path::Path,
};
use zip::{ZipArchive, result::ZipError};

/// Archives listing more members than this are not ROM ZIPs and are not inspected.
pub const MAX_ZIP_MEMBERS: usize = 65_536;
/// Members declaring more than this uncompressed are not read.
pub const MAX_MEMBER_BYTES: u64 = 16 * 1024 * 1024 * 1024;
/// Large members claiming more than `MAX_RATIO` times their stored size are treated as
/// decompression bombs. ROM padding compresses well, so small members are exempt.
const RATIO_CHECK_BYTES: u64 = 1024 * 1024 * 1024;
const MAX_RATIO: u64 = 1_000;

fn member_record(
    path: &Path,
    member: &str,
    status: Verification,
    explanation: impl Into<String>,
) -> FileRecord {
    FileRecord {
        path: path.to_owned(),
        member: Some(member.to_owned()),
        size: None,
        digests: None,
        header: None,
        format: None,
        header_check: None,
        result: MatchResult {
            status,
            candidates: vec![],
            rejected: vec![],
            explanation: explanation.into(),
        },
    }
}

fn archive_record(path: &Path, status: Verification, explanation: String) -> FileRecord {
    crate::scan::issue(path.to_owned(), status, explanation)
}

/// Why a member could not be opened, as a record status and explanation.
fn open_failure(error: &ZipError) -> (Verification, String) {
    match error {
        ZipError::UnsupportedArchive(message) if *message == ZipError::PASSWORD_REQUIRED => (
            Verification::Unsupported,
            "Encrypted member; password-protected ZIPs are not read.".into(),
        ),
        ZipError::CompressionMethodNotSupported(method) => (
            Verification::Unsupported,
            format!(
                "Compression method {method} is not supported; stored, Deflate and Deflate64 members are read."
            ),
        ),
        ZipError::UnsupportedArchive(message) => (
            Verification::Unsupported,
            format!("Unsupported ZIP feature: {message}."),
        ),
        error => (
            Verification::ReadError,
            format!("Damaged ZIP member: {error}."),
        ),
    }
}

/// Hashes and matches every member of the ZIP at `path`. Returns the member records
/// and the number of uncompressed bytes hashed. Problems with the archive or a member
/// become records; only cancellation is an error.
pub fn scan_zip(
    path: &Path,
    index: &CatalogIndex,
    cancel: &Cancellation,
    mut progress: impl FnMut(u64),
) -> Result<(Vec<FileRecord>, u64)> {
    let source = match OpenedSource::open(path) {
        Ok(source) => source,
        Err(error) => {
            return Ok((
                vec![archive_record(
                    path,
                    Verification::ReadError,
                    format!("{error:#}"),
                )],
                0,
            ));
        }
    };
    let mut archive = match ZipArchive::new(BufReader::new(&source.file)) {
        Ok(archive) => archive,
        Err(error) => {
            return Ok((
                vec![archive_record(
                    path,
                    Verification::ReadError,
                    format!("Not a readable ZIP archive: {error}."),
                )],
                0,
            ));
        }
    };
    if archive.len() > MAX_ZIP_MEMBERS {
        return Ok((
            vec![archive_record(
                path,
                Verification::Unsupported,
                format!(
                    "ZIP lists {} members; archives with more than {MAX_ZIP_MEMBERS} are not inspected.",
                    archive.len()
                ),
            )],
            0,
        ));
    }
    let mut records = Vec::new();
    let mut hashed_bytes = 0;
    for position in 0..archive.len() {
        cancel.check()?;
        let (name, size, stored, is_dir) = match archive.by_index_raw(position) {
            Ok(entry) => (
                entry.name().to_owned(),
                entry.size(),
                entry.compressed_size(),
                entry.is_dir(),
            ),
            Err(error) => {
                let (status, explanation) = open_failure(&error);
                records.push(member_record(
                    path,
                    &format!("member {}", position + 1),
                    status,
                    explanation,
                ));
                continue;
            }
        };
        if is_dir {
            continue;
        }
        // The member name is only used to judge its type; nothing is written to it.
        let member_name = Path::new(&name);
        if unsupported_representation(member_name).is_some() {
            records.push(member_record(
                path,
                &name,
                Verification::Unsupported,
                "Nested archives and containers inside a ZIP are not opened.",
            ));
            continue;
        }
        if !index.should_read(member_name, size) {
            records.push(member_record(
                path,
                &name,
                Verification::Other,
                "Not a ROM file type, and no active catalog lists a file of this size; not read.",
            ));
            continue;
        }
        if size > MAX_MEMBER_BYTES || (size > RATIO_CHECK_BYTES && size / stored.max(1) > MAX_RATIO)
        {
            records.push(member_record(
                path,
                &name,
                Verification::Unsupported,
                format!(
                    "Member claims {size} bytes from {stored} stored bytes; refused as a possible decompression bomb."
                ),
            ));
            continue;
        }
        let member = match archive.by_index(position) {
            Ok(member) => member,
            Err(error) => {
                let (status, explanation) = open_failure(&error);
                records.push(member_record(path, &name, status, explanation));
                continue;
            }
        };
        // Never decompress past the declared size: a lying header cannot expand further.
        let mut bounded = member.take(size.saturating_add(1));
        let base = hashed_bytes;
        let hashed = hash_rom(
            &mut bounded,
            &extension(member_name),
            size,
            cancel,
            |bytes| progress(base + bytes),
        );
        match hashed {
            Ok(hashed) if hashed.size != size => records.push(member_record(
                path,
                &name,
                Verification::ReadError,
                format!(
                    "Member holds {} bytes but declares {size}; the archive is damaged.",
                    if hashed.size > size {
                        "more".to_owned()
                    } else {
                        hashed.size.to_string()
                    }
                ),
            )),
            Ok(hashed) => {
                hashed_bytes += hashed.size;
                records.push(hashed_record(
                    path.to_owned(),
                    Some(name.clone()),
                    member_name,
                    hashed,
                    index,
                )?);
            }
            Err(_) if cancel.is_cancelled() => return Err(anyhow!("Job cancelled")),
            Err(error) => {
                let message = format!("{error:#}");
                let explanation = if message.contains("Invalid checksum") {
                    "Member failed its CRC check; the archive is damaged.".to_owned()
                } else {
                    format!("Damaged ZIP member: {message}.")
                };
                records.push(member_record(
                    path,
                    &name,
                    Verification::ReadError,
                    explanation,
                ));
            }
        }
    }
    drop(archive);
    if let Err(error) = source.ensure_unchanged(path) {
        return Ok((
            vec![archive_record(
                path,
                Verification::ReadError,
                format!("{error:#}"),
            )],
            0,
        ));
    }
    if records.is_empty() {
        records.push(archive_record(
            path,
            Verification::Other,
            "Empty ZIP archive.".into(),
        ));
    }
    Ok((records, hashed_bytes))
}
