use crate::{catalog::ImportedCatalog, model::*};
use anyhow::{Context, Result, bail};
use rusqlite::{Connection, params};
use sha2::{Digest, Sha256};
use std::path::Path;

pub struct Store(Connection);

impl Store {
    /// Must be called on a worker thread, never inside the GUI render callback.
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            std::fs::create_dir_all(parent)?;
        }
        let connection = Connection::open(path).context("Opening local Skyline database")?;
        connection.busy_timeout(std::time::Duration::from_secs(5))?;
        let version: i64 = connection.query_row("PRAGMA user_version", [], |row| row.get(0))?;
        if version > 1 {
            bail!("Database schema is newer than this Skyline build; use the newer app");
        }
        connection.execute_batch("PRAGMA foreign_keys=ON; PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL;
            CREATE TABLE IF NOT EXISTS snapshots (sha256 TEXT PRIMARY KEY, bytes BLOB NOT NULL);
            CREATE TABLE IF NOT EXISTS catalogs (sha256 TEXT NOT NULL REFERENCES snapshots(sha256), system TEXT NOT NULL, json TEXT NOT NULL, PRIMARY KEY(sha256, system));
            CREATE TABLE IF NOT EXISTS collection (id INTEGER PRIMARY KEY, path TEXT NOT NULL, status TEXT NOT NULL, sha256 TEXT, json TEXT NOT NULL);
            CREATE INDEX IF NOT EXISTS collection_status ON collection(status);
            CREATE INDEX IF NOT EXISTS collection_fingerprint ON collection(sha256);
            PRAGMA user_version=1;")?;
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
            "INSERT OR IGNORE INTO catalogs(sha256,system,json) VALUES (?1,?2,?3)",
            params![calculated, catalog.provenance.system, json],
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
}
