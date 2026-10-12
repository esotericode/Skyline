//! Indexed catalog entries. Entries are stored as compact rows with binary digests and
//! looked up per scanned file, so neither the store nor a scan holds every catalog in
//! memory. Used by the on-disk store and by in-memory indexes built from parsed DATs.
use crate::{
    catalog::quality_of,
    model::{Catalog, CatalogEntry, Digests},
};
use anyhow::Result;
use rusqlite::{Connection, Row, params};
use std::{collections::BTreeMap, path::Path};

pub(crate) const SCHEMA: &str = "
    CREATE TABLE IF NOT EXISTS entries (
        catalog_id INTEGER NOT NULL,
        entry INTEGER NOT NULL,
        game TEXT NOT NULL,
        filename TEXT NOT NULL,
        size INTEGER,
        crc32 INTEGER,
        md5 BLOB,
        sha1 BLOB,
        sha256 BLOB,
        status TEXT NOT NULL,
        header TEXT,
        group_members INTEGER NOT NULL,
        unsupported TEXT,
        PRIMARY KEY (catalog_id, entry)
    ) WITHOUT ROWID;
    CREATE INDEX IF NOT EXISTS entries_crc32 ON entries(crc32) WHERE crc32 IS NOT NULL;
    CREATE INDEX IF NOT EXISTS entries_md5 ON entries(md5) WHERE md5 IS NOT NULL;
    CREATE INDEX IF NOT EXISTS entries_sha1 ON entries(sha1) WHERE sha1 IS NOT NULL;
    CREATE INDEX IF NOT EXISTS entries_sha256 ON entries(sha256) WHERE sha256 IS NOT NULL;
    CREATE INDEX IF NOT EXISTS entries_size ON entries(size) WHERE size IS NOT NULL;
    CREATE TABLE IF NOT EXISTS catalog_extensions (
        catalog_id INTEGER NOT NULL,
        extension TEXT NOT NULL,
        entries INTEGER NOT NULL,
        PRIMARY KEY (catalog_id, extension)
    ) WITHOUT ROWID;";

const COLUMNS: &str = "catalog_id,entry,game,filename,size,crc32,md5,sha1,sha256,status,header,group_members,unsupported";

fn hex_to_bytes(hex: &str) -> Option<Vec<u8>> {
    if !hex.len().is_multiple_of(2) {
        return None;
    }
    (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(hex.get(i..i + 2)?, 16).ok())
        .collect()
}

fn bytes_to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// Digest columns in storage form: CRC32 as an integer, the rest as raw bytes.
fn digest_columns(digests: &Digests) -> (Option<i64>, [Option<Vec<u8>>; 3]) {
    let crc = digests
        .crc32
        .as_deref()
        .and_then(|hex| u32::from_str_radix(hex, 16).ok())
        .map(i64::from);
    let bytes = |digest: &Option<String>| digest.as_deref().and_then(hex_to_bytes);
    (
        crc,
        [
            bytes(&digests.md5),
            bytes(&digests.sha1),
            bytes(&digests.sha256),
        ],
    )
}

/// Lowercase extension of a catalog filename, ignoring any folder part.
pub(crate) fn filename_extension(filename: &str) -> String {
    let name = filename.rsplit(['/', '\\']).next().unwrap_or(filename);
    Path::new(name)
        .extension()
        .map(|extension| extension.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default()
}

pub(crate) fn insert_entries(db: &Connection, catalog_id: i64, catalog: &Catalog) -> Result<()> {
    let mut insert = db.prepare_cached(&format!(
        "INSERT INTO entries({COLUMNS}) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13)"
    ))?;
    let mut extensions = BTreeMap::<String, i64>::new();
    for (index, entry) in catalog.entries.iter().enumerate() {
        let (crc32, [md5, sha1, sha256]) = digest_columns(&entry.digests);
        let unsupported = (!entry.unsupported_algorithms.is_empty())
            .then(|| entry.unsupported_algorithms.join(","));
        insert.execute(params![
            catalog_id,
            index as i64,
            entry.game,
            entry.filename,
            entry.size.map(|size| size as i64),
            crc32,
            md5,
            sha1,
            sha256,
            entry.raw_status,
            entry.reference_header,
            entry.group_members as i64,
            unsupported,
        ])?;
        *extensions
            .entry(filename_extension(&entry.filename))
            .or_default() += 1;
    }
    let mut insert = db.prepare_cached(
        "INSERT INTO catalog_extensions(catalog_id,extension,entries) VALUES (?1,?2,?3)",
    )?;
    for (extension, count) in extensions {
        insert.execute(params![catalog_id, extension, count])?;
    }
    Ok(())
}

pub(crate) fn delete_entries(db: &Connection, catalog_id: i64) -> Result<()> {
    db.execute("DELETE FROM entries WHERE catalog_id=?1", [catalog_id])?;
    db.execute(
        "DELETE FROM catalog_extensions WHERE catalog_id=?1",
        [catalog_id],
    )?;
    Ok(())
}

fn entry_from_row(row: &Row<'_>) -> rusqlite::Result<(i64, CatalogEntry)> {
    let size: Option<i64> = row.get(4)?;
    let crc32: Option<i64> = row.get(5)?;
    let hex = |index| -> rusqlite::Result<Option<String>> {
        Ok(row
            .get::<_, Option<Vec<u8>>>(index)?
            .map(|b| bytes_to_hex(&b)))
    };
    let raw_status: String = row.get(9)?;
    let unsupported: Option<String> = row.get(12)?;
    Ok((
        row.get(0)?,
        CatalogEntry {
            game: row.get(2)?,
            filename: row.get(3)?,
            size: size.map(|size| size as u64),
            digests: Digests {
                crc32: crc32.map(|crc| format!("{:08x}", crc as u32)),
                md5: hex(6)?,
                sha1: hex(7)?,
                sha256: hex(8)?,
            },
            quality: quality_of(&raw_status),
            raw_status,
            reference_header: row.get(10)?,
            group_members: row.get::<_, i64>(11)? as usize,
            unsupported_algorithms: unsupported
                .map(|list| list.split(',').map(str::to_owned).collect())
                .unwrap_or_default(),
        },
    ))
}

/// Every entry of one catalog, in DAT order.
pub(crate) fn entries_of(db: &Connection, catalog_id: i64) -> Result<Vec<CatalogEntry>> {
    let mut statement = db.prepare_cached(&format!(
        "SELECT {COLUMNS} FROM entries WHERE catalog_id=?1 ORDER BY entry"
    ))?;
    let rows = statement.query_map([catalog_id], entry_from_row)?;
    Ok(rows
        .map(|row| row.map(|(_, entry)| entry))
        .collect::<rusqlite::Result<_>>()?)
}

/// Entries sharing any supplied digest, as (catalog id, entry), in a stable order.
pub(crate) fn by_digests(db: &Connection, digests: &Digests) -> Result<Vec<(i64, CatalogEntry)>> {
    let (crc32, [md5, sha1, sha256]) = digest_columns(digests);
    let mut statement = db.prepare_cached(&format!(
        "SELECT {COLUMNS} FROM entries WHERE crc32=?1
         UNION SELECT {COLUMNS} FROM entries WHERE md5=?2
         UNION SELECT {COLUMNS} FROM entries WHERE sha1=?3
         UNION SELECT {COLUMNS} FROM entries WHERE sha256=?4
         ORDER BY 1,2"
    ))?;
    let rows = statement.query_map(params![crc32, md5, sha1, sha256], entry_from_row)?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

/// Entries for `game` with catalog header metadata whose size is one of `sizes`.
pub(crate) fn headered_variants(
    db: &Connection,
    game: &str,
    sizes: &[u64],
) -> Result<Vec<(i64, CatalogEntry)>> {
    let mut statement = db.prepare_cached(&format!(
        "SELECT {COLUMNS} FROM entries WHERE size IN (SELECT value FROM json_each(?1))
         AND game=?2 AND header IS NOT NULL ORDER BY 1,2"
    ))?;
    let sizes = serde_json::to_string(sizes)?;
    let rows = statement.query_map(params![sizes, game], entry_from_row)?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}
