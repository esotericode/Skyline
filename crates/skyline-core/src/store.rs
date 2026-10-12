use crate::{
    catalog::{ImportedCatalog, MAX_DAT_BYTES, PARSER_REVISION, parse_dat},
    hash::Cancellation,
    model::*,
    settings::AppSettings,
};
use anyhow::{Context, Result, bail};
use rusqlite::{Connection, OptionalExtension, Transaction, TransactionBehavior, params};
use sha2::{Digest, Sha256};
use std::path::Path;

pub struct Store(Connection);
const SCHEMA_VERSION: i64 = 2;
const MAX_SETTINGS_BYTES: usize = 1024 * 1024;

impl Store {
    /// Must be called on a worker thread, never inside the GUI render callback.
    pub fn open(path: &Path) -> Result<Self> {
        Self::open_cancellable(path, &Cancellation::default())
    }

    pub fn open_cancellable(path: &Path, cancel: &Cancellation) -> Result<Self> {
        cancel.check()?;
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            std::fs::create_dir_all(parent)?;
        }
        let mut connection = Connection::open(path).context("Opening local Skyline database")?;
        connection.busy_timeout(std::time::Duration::from_secs(5))?;
        let version: i64 = connection.query_row("PRAGMA user_version", [], |row| row.get(0))?;
        if version > SCHEMA_VERSION {
            bail!("Database schema is newer than this Skyline build; use the newer app");
        }
        connection.execute_batch(
            "PRAGMA foreign_keys=ON; PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL;",
        )?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        // Another instance may have migrated while we waited for the write lock.
        let version: i64 = transaction.query_row("PRAGMA user_version", [], |row| row.get(0))?;
        if version > SCHEMA_VERSION {
            bail!("Database schema is newer than this Skyline build; use the newer app");
        }
        transaction.execute_batch("
            CREATE TABLE IF NOT EXISTS snapshots (sha256 TEXT PRIMARY KEY, bytes BLOB NOT NULL);
            CREATE TABLE IF NOT EXISTS catalogs (sha256 TEXT NOT NULL REFERENCES snapshots(sha256), system TEXT NOT NULL, json TEXT NOT NULL, PRIMARY KEY(sha256, system));
            CREATE TABLE IF NOT EXISTS collection (id INTEGER PRIMARY KEY, path TEXT NOT NULL, status TEXT NOT NULL, sha256 TEXT, json TEXT NOT NULL);
            CREATE INDEX IF NOT EXISTS collection_status ON collection(status);
            CREATE INDEX IF NOT EXISTS collection_fingerprint ON collection(sha256);")?;
        if version < 2 {
            transaction.execute_batch(
                "ALTER TABLE catalogs ADD COLUMN parser_revision INTEGER NOT NULL DEFAULT 0;",
            )?;
        }
        transaction.execute_batch("CREATE TABLE IF NOT EXISTS settings (id INTEGER PRIMARY KEY CHECK(id=1), json TEXT NOT NULL);")?;
        refresh_catalog_caches(&transaction, cancel)?;
        cancel.check()?;
        transaction.pragma_update(None, "user_version", SCHEMA_VERSION)?;
        transaction.commit()?;
        Ok(Self(connection))
    }

    pub fn import_catalog(&mut self, imported: &ImportedCatalog) -> Result<()> {
        let catalog = &imported.catalog;
        let calculated = format!("{:x}", Sha256::digest(&imported.snapshot));
        if calculated != catalog.provenance.snapshot_sha256 {
            bail!("Catalog snapshot fingerprint disagrees");
        }
        let json = serde_json::to_string(catalog)?;
        let transaction = self.0.transaction()?;
        transaction.execute(
            "INSERT OR IGNORE INTO snapshots(sha256,bytes) VALUES (?1,?2)",
            params![calculated, imported.snapshot],
        )?;
        transaction.execute(
            "INSERT OR IGNORE INTO catalogs(sha256,system,json,parser_revision) VALUES (?1,?2,?3,?4)",
            params![calculated, catalog.provenance.system, json, PARSER_REVISION],
        )?;
        transaction.commit()?;
        Ok(())
    }

    pub fn catalogs(&self) -> Result<Vec<Catalog>> {
        let mut statement = self
            .0
            .prepare("SELECT json FROM catalogs ORDER BY system,sha256")?;
        let mut result = Vec::new();
        for json in statement.query_map([], |row| row.get::<_, String>(0))? {
            result.push(serde_json::from_str(&json?)?);
        }
        Ok(result)
    }

    /// Transactionally replaces only the last *completed* scan. The caller must never
    /// use this for a cancelled/failed scan. Collection source files are never opened for write.
    pub fn save_completed_scan(&mut self, records: &[FileRecord]) -> Result<()> {
        let transaction = self.0.transaction()?;
        transaction.execute("DELETE FROM collection", [])?;
        {
            let mut statement = transaction
                .prepare("INSERT INTO collection(path,status,sha256,json) VALUES (?1,?2,?3,?4)")?;
            for record in records {
                statement.execute(params![
                    record.path.to_string_lossy(),
                    record.result.status.label(),
                    record.digests.as_ref().and_then(|d| d.sha256.as_deref()),
                    serde_json::to_string(record)?
                ])?;
            }
        }
        transaction.commit()?;
        Ok(())
    }

    pub fn collection(&self) -> Result<Vec<FileRecord>> {
        let mut statement = self
            .0
            .prepare("SELECT json FROM collection ORDER BY path,id")?;
        let mut result = Vec::new();
        for json in statement.query_map([], |row| row.get::<_, String>(0))? {
            result.push(serde_json::from_str(&json?)?);
        }
        Ok(result)
    }

    pub fn snapshot(&self, sha256: &str) -> Result<Vec<u8>> {
        Ok(self.0.query_row(
            "SELECT bytes FROM snapshots WHERE sha256=?1",
            [sha256],
            |row| row.get(0),
        )?)
    }

    pub fn settings(&self) -> Result<AppSettings> {
        let json: Option<String> = self
            .0
            .query_row("SELECT json FROM settings WHERE id=1", [], |row| row.get(0))
            .optional()?;
        match json {
            Some(json) if json.len() > MAX_SETTINGS_BYTES => {
                bail!("Saved preferences exceed the 1 MiB limit")
            }
            Some(json) => {
                serde_json::from_str(&json).context("Saved preferences could not be read")
            }
            None => Ok(AppSettings::default()),
        }
    }

    pub fn save_settings(&mut self, settings: &AppSettings) -> Result<()> {
        let json = serde_json::to_string(settings)?;
        if json.len() > MAX_SETTINGS_BYTES {
            bail!("Preferences exceed the 1 MiB limit");
        }
        self.0.execute("INSERT INTO settings(id,json) VALUES (1,?1) ON CONFLICT(id) DO UPDATE SET json=excluded.json", [json])?;
        Ok(())
    }
}

fn refresh_catalog_caches(transaction: &Transaction<'_>, cancel: &Cancellation) -> Result<()> {
    let newer: bool = transaction.query_row(
        "SELECT EXISTS(SELECT 1 FROM catalogs WHERE parser_revision>?1)",
        [PARSER_REVISION],
        |row| row.get(0),
    )?;
    if newer {
        bail!(
            "Catalogs were parsed by a newer Skyline build; use that build instead of downgrading"
        );
    }
    // Collect keys only; load and parse one bounded snapshot at a time.
    let keys: Vec<(String, String)> = {
        let mut statement = transaction.prepare(
            "SELECT sha256,system FROM catalogs WHERE parser_revision<?1 ORDER BY sha256,system",
        )?;
        statement
            .query_map([PARSER_REVISION], |row| Ok((row.get(0)?, row.get(1)?)))?
            .collect::<std::result::Result<_, _>>()?
    };
    for (sha256, system) in keys {
        cancel.check()?;
        let bytes_len: u64 = transaction
            .query_row(
                "SELECT length(bytes) FROM snapshots WHERE sha256=?1",
                [&sha256],
                |row| row.get(0),
            )
            .context("A preserved catalog snapshot is missing; existing data was retained")?;
        if bytes_len > MAX_DAT_BYTES {
            bail!("Preserved DAT exceeds the 64 MiB limit; existing data was retained");
        }
        let (json, bytes): (String, Vec<u8>) = transaction.query_row(
            "SELECT c.json,s.bytes FROM catalogs c JOIN snapshots s ON c.sha256=s.sha256 WHERE c.sha256=?1 AND c.system=?2",
            params![sha256, system], |row| Ok((row.get(0)?, row.get(1)?)),
        ).context("Cannot refresh a catalog without its preserved DAT snapshot; existing data was retained")?;
        let old: Catalog = serde_json::from_str(&json)
            .context("Cannot read catalog provenance; existing data was retained")?;
        if format!("{:x}", Sha256::digest(&bytes)) != sha256
            || old.provenance.snapshot_sha256 != sha256
            || old.provenance.system != system
        {
            bail!(
                "Catalog snapshot/provenance disagrees; refresh stopped and existing data was retained"
            );
        }
        let mut refreshed = parse_dat(&bytes, &old.provenance.origin, &system, cancel)
            .context("Catalog refresh failed; existing data was retained")?;
        // The import's origin, time, publisher and license metadata remain immutable.
        refreshed.provenance = old.provenance;
        transaction.execute(
            "UPDATE catalogs SET json=?1,parser_revision=?2 WHERE sha256=?3 AND system=?4",
            params![
                serde_json::to_string(&refreshed)?,
                PARSER_REVISION,
                sha256,
                system
            ],
        )?;
    }
    Ok(())
}
