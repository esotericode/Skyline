use rusqlite::{Connection, params};
use skyline_core::{
    catalog::{ImportedCatalog, parse_dat},
    hash::{Cancellation, hash_stream},
    matching::CatalogIndex,
    model::*,
    settings::{AppSettings, CatalogChoice},
    store::Store,
    view::Sort,
};
use std::path::Path;

fn imported() -> ImportedCatalog {
    let snapshot = br#"<datafile><header><name>Migration fixture</name><version>1</version></header><game name="Synthetic"><rom name="synthetic.gb" size="3" sha1="a9993e364706816aba3e25717850c26c9cd0d89d" serial="DMG-XYZ" mia="no" header="!none"/></game></datafile>"#.to_vec();
    let mut catalog = parse_dat(
        &snapshot,
        Path::new("synthetic.dat"),
        "Game Boy",
        &Cancellation::default(),
    )
    .unwrap();
    catalog.provenance.imported_unix_seconds = 42;
    catalog.provenance.license_reference = Some("synthetic provenance fixture".into());
    catalog.provenance.license_review_date = Some("2026-10-12".into());
    ImportedCatalog { catalog, snapshot }
}

fn v1_database(path: &Path, imported: &ImportedCatalog) -> FileRecord {
    let connection = Connection::open(path).unwrap();
    connection.execute_batch("CREATE TABLE snapshots(sha256 TEXT PRIMARY KEY,bytes BLOB NOT NULL);
        CREATE TABLE catalogs(sha256 TEXT NOT NULL REFERENCES snapshots(sha256),system TEXT NOT NULL,json TEXT NOT NULL,PRIMARY KEY(sha256,system));
        CREATE TABLE collection(id INTEGER PRIMARY KEY,path TEXT NOT NULL,status TEXT NOT NULL,sha256 TEXT,json TEXT NOT NULL);
        PRAGMA user_version=1;").unwrap();
    let mut old = imported.catalog.clone();
    old.entries[0].unsupported_algorithms = vec!["serial".into(), "mia".into(), "header".into()];
    let sha = &old.provenance.snapshot_sha256;
    connection
        .execute(
            "INSERT INTO snapshots VALUES (?1,?2)",
            params![sha, imported.snapshot],
        )
        .unwrap();
    connection
        .execute(
            "INSERT INTO catalogs VALUES (?1,?2,?3)",
            params![
                sha,
                old.provenance.system,
                serde_json::to_string(&old).unwrap()
            ],
        )
        .unwrap();
    let (size, digests) = hash_stream(&b"abc"[..], &Cancellation::default(), |_| {}).unwrap();
    let result = CatalogIndex::new(vec![old]).identify(size, &digests);
    assert_eq!(result.status, Verification::Unsupported);
    let record = FileRecord {
        path: "synthetic.gb".into(),
        member: None,
        size: Some(size),
        digests: Some(digests),
        header: None,
        format: None,
        header_check: None,
        result,
    };
    connection
        .execute(
            "INSERT INTO collection(path,status,sha256,json) VALUES (?1,?2,?3,?4)",
            params![
                "synthetic.gb",
                "Unsupported",
                record.digests.as_ref().unwrap().sha256,
                serde_json::to_string(&record).unwrap()
            ],
        )
        .unwrap();
    record
}

#[test]
fn migration_refreshes_cached_entries_preserving_snapshots_provenance_and_history() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("legacy.sqlite3");
    let imported = imported();
    let previous = v1_database(&path, &imported);
    let store = Store::open(&path).unwrap();
    let catalogs = store.catalogs().unwrap();
    assert_eq!(catalogs.len(), 1);
    assert!(catalogs[0].entries[0].unsupported_algorithms.is_empty());
    assert_eq!(
        serde_json::to_value(&catalogs[0].provenance).unwrap(),
        serde_json::to_value(&imported.catalog.provenance).unwrap()
    );
    assert_eq!(
        store
            .snapshot(&imported.catalog.provenance.snapshot_sha256)
            .unwrap(),
        imported.snapshot
    );
    assert_eq!(
        serde_json::to_value(store.collection().unwrap()).unwrap(),
        serde_json::to_value(vec![previous]).unwrap()
    );
    let (_, digests) = hash_stream(&b"abc"[..], &Cancellation::default(), |_| {}).unwrap();
    assert_eq!(
        CatalogIndex::new(catalogs).identify(3, &digests).status,
        Verification::Exact
    );
    assert_eq!(store.settings().unwrap(), AppSettings::default());
    drop(store);
    assert_eq!(Store::open(&path).unwrap().catalogs().unwrap().len(), 1);
    let connection = Connection::open(path).unwrap();
    assert_eq!(
        connection
            .query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        4
    );
}

#[test]
fn failed_refresh_keeps_previous_entries_and_never_blocks_opening() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("broken.sqlite3");
    let imported = imported();
    let previous = v1_database(&path, &imported);
    let connection = Connection::open(&path).unwrap();
    connection
        .execute(
            "UPDATE snapshots SET bytes=?1",
            [b"different snapshot".as_slice()],
        )
        .unwrap();
    drop(connection);
    // A catalog that cannot be re-derived must not lock the user out of their data.
    let store = Store::open(&path).unwrap();
    assert_eq!(store.warnings().len(), 1);
    assert!(store.warnings()[0].contains("previous entries kept"));
    let catalogs = store.catalogs().unwrap();
    assert_eq!(catalogs.len(), 1);
    assert_eq!(
        catalogs[0].entries[0].unsupported_algorithms,
        ["serial", "mia", "header"],
        "the entries saved before the failed refresh are kept as they were"
    );
    assert_eq!(
        serde_json::to_value(store.collection().unwrap()).unwrap(),
        serde_json::to_value(vec![previous]).unwrap()
    );
    drop(store);
    // The stale catalog keeps its old revision so a later open retries the refresh.
    let connection = Connection::open(&path).unwrap();
    assert_eq!(
        connection
            .query_row("SELECT parser_revision FROM catalogs", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        0
    );
    drop(connection);
    let reopened = Store::open(&path).unwrap();
    assert_eq!(reopened.warnings().len(), 1);
    assert_eq!(reopened.catalogs().unwrap()[0].entries.len(), 1);
}

#[test]
fn cancelled_migration_does_not_change_old_schema() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("cancelled.sqlite3");
    v1_database(&path, &imported());
    let cancel = Cancellation::default();
    cancel.cancel();
    assert!(Store::open_cancellable(&path, &cancel).is_err());
    let connection = Connection::open(path).unwrap();
    assert_eq!(
        connection
            .query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        1
    );
    assert_eq!(
        connection
            .query_row("SELECT count(*) FROM catalogs", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        1
    );
}

#[test]
fn settings_round_trip_and_future_optional_fields_preserve_choices() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("settings.sqlite3");
    let mut store = Store::open(&path).unwrap();
    let settings = AppSettings {
        revision: skyline_core::settings::SETTINGS_REVISION,
        dat_path: "references/synthetic.dat".into(),
        dat_folder: "references".into(),
        root_path: "roms/試験".into(),
        exclusions: "Backups\nUnsorted".into(),
        system: "Game Boy Color".into(),
        preferred_catalog: Some(CatalogChoice::from_catalog(&imported().catalog)),
        search: "Synthetic".into(),
        status_filter: Some(Verification::Unknown),
        collection_system: Some("Game Boy Color".into()),
        sort: Sort::Status,
        descending: true,
    };
    store.save_settings(&settings).unwrap();
    drop(store);
    assert_eq!(Store::open(&path).unwrap().settings().unwrap(), settings);
    let connection = Connection::open(&path).unwrap();
    connection
        .execute(
            "UPDATE settings SET json=?1",
            [r#"{"root_path":"roms","future_preference":true}"#],
        )
        .unwrap();
    drop(connection);
    let restored = Store::open(&path).unwrap().settings().unwrap();
    assert_eq!(restored.root_path, "roms");
    assert_eq!(restored.system, "");
    assert_eq!(restored.collection_system, None);
    assert_eq!(restored.sort, Sort::Name);
}

#[test]
fn legacy_game_boy_default_does_not_become_an_import_override() {
    // Saved by the build where `system` was the required label defaulting to Game Boy.
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("legacy-settings.sqlite3");
    drop(Store::open(&path).unwrap());
    let connection = Connection::open(&path).unwrap();
    connection
        .execute(
            "INSERT INTO settings(id,json) VALUES (1,?1)",
            [r#"{"dat_path":"gb.dat","root_path":"roms","exclusions":"","system":"Game Boy","preferred_catalog":null,"search":"pokemon","status_filter":null,"sort":"name","descending":false}"#],
        )
        .unwrap();
    drop(connection);
    let mut store = Store::open(&path).unwrap();
    let restored = store.settings().unwrap();
    assert_eq!(restored.system, "");
    assert_eq!(restored.search, "pokemon");
    assert_eq!(restored.root_path, "roms");
    // A deliberate override chosen after the change is kept.
    let chosen = AppSettings {
        system: "Game Boy".into(),
        ..restored
    };
    store.save_settings(&chosen).unwrap();
    assert_eq!(store.settings().unwrap().system, "Game Boy");
}

#[test]
fn malformed_and_oversized_preferences_do_not_erase_saved_data() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("invalid-settings.sqlite3");
    let mut store = Store::open(&path).unwrap();
    let settings = AppSettings {
        root_path: "keep-me".into(),
        ..Default::default()
    };
    store.save_settings(&settings).unwrap();
    let too_large = AppSettings {
        exclusions: "x".repeat(1024 * 1024),
        ..Default::default()
    };
    assert!(store.save_settings(&too_large).is_err());
    assert_eq!(store.settings().unwrap(), settings);
    drop(store);
    let connection = Connection::open(&path).unwrap();
    connection
        .execute("UPDATE settings SET json='not JSON'", [])
        .unwrap();
    drop(connection);
    assert!(Store::open(&path).unwrap().settings().is_err());
    let connection = Connection::open(path).unwrap();
    assert_eq!(
        connection
            .query_row("SELECT json FROM settings", [], |r| r.get::<_, String>(0))
            .unwrap(),
        "not JSON"
    );
}

#[test]
fn catalog_preferences_survive_reordering_and_include_system_identity() {
    let a = imported().catalog;
    let mut b = a.clone();
    b.provenance.system = "Game Boy Color".into();
    let choice = CatalogChoice::from_catalog(&a);
    assert_eq!(choice.resolve(&[b.clone(), a.clone()]), Some(1));
    assert_eq!(choice.resolve(&[a, b.clone()]), Some(0));
    assert_eq!(choice.resolve(&[b]), None);
}

#[test]
fn newer_parser_revision_is_rejected_without_downgrading_cache() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("future-parser.sqlite3");
    let mut store = Store::open(&path).unwrap();
    store.import_catalog(&imported()).unwrap();
    drop(store);
    let connection = Connection::open(&path).unwrap();
    connection
        .execute("UPDATE catalogs SET parser_revision=999", [])
        .unwrap();
    drop(connection);
    assert!(Store::open(&path).is_err());
    let connection = Connection::open(path).unwrap();
    assert_eq!(
        connection
            .query_row("SELECT parser_revision FROM catalogs", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        999
    );
}

#[test]
fn concurrent_first_opens_serialize_schema_migration() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("concurrent.sqlite3");
    // Make journal-mode setup deterministic before racing the migration transactions.
    let connection = Connection::open(&path).unwrap();
    connection.execute_batch("PRAGMA journal_mode=WAL").unwrap();
    drop(connection);
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(4));
    let workers: Vec<_> = (0..4)
        .map(|_| {
            let path = path.clone();
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                barrier.wait();
                Store::open(&path).map(|store| store.settings().unwrap())
            })
        })
        .collect();
    for worker in workers {
        assert_eq!(worker.join().unwrap().unwrap(), AppSettings::default());
    }
}
