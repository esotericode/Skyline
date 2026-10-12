use skyline_core::{
    catalog::{import_path, parse_dat},
    hash::{Cancellation, hash_stream},
    matching::CatalogIndex,
    model::Verification,
    scan::{ScanOptions, scan_folder},
    settings::{AppSettings, CatalogChoice},
    store::Store,
    systems::{collection_systems, system_family},
    view::{Sort, query_rows_in_family},
};
use std::{path::Path, process::Command};

fn dat(system: &str, filename: &str, bytes: &[u8]) -> String {
    let (_, hashes) = hash_stream(bytes, &Cancellation::default(), |_| {}).unwrap();
    format!(
        "<datafile><header><name>{system}</name><version>1</version></header><game name=\"Synthetic\"><rom name=\"{filename}\" size=\"{}\" sha1=\"{}\" sha256=\"{}\"/></game></datafile>",
        bytes.len(),
        hashes.sha1.unwrap(),
        hashes.sha256.unwrap()
    )
}

#[test]
fn nested_mixed_folder_uses_every_variant_but_shows_only_collection_systems() {
    let temp = tempfile::tempdir().unwrap();
    let dats = temp.path().join("dats");
    let roms = temp.path().join("roms");
    std::fs::create_dir_all(dats.join("nested")).unwrap();
    std::fs::create_dir(&roms).unwrap();
    let gba = dat("Nintendo - Game Boy Advance", "Game.gba", b"GBA");
    std::fs::write(dats.join("gba.DAT"), &gba).unwrap();
    std::fs::write(dats.join("nested/duplicate.xml"), &gba).unwrap();
    std::fs::write(
        dats.join("n64.dat"),
        dat(
            "Nintendo - Nintendo 64 (BigEndian)",
            "Game.z64",
            b"big endian",
        ),
    )
    .unwrap();
    std::fs::write(
        dats.join("nested/swapped.dat"),
        dat(
            "Nintendo - Nintendo 64 (ByteSwapped)",
            "Game.v64",
            b"byte swapped",
        ),
    )
    .unwrap();
    std::fs::write(
        dats.join("3ds.dat"),
        dat("Nintendo - Nintendo 3DS (Encrypted)", "Game.3ds", b"3DS"),
    )
    .unwrap();
    std::fs::write(dats.join("README.txt"), "ignored").unwrap();
    std::fs::write(roms.join("renamed.gba"), b"GBA").unwrap();
    std::fs::write(roms.join("renamed.z64"), b"big endian").unwrap();
    std::fs::write(roms.join("renamed.v64"), b"byte swapped").unwrap();
    let mut store = Store::open(&temp.path().join("state.db")).unwrap();
    let report = store
        .sync_catalog_folder(&dats, &Cancellation::default(), |_, _, _| {})
        .unwrap();
    assert_eq!(
        (
            report.discovered,
            report.parsed,
            report.cached,
            report.catalogs
        ),
        (5, 4, 1, 4)
    );
    let active = store.active_catalogs(Some(&dats)).unwrap();
    let rows = scan_folder(
        &roms,
        &CatalogIndex::new(active.clone()),
        &ScanOptions::default(),
        &Cancellation::default(),
        |_| {},
    )
    .unwrap()
    .records;
    assert!(rows.iter().all(|r| r.result.status == Verification::Exact));
    let systems = collection_systems(&rows, &active);
    assert_eq!(
        systems.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(),
        ["Game Boy Advance", "Nintendo 64"]
    );
    assert_eq!(systems[1].used_catalogs.len(), 2);
    assert_eq!(
        query_rows_in_family(&rows, "", None, Some("Nintendo 64"), Sort::Name, false).len(),
        2
    );
    assert!(
        rows.iter()
            .any(|r| r.result.candidates[0].system.ends_with("(ByteSwapped)"))
    );
    let again = store
        .sync_catalog_folder(&dats, &Cancellation::default(), |_, _, _| {})
        .unwrap();
    assert_eq!((again.parsed, again.cached, again.catalogs), (0, 5, 4));
    assert_eq!(
        std::fs::read(roms.join("renamed.v64")).unwrap(),
        b"byte swapped"
    );
}

#[test]
fn broken_replacement_retains_previous_then_updates_and_removes_without_losing_history() {
    let temp = tempfile::tempdir().unwrap();
    let dats = temp.path().join("dats");
    std::fs::create_dir(&dats).unwrap();
    let path = dats.join("gba.dat");
    std::fs::write(&path, dat("Game Boy Advance", "Game.gba", b"old")).unwrap();
    let database = temp.path().join("state.db");
    let mut store = Store::open(&database).unwrap();
    store
        .sync_catalog_folder(&dats, &Cancellation::default(), |_, _, _| {})
        .unwrap();
    let first = store.active_catalogs(Some(&dats)).unwrap().remove(0);
    std::fs::write(&path, "broken XML").unwrap();
    std::fs::write(dats.join("other.dat"), "also broken").unwrap();
    let bad = store
        .sync_catalog_folder(&dats, &Cancellation::default(), |_, _, _| {})
        .unwrap();
    assert_eq!((bad.retained, bad.issues.len(), bad.catalogs), (1, 2, 1));
    assert_eq!(
        store.active_catalogs(Some(&dats)).unwrap()[0]
            .provenance
            .snapshot_sha256,
        first.provenance.snapshot_sha256
    );
    std::fs::write(&path, dat("Game Boy Advance", "Game.gba", b"new")).unwrap();
    let updated = store
        .sync_catalog_folder(&dats, &Cancellation::default(), |_, _, _| {})
        .unwrap();
    assert_eq!((updated.parsed, updated.catalogs), (1, 1));
    assert_eq!(store.catalogs().unwrap().len(), 2);
    assert!(store.snapshot(&first.provenance.snapshot_sha256).is_ok());
    std::fs::remove_file(&path).unwrap();
    store
        .sync_catalog_folder(&dats, &Cancellation::default(), |_, _, _| {})
        .unwrap();
    assert!(store.active_catalogs(Some(&dats)).unwrap().is_empty());
    assert_eq!(store.catalogs().unwrap().len(), 2);
    assert!(store.active_catalogs(None).unwrap().is_empty());
    drop(store);
    let store = Store::open(&database).unwrap();
    assert!(store.catalog_folder_report(&dats).unwrap().is_some());
}

#[test]
fn cancelled_refresh_rolls_back_new_snapshots_and_activation() {
    let temp = tempfile::tempdir().unwrap();
    let dats = temp.path().join("dats");
    std::fs::create_dir(&dats).unwrap();
    std::fs::write(dats.join("a.dat"), dat("GBA", "A.gba", b"a")).unwrap();
    let mut store = Store::open(&temp.path().join("state.db")).unwrap();
    store
        .sync_catalog_folder(&dats, &Cancellation::default(), |_, _, _| {})
        .unwrap();
    std::fs::write(dats.join("a.dat"), dat("GBA", "B.gba", b"b")).unwrap();
    std::fs::write(dats.join("b.dat"), dat("Nintendo 64", "B.z64", b"b")).unwrap();
    let cancel = Cancellation::default();
    assert!(
        store
            .sync_catalog_folder(&dats, &cancel, |done, _, _| {
                if done == 1 {
                    cancel.cancel();
                }
            })
            .is_err()
    );
    assert_eq!(store.catalogs().unwrap().len(), 1);
    assert_eq!(
        store.active_catalogs(Some(&dats)).unwrap()[0].entries[0].filename,
        "A.gba"
    );
}

#[test]
fn offline_source_keeps_saved_catalogs_and_manual_imports_do_not_override_folder() {
    let temp = tempfile::tempdir().unwrap();
    let dats = temp.path().join("dats");
    std::fs::create_dir(&dats).unwrap();
    let path = dats.join("a.dat");
    std::fs::write(
        &path,
        dat("Nintendo - Nintendo 64 (BigEndian)", "Game.z64", b"a"),
    )
    .unwrap();
    let mut store = Store::open(&temp.path().join("state.db")).unwrap();
    store
        .import_catalog(&import_path(&path, "Game Boy", &Cancellation::default()).unwrap())
        .unwrap();
    let report = store
        .sync_catalog_folder(&dats, &Cancellation::default(), |_, _, _| {})
        .unwrap();
    assert_eq!(report.parsed, 1);
    assert_eq!(store.active_catalogs(Some(&dats)).unwrap().len(), 1);
    assert_eq!(store.active_catalogs(None).unwrap().len(), 2);
    std::fs::rename(&dats, temp.path().join("offline")).unwrap();
    assert!(
        store
            .sync_catalog_folder(&dats, &Cancellation::default(), |_, _, _| {})
            .is_err()
    );
    assert_eq!(store.active_catalogs(Some(&dats)).unwrap().len(), 1);
}

#[test]
fn hints_request_only_missing_relevant_catalogs_and_do_not_verify_or_resolve_ambiguity() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::write(temp.path().join("unknown.gba"), b"unknown").unwrap();
    std::fs::write(temp.path().join("unknown.bin"), b"unknown").unwrap();
    let catalog = parse_dat(
        dat("Nintendo - Nintendo 3DS (Encrypted)", "Game.3ds", b"other").as_bytes(),
        Path::new("synthetic.dat"),
        "",
        &Cancellation::default(),
    )
    .unwrap();
    let rows = scan_folder(
        temp.path(),
        &CatalogIndex::new(vec![catalog.clone()]),
        &ScanOptions::default(),
        &Cancellation::default(),
        |_| {},
    )
    .unwrap()
    .records;
    let systems = collection_systems(&rows, &[catalog]);
    assert_eq!(systems.len(), 1);
    assert_eq!(systems[0].name, "Game Boy Advance");
    assert_eq!(
        (
            systems[0].matched_files,
            systems[0].suggested_files,
            systems[0].available_catalogs
        ),
        (0, 1, 0)
    );
    assert!(
        rows.iter()
            .all(|r| r.result.status == Verification::Unknown && r.result.candidates.is_empty())
    );
    assert_eq!(
        query_rows_in_family(&rows, "", None, Some("Game Boy Advance"), Sort::Name, false).len(),
        1
    );
    assert_ne!(
        system_family("Nintendo - Nintendo 64DD"),
        system_family("Nintendo - Nintendo 64 (BigEndian)")
    );
    let a = parse_dat(
        dat("Nintendo - Nintendo 64 (BigEndian)", "Game.z64", b"same").as_bytes(),
        Path::new("a.dat"),
        "",
        &Cancellation::default(),
    )
    .unwrap();
    let b = parse_dat(
        dat("Nintendo - Nintendo 64 (ByteSwapped)", "Game.v64", b"same").as_bytes(),
        Path::new("b.dat"),
        "",
        &Cancellation::default(),
    )
    .unwrap();
    let (_, hashes) = hash_stream(&b"same"[..], &Cancellation::default(), |_| {}).unwrap();
    let result = CatalogIndex::new(vec![a, b]).identify(4, &hashes);
    assert_eq!(result.status, Verification::Ambiguous);
    assert_eq!(result.candidates.len(), 2);
}

#[test]
fn revision_one_selection_cannot_restrict_new_folder_workflow() {
    let catalog = parse_dat(
        dat("Nintendo 64", "Game.z64", b"a").as_bytes(),
        Path::new("a.dat"),
        "",
        &Cancellation::default(),
    )
    .unwrap();
    let settings = AppSettings {
        revision: 1,
        preferred_catalog: Some(CatalogChoice::from_catalog(&catalog)),
        collection_system: Some("Nintendo - Nintendo 64 (ByteSwapped)".into()),
        root_path: "keep".into(),
        ..Default::default()
    }
    .migrate();
    assert!(settings.preferred_catalog.is_none());
    assert_eq!(settings.collection_system.as_deref(), Some("Nintendo 64"));
    assert_eq!(settings.root_path, "keep");
}

#[test]
fn cli_folder_import_reports_partial_failures_and_can_scan() {
    let temp = tempfile::tempdir().unwrap();
    let dats = temp.path().join("dats");
    let roms = temp.path().join("roms");
    std::fs::create_dir(&dats).unwrap();
    std::fs::create_dir(&roms).unwrap();
    std::fs::write(
        dats.join("a.dat"),
        dat("Game Boy Advance", "Game.gba", b"abc"),
    )
    .unwrap();
    std::fs::write(dats.join("bad.dat"), "bad").unwrap();
    std::fs::write(roms.join("renamed.gba"), b"abc").unwrap();
    let database = temp.path().join("state.db");
    let output = Command::new(env!("CARGO_BIN_EXE_skyline-cli"))
        .arg("import-folder")
        .arg(&database)
        .arg(&dats)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["catalogs"], 1);
    assert_eq!(json["issues"].as_array().unwrap().len(), 1);
    let output = Command::new(env!("CARGO_BIN_EXE_skyline-cli"))
        .arg("scan")
        .arg(&database)
        .arg(&roms)
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(
        Store::open(&database).unwrap().collection().unwrap()[0]
            .result
            .status,
        Verification::Exact
    );
}

#[cfg(unix)]
#[test]
fn dat_discovery_skips_file_and_directory_links_and_rejects_link_roots() {
    use std::os::unix::fs::symlink;
    let temp = tempfile::tempdir().unwrap();
    let dats = temp.path().join("dats");
    std::fs::create_dir(&dats).unwrap();
    std::fs::write(
        temp.path().join("outside.dat"),
        dat("GBA", "Game.gba", b"a"),
    )
    .unwrap();
    symlink(temp.path().join("outside.dat"), dats.join("linked.dat")).unwrap();
    symlink(&dats, dats.join("loop")).unwrap();
    let mut store = Store::open(&temp.path().join("state.db")).unwrap();
    assert_eq!(
        store
            .sync_catalog_folder(&dats, &Cancellation::default(), |_, _, _| {})
            .unwrap()
            .discovered,
        0
    );
    assert!(
        store
            .sync_catalog_folder(&dats.join("loop"), &Cancellation::default(), |_, _, _| {})
            .is_err()
    );
}

#[cfg(unix)]
#[test]
fn unrepresentable_filename_is_a_quiet_issue_not_a_failed_batch() {
    use std::os::unix::ffi::OsStringExt;
    let temp = tempfile::tempdir().unwrap();
    let dats = temp.path().join("dats");
    std::fs::create_dir(&dats).unwrap();
    std::fs::write(dats.join("good.dat"), dat("GBA", "Game.gba", b"a")).unwrap();
    let bad_name = std::ffi::OsString::from_vec(b"bad-\xff.dat".to_vec());
    std::fs::write(dats.join(bad_name), dat("Other", "Game.rom", b"b")).unwrap();
    let mut store = Store::open(&temp.path().join("state.db")).unwrap();
    let report = store
        .sync_catalog_folder(&dats, &Cancellation::default(), |_, _, _| {})
        .unwrap();
    assert_eq!((report.catalogs, report.issues.len()), (1, 1));
    assert!(store.catalog_folder_report(&dats).unwrap().is_some());
}

#[test]
fn storage_failure_rolls_back_entire_batch_instead_of_treating_it_as_bad_xml() {
    let temp = tempfile::tempdir().unwrap();
    let dats = temp.path().join("dats");
    std::fs::create_dir(&dats).unwrap();
    std::fs::write(dats.join("a.dat"), dat("GBA", "A.gba", b"a")).unwrap();
    std::fs::write(dats.join("b.dat"), dat("Nintendo 64", "B.z64", b"b")).unwrap();
    let database = temp.path().join("state.db");
    let mut store = Store::open(&database).unwrap();
    rusqlite::Connection::open(&database).unwrap().execute_batch("CREATE TRIGGER injected_failure BEFORE INSERT ON catalogs WHEN NEW.system='Nintendo 64' BEGIN SELECT RAISE(ABORT,'synthetic storage failure'); END;").unwrap();
    assert!(
        store
            .sync_catalog_folder(&dats, &Cancellation::default(), |_, _, _| {})
            .is_err()
    );
    assert!(store.catalogs().unwrap().is_empty());
    assert!(store.catalog_folder_report(&dats).unwrap().is_none());
}

#[test]
fn oversized_input_is_quiet_and_discovery_limit_retains_previous_activation() {
    let temp = tempfile::tempdir().unwrap();
    let dats = temp.path().join("dats");
    std::fs::create_dir(&dats).unwrap();
    std::fs::write(dats.join("good.dat"), dat("GBA", "Game.gba", b"a")).unwrap();
    let file = std::fs::File::create(dats.join("huge.dat")).unwrap();
    file.set_len(skyline_core::catalog::MAX_DAT_BYTES + 1)
        .unwrap();
    let mut store = Store::open(&temp.path().join("state.db")).unwrap();
    let report = store
        .sync_catalog_folder(&dats, &Cancellation::default(), |_, _, _| {})
        .unwrap();
    assert_eq!((report.catalogs, report.issues.len()), (1, 1));
    assert!(report.issues[0].explanation.contains("64 MiB"));
    for i in 0..skyline_core::catalog_folder::MAX_FOLDER_FILES {
        std::fs::write(dats.join(format!("extra-{i}.dat")), "bad").unwrap();
    }
    assert!(
        store
            .sync_catalog_folder(&dats, &Cancellation::default(), |_, _, _| {})
            .is_err()
    );
    assert_eq!(store.active_catalogs(Some(&dats)).unwrap().len(), 1);
}

#[test]
fn all_fresh1g1r_names_and_a_future_system_are_catalog_driven() {
    // Names only, pinned source f54f00d; payloads and all reference entries synthetic.
    // This fixture is a compatibility inventory, never a runtime allowlist.
    let mut names: Vec<String> =
        serde_json::from_str(include_str!("fixtures/fresh1g1r_system_names.json")).unwrap();
    assert_eq!(names.len(), 250);
    names.push("Future Vendor - Unknown Console (New Layout)".into());
    let temp = tempfile::tempdir().unwrap();
    let dats = temp.path().join("dats");
    let roms = temp.path().join("roms");
    std::fs::create_dir(&dats).unwrap();
    std::fs::create_dir(&roms).unwrap();
    for (id, name) in names.iter().enumerate() {
        let bytes = format!("synthetic system {id}");
        let filename = format!("Game.custom{id}");
        let xml = dat(&name.replace('&', "&amp;"), &filename, bytes.as_bytes());
        std::fs::write(dats.join(format!("{id}.dat")), xml).unwrap();
        std::fs::write(roms.join(filename), bytes).unwrap();
    }
    let mut store = Store::open(&temp.path().join("state.db")).unwrap();
    let report = store
        .sync_catalog_folder(&dats, &Cancellation::default(), |_, _, _| {})
        .unwrap();
    assert_eq!(report.catalogs, 251);
    assert!(report.issues.is_empty());
    let index = CatalogIndex::new(store.active_catalogs(Some(&dats)).unwrap());
    let rows = scan_folder(
        &roms,
        &index,
        &ScanOptions::default(),
        &Cancellation::default(),
        |_| {},
    )
    .unwrap()
    .records;
    assert_eq!(rows.len(), 251);
    assert!(
        rows.iter()
            .all(|row| row.result.status == Verification::Exact)
    );
    assert!(
        rows.iter()
            .any(|row| row.result.candidates[0].system == "Toshiba - Visicom")
    );
    assert!(rows.iter().any(
        |row| row.result.candidates[0].system == "Future Vendor - Unknown Console (New Layout)"
    ));
}

#[test]
fn empty_catalog_and_unspecified_sizes_are_valid_metadata_not_false_matches() {
    let empty = parse_dat(
        b"<datafile><header><name>Empty new system</name></header></datafile>",
        Path::new("empty.dat"),
        "",
        &Cancellation::default(),
    )
    .unwrap();
    assert!(empty.entries.is_empty());
    let bytes = dat("New system", "Game.custom", b"abc").replace("size=\"3\"", "size=\"\"");
    let catalog = parse_dat(
        bytes.as_bytes(),
        Path::new("size.dat"),
        "",
        &Cancellation::default(),
    )
    .unwrap();
    assert!(catalog.entries[0].size.is_none());
    let (_, hashes) = hash_stream(&b"abc"[..], &Cancellation::default(), |_| {}).unwrap();
    assert_eq!(
        CatalogIndex::new(vec![empty, catalog])
            .identify(3, &hashes)
            .status,
        Verification::Exact
    );
    // One unspecified extensionless entry must not enable every LICENSE/README.
    let bare = dat("New system", "cetk", b"abc").replace("size=\"3\"", "size=\"\"");
    let catalog = parse_dat(
        bare.as_bytes(),
        Path::new("bare.dat"),
        "",
        &Cancellation::default(),
    )
    .unwrap();
    let index = CatalogIndex::new(vec![catalog]);
    assert!(index.accepts_loose_file(Path::new("cetk"), 3));
    assert!(!index.accepts_loose_file(Path::new("LICENSE"), 3));
}

#[test]
fn unknown_future_extension_gets_a_labelled_hint_and_filter_without_verification() {
    use skyline_core::view::query_rows_in_collection_system;
    let temp = tempfile::tempdir().unwrap();
    std::fs::write(temp.path().join("Unknown.weirdrom"), b"other").unwrap();
    let catalog = parse_dat(
        dat("New Vendor - Console (Variant)", "Game.weirdrom", b"abc").as_bytes(),
        Path::new("new.dat"),
        "",
        &Cancellation::default(),
    )
    .unwrap();
    let index = CatalogIndex::new(vec![catalog]);
    let rows = scan_folder(
        temp.path(),
        &index,
        &ScanOptions::default(),
        &Cancellation::default(),
        |_| {},
    )
    .unwrap()
    .records;
    let systems = index.collection_systems(&rows);
    assert_eq!(systems[0].name, "New Vendor - Console");
    assert_eq!(systems[0].suggested_files, 1);
    assert_eq!(
        query_rows_in_collection_system(&rows, "", None, Some(&systems[0]), Sort::Name, false),
        [0]
    );
    assert_eq!(rows[0].result.status, Verification::Unknown);
    assert!(rows[0].result.candidates.is_empty());
}
