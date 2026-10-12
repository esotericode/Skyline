use crate::{
    catalog::{ImportedCatalog, MAX_DAT_BYTES, PARSER_REVISION, parse_dat, read_path},
    catalog_folder::{IntakeReport, MAX_FOLDER_BYTES, MAX_FOLDER_ENTRIES, discover, folder_key},
    hash::Cancellation,
    matching::CatalogIndex,
    model::*,
    reference,
    settings::AppSettings,
    systems::ReferenceSummary,
};
use anyhow::{Context, Result, bail};
use rusqlite::{
    Connection, OpenFlags, OptionalExtension, Transaction, TransactionBehavior, params,
};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

pub struct Store {
    db: Connection,
    path: PathBuf,
    warnings: Vec<String>,
}

const SCHEMA_VERSION: i64 = 4;
const MAX_SETTINGS_BYTES: usize = 1024 * 1024;
/// Compact after routine pruning once this much is free and it is an eighth of the
/// file; after a migration, which frees per-catalog JSON once, from a lower floor.
const VACUUM_FREE_BYTES: i64 = 64 * 1024 * 1024;
const VACUUM_AFTER_MIGRATION_BYTES: i64 = 16 * 1024 * 1024;

/// Catalogs that participate in scans: current members of a DAT folder (all folders,
/// or only `?1`), plus deliberate manual imports when no folder is selected.
const ACTIVE: &str = "((?1 IS NULL AND EXISTS(SELECT 1 FROM manual_catalogs m WHERE m.sha256=c.sha256 AND m.system=c.system))
    OR EXISTS(SELECT 1 FROM catalog_files f WHERE f.sha256=c.sha256 AND f.system=c.system AND (?1 IS NULL OR f.folder=?1)))";

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
        transaction.execute_batch("CREATE TABLE IF NOT EXISTS manual_catalogs (sha256 TEXT NOT NULL,system TEXT NOT NULL,PRIMARY KEY(sha256,system),FOREIGN KEY(sha256,system) REFERENCES catalogs(sha256,system));
            CREATE TABLE IF NOT EXISTS catalog_folders (folder TEXT PRIMARY KEY,json TEXT NOT NULL);
            CREATE TABLE IF NOT EXISTS catalog_files (folder TEXT NOT NULL REFERENCES catalog_folders(folder),path TEXT NOT NULL,sha256 TEXT NOT NULL,system TEXT NOT NULL,PRIMARY KEY(folder,path),FOREIGN KEY(sha256,system) REFERENCES catalogs(sha256,system));")?;
        if version < 3 {
            transaction.execute(
                "INSERT OR IGNORE INTO manual_catalogs SELECT sha256,system FROM catalogs",
                [],
            )?;
            transaction.execute_batch("ALTER TABLE catalogs ADD COLUMN automatic INTEGER NOT NULL DEFAULT 0;
                ALTER TABLE catalogs ADD COLUMN entry_count INTEGER NOT NULL DEFAULT 0;
                UPDATE catalogs SET automatic=(system=trim(json_extract(json,'$.name'))),entry_count=json_array_length(json_extract(json,'$.entries'));")?;
        }
        if version < 4 {
            // Entries move from per-catalog JSON into indexed rows keyed by a stable id
            // (rowid is not stable across VACUUM for a table without an integer key).
            transaction.execute_batch(
                "ALTER TABLE catalogs ADD COLUMN catalog_id INTEGER;
                UPDATE catalogs SET catalog_id=rowid;",
            )?;
        }
        transaction.execute_batch(
            "CREATE UNIQUE INDEX IF NOT EXISTS catalogs_id ON catalogs(catalog_id);",
        )?;
        transaction.execute_batch(reference::SCHEMA)?;
        let warnings = refresh_catalog_caches(&transaction, cancel)?;
        cancel.check()?;
        transaction.pragma_update(None, "user_version", SCHEMA_VERSION)?;
        transaction.commit()?;
        let store = Self {
            db: connection,
            path: path.to_owned(),
            warnings,
        };
        if version < 4 {
            store.compact(VACUUM_AFTER_MIGRATION_BYTES, 0)?;
        }
        Ok(store)
    }

    /// Catalogs that could not be re-parsed when this build opened the database. They
    /// keep their previous entries; nothing else is blocked.
    pub fn warnings(&self) -> &[String] {
        &self.warnings
    }

    pub fn import_catalog(&mut self, imported: &ImportedCatalog) -> Result<()> {
        let transaction = self.db.transaction()?;
        insert_catalog(&transaction, imported)?;
        transaction.execute(
            "INSERT OR IGNORE INTO manual_catalogs VALUES (?1,?2)",
            params![
                imported.catalog.provenance.snapshot_sha256,
                imported.catalog.provenance.system
            ],
        )?;
        transaction.commit()?;
        Ok(())
    }

    /// Every saved catalog with its entries. Loads everything; for tools and tests.
    pub fn catalogs(&self) -> Result<Vec<Catalog>> {
        self.full_catalogs(
            "SELECT c.catalog_id,c.json FROM catalogs c ORDER BY c.system,c.sha256",
            None,
        )
    }

    /// Active catalogs with their entries. Loads everything; for tools and tests. The
    /// app uses `catalog_infos` and `catalog_index`, which do not.
    pub fn active_catalogs(&self, folder: Option<&Path>) -> Result<Vec<Catalog>> {
        self.full_catalogs(
            &format!("SELECT c.catalog_id,c.json FROM catalogs c WHERE {ACTIVE} ORDER BY c.system,c.sha256"),
            Some(folder),
        )
    }

    fn full_catalogs(&self, sql: &str, folder: Option<Option<&Path>>) -> Result<Vec<Catalog>> {
        let rows: Vec<(i64, String)> = match folder {
            Some(folder) => {
                let key = active_key(folder)?;
                let mut statement = self.db.prepare(sql)?;
                statement
                    .query_map([key], |row| Ok((row.get(0)?, row.get(1)?)))?
                    .collect::<rusqlite::Result<_>>()?
            }
            None => {
                let mut statement = self.db.prepare(sql)?;
                statement
                    .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
                    .collect::<rusqlite::Result<_>>()?
            }
        };
        rows.into_iter()
            .map(|(id, json)| {
                let mut catalog: Catalog = serde_json::from_str(&json)?;
                catalog.entries = reference::entries_of(&self.db, id)?;
                Ok(catalog)
            })
            .collect()
    }

    fn active_infos(&self, folder: Option<&Path>) -> Result<Vec<(i64, CatalogInfo)>> {
        let key = active_key(folder)?;
        let mut statement = self.db.prepare(&format!(
            "SELECT c.catalog_id,c.json,c.entry_count FROM catalogs c WHERE {ACTIVE} ORDER BY c.system,c.sha256"
        ))?;
        let rows = statement.query_map([key], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
            ))
        })?;
        rows.map(|row| {
            let (id, json, entry_count) = row?;
            let catalog: Catalog = serde_json::from_str(&json)?;
            Ok((
                id,
                CatalogInfo {
                    name: catalog.name,
                    version: catalog.version,
                    provenance: catalog.provenance,
                    entry_count: entry_count as usize,
                },
            ))
        })
        .collect()
    }

    /// Metadata of active catalogs, without entries.
    pub fn catalog_infos(&self, folder: Option<&Path>) -> Result<Vec<CatalogInfo>> {
        Ok(self
            .active_infos(folder)?
            .into_iter()
            .map(|(_, info)| info)
            .collect())
    }

    /// What presentation needs from active catalogs, without loading their entries.
    pub fn reference_summary(&self, folder: Option<&Path>) -> Result<ReferenceSummary> {
        Ok(self.catalog_index(folder)?.summary())
    }

    /// An index over active catalogs on its own read-only connection, so a scan can
    /// look entries up while this store saves results.
    pub fn catalog_index(&self, folder: Option<&Path>) -> Result<CatalogIndex> {
        let infos = self.active_infos(folder)?;
        let db = Connection::open_with_flags(
            &self.path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .context("Opening the reference index")?;
        db.busy_timeout(std::time::Duration::from_secs(5))?;
        CatalogIndex::open(db, infos)
    }

    pub fn catalog_folder_report(&self, folder: &Path) -> Result<Option<IntakeReport>> {
        let key = folder_key(folder)?;
        let json: Option<String> = self
            .db
            .query_row(
                "SELECT json FROM catalog_folders WHERE folder=?1",
                [key.to_string_lossy()],
                |row| row.get(0),
            )
            .optional()?;
        json.map(|json| Ok(serde_json::from_str(&json)?))
            .transpose()
    }

    /// One transaction for the entire refresh. Parse failures are per-file and keep
    /// that path's last working snapshot. Cancellation/resource-limit/DB failures
    /// roll back all activation and new snapshots. No collection bytes are touched.
    pub fn sync_catalog_folder(
        &mut self,
        folder: &Path,
        cancel: &Cancellation,
        mut progress: impl FnMut(usize, usize, &Path),
    ) -> Result<IntakeReport> {
        let discovery = discover(folder, cancel)?;
        let mut report = discovery.report;
        let folder = report
            .folder
            .to_str()
            .context("DAT folder must be UTF-8")?
            .to_owned();
        let transaction = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut next: BTreeMap<PathBuf, (String, String)> = {
            let mut statement = transaction
                .prepare("SELECT path,sha256,system FROM catalog_files WHERE folder=?1")?;
            statement
                .query_map([&folder], |row| {
                    Ok((
                        PathBuf::from(row.get::<_, String>(0)?),
                        (row.get(1)?, row.get(2)?),
                    ))
                })?
                .collect::<std::result::Result<_, _>>()?
        };
        let seen: BTreeSet<_> = discovery.files.iter().cloned().collect();
        let mut bytes_read = 0_u64;
        for (position, path) in discovery.files.iter().enumerate() {
            cancel.check()?;
            progress(position, report.discovered, path);
            let bytes = match read_path(path, cancel) {
                Ok(bytes) if path.to_str().is_some() => bytes,
                outcome => {
                    cancel.check()?;
                    if next.contains_key(path) {
                        report.retained += 1;
                    }
                    let explanation = match outcome {
                        Err(error) => format!("{error:#}"),
                        Ok(_) => "Catalog path must be UTF-8".into(),
                    };
                    report.issue(path.clone(), explanation);
                    continue;
                }
            };
            bytes_read += bytes.len() as u64;
            if bytes_read > MAX_FOLDER_BYTES {
                bail!(
                    "DAT folder exceeds {} MiB; previous references retained",
                    MAX_FOLDER_BYTES / (1024 * 1024)
                );
            }
            let sha256 = format!("{:x}", Sha256::digest(&bytes));
            // Cache metadata avoids both XML reparsing and full JSON deserialization.
            // Legacy manual overrides cannot relabel an automatic folder import.
            let cached: Option<String> = transaction
                .query_row(
                    "SELECT system FROM catalogs WHERE sha256=?1 AND automatic=1 LIMIT 1",
                    [&sha256],
                    |row| row.get(0),
                )
                .optional()?;
            let system = if let Some(system) = cached {
                report.cached += 1;
                system
            } else {
                let catalog = match parse_dat(&bytes, path, "", cancel) {
                    Ok(catalog) => catalog,
                    Err(error) => {
                        cancel.check()?;
                        if next.contains_key(path) {
                            report.retained += 1;
                        }
                        report.issue(path.clone(), format!("{error:#}"));
                        continue;
                    }
                };
                let system = catalog.provenance.system.clone();
                // Storage errors are fatal to the batch; only input errors are quiet.
                insert_catalog(
                    &transaction,
                    &ImportedCatalog {
                        catalog,
                        snapshot: bytes,
                    },
                )?;
                report.parsed += 1;
                system
            };
            cancel.check()?;
            next.insert(path.clone(), (sha256, system));
        }
        if discovery.complete {
            next.retain(|path, _| seen.contains(path));
        }
        let unique: BTreeSet<_> = next.values().collect();
        report.catalogs = unique.len();
        for (sha256, system) in unique {
            cancel.check()?;
            let entries: usize = transaction.query_row(
                "SELECT entry_count FROM catalogs WHERE sha256=?1 AND system=?2",
                params![sha256, system],
                |row| row.get(0),
            )?;
            report.entries += entries;
            if report.entries > MAX_FOLDER_ENTRIES {
                bail!(
                    "DAT folder exceeds {MAX_FOLDER_ENTRIES} entries; previous references retained"
                );
            }
        }
        transaction.execute("INSERT INTO catalog_folders(folder,json) VALUES (?1,?2) ON CONFLICT(folder) DO UPDATE SET json=excluded.json", params![folder, serde_json::to_string(&report)?])?;
        transaction.execute("DELETE FROM catalog_files WHERE folder=?1", [&folder])?;
        for (path, (sha256, system)) in next {
            cancel.check()?;
            transaction.execute(
                "INSERT INTO catalog_files VALUES (?1,?2,?3,?4)",
                params![folder, path.to_string_lossy(), sha256, system],
            )?;
        }
        cancel.check()?;
        prune_unused_catalogs(&transaction)?;
        transaction.commit()?;
        self.compact(VACUUM_FREE_BYTES, 8)?;
        progress(report.discovered, report.discovered, &report.folder);
        Ok(report)
    }

    /// Reclaims space freed by pruning or migration once it is large enough to matter:
    /// at least `minimum` bytes and, when `share` is non-zero, 1/`share` of the file.
    fn compact(&self, minimum: i64, share: i64) -> Result<()> {
        let pragma = |name: &str| -> Result<i64> {
            Ok(self
                .db
                .query_row(&format!("PRAGMA {name}"), [], |row| row.get(0))?)
        };
        let page_size = pragma("page_size")?;
        let free = pragma("freelist_count")? * page_size;
        let total = pragma("page_count")? * page_size;
        if free >= minimum && (share == 0 || free * share >= total) {
            self.db.execute_batch("VACUUM")?;
        }
        Ok(())
    }

    /// Transactionally replaces only the last *completed* scan. The caller must never
    /// use this for a cancelled/failed scan. Collection source files are never opened for write.
    pub fn save_completed_scan(&mut self, records: &[FileRecord]) -> Result<()> {
        let transaction = self.db.transaction()?;
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
            .db
            .prepare("SELECT json FROM collection ORDER BY path,id")?;
        let mut result = Vec::new();
        for json in statement.query_map([], |row| row.get::<_, String>(0))? {
            result.push(serde_json::from_str(&json?)?);
        }
        Ok(result)
    }

    pub fn snapshot(&self, sha256: &str) -> Result<Vec<u8>> {
        Ok(self.db.query_row(
            "SELECT bytes FROM snapshots WHERE sha256=?1",
            [sha256],
            |row| row.get(0),
        )?)
    }

    pub fn settings(&self) -> Result<AppSettings> {
        let json: Option<String> = self
            .db
            .query_row("SELECT json FROM settings WHERE id=1", [], |row| row.get(0))
            .optional()?;
        match json {
            Some(json) if json.len() > MAX_SETTINGS_BYTES => {
                bail!("Saved preferences exceed the 1 MiB limit")
            }
            Some(json) => serde_json::from_str::<AppSettings>(&json)
                .map(AppSettings::migrate)
                .context("Saved preferences could not be read"),
            None => Ok(AppSettings::default()),
        }
    }

    pub fn save_settings(&mut self, settings: &AppSettings) -> Result<()> {
        let json = serde_json::to_string(settings)?;
        if json.len() > MAX_SETTINGS_BYTES {
            bail!("Preferences exceed the 1 MiB limit");
        }
        self.db.execute("INSERT INTO settings(id,json) VALUES (1,?1) ON CONFLICT(id) DO UPDATE SET json=excluded.json", [json])?;
        Ok(())
    }
}

fn active_key(folder: Option<&Path>) -> Result<Option<String>> {
    Ok(folder
        .map(folder_key)
        .transpose()?
        .map(|path| path.to_string_lossy().into_owned()))
}

/// Catalog metadata as stored: the parsed catalog without its entries.
fn metadata_json(catalog: &Catalog) -> Result<String> {
    Ok(serde_json::to_string(&Catalog {
        name: catalog.name.clone(),
        version: catalog.version.clone(),
        provenance: catalog.provenance.clone(),
        entries: vec![],
    })?)
}

fn insert_catalog(transaction: &Transaction<'_>, imported: &ImportedCatalog) -> Result<()> {
    let catalog = &imported.catalog;
    let calculated = format!("{:x}", Sha256::digest(&imported.snapshot));
    if calculated != catalog.provenance.snapshot_sha256 {
        bail!("Catalog snapshot fingerprint disagrees");
    }
    transaction.execute(
        "INSERT OR IGNORE INTO snapshots(sha256,bytes) VALUES (?1,?2)",
        params![calculated, imported.snapshot],
    )?;
    let inserted = transaction.execute(
        "INSERT OR IGNORE INTO catalogs(sha256,system,json,parser_revision,automatic,entry_count,catalog_id)
         VALUES (?1,?2,?3,?4,?5,?6,(SELECT COALESCE(MAX(catalog_id),0)+1 FROM catalogs))",
        params![
            calculated,
            catalog.provenance.system,
            metadata_json(catalog)?,
            PARSER_REVISION,
            catalog.provenance.system == catalog.name.trim(),
            catalog.entries.len()
        ],
    )?;
    if inserted == 1 {
        let id: i64 = transaction.query_row(
            "SELECT catalog_id FROM catalogs WHERE sha256=?1 AND system=?2",
            params![calculated, catalog.provenance.system],
            |row| row.get(0),
        )?;
        reference::insert_entries(transaction, id, catalog)?;
    }
    Ok(())
}

/// Removes catalogs nothing uses any more: not in a DAT folder, not a manual import,
/// and not cited by the saved scan's evidence. Typically superseded DAT versions.
fn prune_unused_catalogs(transaction: &Transaction<'_>) -> Result<()> {
    transaction.execute_batch(
        "CREATE TEMP TABLE IF NOT EXISTS cited(sha256 TEXT PRIMARY KEY) WITHOUT ROWID;
        DELETE FROM cited;
        INSERT OR IGNORE INTO cited
            SELECT json_extract(c.value,'$.catalog_sha256') FROM collection, json_each(collection.json,'$.result.candidates') c
            UNION SELECT json_extract(r.value,'$.catalog_sha256') FROM collection, json_each(collection.json,'$.result.rejected') r;
        CREATE TEMP TABLE IF NOT EXISTS unused(catalog_id INTEGER PRIMARY KEY);
        DELETE FROM unused;
        INSERT INTO unused SELECT c.catalog_id FROM catalogs c
            WHERE NOT EXISTS(SELECT 1 FROM catalog_files f WHERE f.sha256=c.sha256 AND f.system=c.system)
            AND NOT EXISTS(SELECT 1 FROM manual_catalogs m WHERE m.sha256=c.sha256 AND m.system=c.system)
            AND c.sha256 NOT IN (SELECT sha256 FROM cited);
        DELETE FROM entries WHERE catalog_id IN (SELECT catalog_id FROM unused);
        DELETE FROM catalog_extensions WHERE catalog_id IN (SELECT catalog_id FROM unused);
        DELETE FROM catalogs WHERE catalog_id IN (SELECT catalog_id FROM unused);
        DELETE FROM snapshots WHERE sha256 NOT IN (SELECT sha256 FROM catalogs);
        DELETE FROM unused;
        DELETE FROM cited;",
    )?;
    Ok(())
}

/// Re-derives entries for catalogs parsed by an older parser revision, from their
/// preserved DAT bytes. A catalog that cannot be refreshed keeps its previous entries
/// and yields a warning; it never prevents the database from opening.
fn refresh_catalog_caches(
    transaction: &Transaction<'_>,
    cancel: &Cancellation,
) -> Result<Vec<String>> {
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
    let keys: Vec<(i64, String, String)> = {
        let mut statement = transaction.prepare(
            "SELECT catalog_id,sha256,system FROM catalogs WHERE parser_revision<?1 ORDER BY sha256,system",
        )?;
        statement
            .query_map([PARSER_REVISION], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?))
            })?
            .collect::<std::result::Result<_, _>>()?
    };
    let mut warnings = vec![];
    for (id, sha256, system) in keys {
        cancel.check()?;
        let json: String = transaction.query_row(
            "SELECT json FROM catalogs WHERE catalog_id=?1",
            [id],
            |row| row.get(0),
        )?;
        let old: Catalog = match serde_json::from_str(&json) {
            Ok(old) => old,
            Err(error) => {
                warnings.push(format!(
                    "{system}: saved catalog unreadable ({error}); left as is"
                ));
                continue;
            }
        };
        match reparse(transaction, &old, &sha256, &system, cancel) {
            Ok(mut refreshed) => {
                // The import's origin, time, publisher and license metadata remain immutable.
                refreshed.provenance = old.provenance;
                reference::delete_entries(transaction, id)?;
                reference::insert_entries(transaction, id, &refreshed)?;
                transaction.execute(
                    "UPDATE catalogs SET json=?1,parser_revision=?2,automatic=?3,entry_count=?4 WHERE catalog_id=?5",
                    params![
                        metadata_json(&refreshed)?,
                        PARSER_REVISION,
                        refreshed.provenance.system == refreshed.name.trim(),
                        refreshed.entries.len(),
                        id
                    ],
                )?;
            }
            Err(error) => {
                cancel.check()?;
                // Entries saved inside older JSON move to the index so the catalog keeps
                // working; the refresh is retried by a later build or open.
                if !old.entries.is_empty() {
                    reference::delete_entries(transaction, id)?;
                    reference::insert_entries(transaction, id, &old)?;
                    transaction.execute(
                        "UPDATE catalogs SET json=?1 WHERE catalog_id=?2",
                        params![metadata_json(&old)?, id],
                    )?;
                }
                warnings.push(format!(
                    "{system}: could not refresh from its saved DAT ({error:#}); previous entries kept"
                ));
            }
        }
    }
    Ok(warnings)
}

fn reparse(
    transaction: &Transaction<'_>,
    old: &Catalog,
    sha256: &str,
    system: &str,
    cancel: &Cancellation,
) -> Result<Catalog> {
    let bytes: Vec<u8> = transaction
        .query_row(
            "SELECT bytes FROM snapshots WHERE sha256=?1",
            [sha256],
            |row| row.get(0),
        )
        .context("The preserved DAT snapshot is missing")?;
    if bytes.len() as u64 > MAX_DAT_BYTES {
        bail!("Preserved DAT exceeds the 64 MiB limit");
    }
    if format!("{:x}", Sha256::digest(&bytes)) != sha256
        || old.provenance.snapshot_sha256 != sha256
        || old.provenance.system != system
    {
        bail!("Catalog snapshot/provenance disagrees");
    }
    parse_dat(&bytes, &old.provenance.origin, system, cancel)
}
