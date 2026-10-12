use skyline_core::{
    catalog::{ImportedCatalog, parse_dat},
    hash::{Cancellation, hash_file, hash_stream},
    matching::CatalogIndex,
    model::*,
    scan::{ScanOptions, scan_folder},
    store::Store,
};
use std::{
    io::{self, Read},
    path::{Path, PathBuf},
};

fn hashes() -> Digests {
    hash_stream(&b"abc"[..], &Cancellation::default(), |_| {})
        .unwrap()
        .1
}

fn dat(roms: &str) -> Vec<u8> {
    format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?><!DOCTYPE datafile SYSTEM \"https://example.invalid/not-requested.dtd\"><datafile><header><name>Synthetic &amp; local</name><version>test-1</version></header>{roms}</datafile>").into_bytes()
}

fn rom(name: &str, attributes: &str) -> String {
    format!("<game name=\"{name}\"><rom name=\"{name}.gb\" {attributes}/></game>")
}

fn catalog(bytes: &[u8]) -> Catalog {
    parse_dat(
        bytes,
        Path::new("synthetic.dat"),
        "Game Boy",
        &Cancellation::default(),
    )
    .unwrap()
}

fn identify(attributes: &str) -> MatchResult {
    CatalogIndex::new(vec![catalog(&dat(&rom("Synthetic", attributes)))]).identify(3, &hashes())
}

#[test]
fn streaming_hashes_match_known_test_vector() {
    let h = hashes();
    assert_eq!(h.crc32.as_deref(), Some("352441c2"));
    assert_eq!(h.md5.as_deref(), Some("900150983cd24fb0d6963f7d28e17f72"));
    assert_eq!(
        h.sha1.as_deref(),
        Some("a9993e364706816aba3e25717850c26c9cd0d89d")
    );
    assert_eq!(
        h.sha256.as_deref(),
        Some("ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad")
    );
}

#[test]
fn exact_checks_all_supplied_digests_and_preserves_suspect_flag() {
    let matched = identify(
        "size=\"3\" crc=\"352441C2\" md5=\"900150983cd24fb0d6963f7d28e17f72\" sha1=\"a9993e364706816aba3e25717850c26c9cd0d89d\" status=\"baddump\"",
    );
    assert_eq!(matched.status, Verification::Exact);
    assert_eq!(matched.candidates[0].checked_algorithms.len(), 3);
    assert_eq!(matched.candidates[0].entry.quality, DumpQuality::Suspect);
    assert_eq!(matched.candidates[0].entry.raw_status, "baddump");
}

#[test]
fn contradictory_digest_and_size_prevent_full_match() {
    for attributes in [
        "size=\"4\" crc=\"352441c2\"",
        "size=\"3\" crc=\"352441c2\" sha1=\"0000000000000000000000000000000000000000\"",
    ] {
        let matched = identify(attributes);
        assert_eq!(matched.status, Verification::Unknown);
        assert!(matched.candidates.is_empty());
        assert_eq!(matched.rejected.len(), 1);
        assert!(!matched.rejected[0].failures.is_empty());
    }
}

#[test]
fn crc_only_is_reduced_confidence_and_nodump_never_verifies() {
    assert_eq!(
        identify("size=\"3\" crc=\"352441c2\"").status,
        Verification::CrcOnly
    );
    assert_eq!(
        identify("size=\"3\" crc=\"352441c2\" status=\"nodump\"").status,
        Verification::Unknown
    );
    assert_eq!(identify("size=\"3\"").status, Verification::Unknown);
}

#[test]
fn ambiguous_releases_and_catalog_conflicts_are_retained() {
    let bytes =
        dat(&(rom("Release A", "crc=\"352441c2\"") + &rom("Release B", "crc=\"352441c2\"")));
    let matched = CatalogIndex::new(vec![catalog(&bytes)]).identify(3, &hashes());
    assert_eq!(matched.status, Verification::Ambiguous);
    assert_eq!(matched.candidates.len(), 2);
    let a = catalog(&dat(&rom("A", "crc=\"352441c2\"")));
    let b = catalog(&dat(&rom("B", "crc=\"352441c2\"")));
    assert_eq!(
        CatalogIndex::new(vec![a, b]).identify(3, &hashes()).status,
        Verification::Ambiguous
    );
}

#[test]
fn revisions_of_same_identity_retain_both_sources() {
    let a = catalog(&dat(&rom("A", "crc=\"352441c2\"")));
    let mut b = a.clone();
    b.version = "test-2".into();
    b.provenance.snapshot_sha256 = "different snapshot".into();
    let matched = CatalogIndex::new(vec![a, b]).identify(3, &hashes());
    assert_eq!(matched.status, Verification::CrcOnly);
    assert_eq!(matched.candidates.len(), 2);
}

#[test]
fn multi_file_and_unsupported_algorithm_candidates_are_inspection_only() {
    let bytes = dat(
        "<game name=\"Set\"><rom name=\"a.gb\" crc=\"352441c2\"/><rom name=\"b.gb\" crc=\"00000000\"/></game>",
    );
    assert_eq!(
        CatalogIndex::new(vec![catalog(&bytes)])
            .identify(3, &hashes())
            .status,
        Verification::Unsupported
    );
    assert_eq!(
        identify("crc=\"352441c2\" sha512=\"unimplemented\"").status,
        Verification::Unsupported
    );
}

#[test]
fn snapshot_provenance_uses_exact_bytes_and_unescapes_header() {
    let bytes = dat(&rom("Fixture", "crc=\"352441c2\""));
    let a = catalog(&bytes);
    let mut changed = bytes.clone();
    changed.extend_from_slice(b"\n");
    let b = catalog(&changed);
    assert_eq!(a.name, "Synthetic & local");
    assert_ne!(a.provenance.snapshot_sha256, b.provenance.snapshot_sha256);
    assert!(a.provenance.license_reference.is_none());
}

#[test]
fn malformed_xml_and_unsupported_schemas_fail_closed() {
    let bad = [
        "<datafile><header><name>A</name></header><game name=\"A\"><rom name=\"a\" crc=\"xyz\"/></game></datafile>",
        "<datafile><header><name>A</name></header><game name=\"A\"><rom name=\"a\" crc=\"352441c2\"/></game>",
        "<datafile></datafile><datafile/>",
        "<softwarelist><software name=\"a\"/></softwarelist>",
        "<!DOCTYPE datafile [<!ENTITY xxe SYSTEM \"file:///private\">]><datafile/>",
        "<datafile><header><name>&unrecognized;</name></header></datafile>",
        "<?xml version=\"1.0\" encoding=\"ISO-8859-1\"?><datafile/>",
    ];
    for xml in bad {
        assert!(
            parse_dat(
                xml.as_bytes(),
                Path::new("fixture"),
                "Game Boy",
                &Cancellation::default()
            )
            .is_err(),
            "accepted {xml}"
        );
    }
}

#[test]
fn nesting_and_attribute_limits_are_enforced() {
    let xml = format!("<datafile>{}</datafile>", "<extra>".repeat(40));
    assert!(
        parse_dat(
            xml.as_bytes(),
            Path::new("fixture"),
            "Game Boy",
            &Cancellation::default()
        )
        .is_err()
    );
    let xml = dat(&rom(&"x".repeat(9000), "crc=\"352441c2\""));
    assert!(
        parse_dat(
            &xml,
            Path::new("fixture"),
            "Game Boy",
            &Cancellation::default()
        )
        .is_err()
    );
}

#[test]
fn cancellation_and_read_errors_are_not_matches() {
    let cancel = Cancellation::default();
    let data = vec![0; 200_000];
    assert!(hash_stream(&data[..], &cancel, |_| cancel.cancel()).is_err());
    struct Broken;
    impl Read for Broken {
        fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
            Err(io::Error::other("injected read failure"))
        }
    }
    assert!(hash_stream(Broken, &Cancellation::default(), |_| {}).is_err());
    let cancel = Cancellation::default();
    cancel.cancel();
    assert!(
        parse_dat(
            &dat(&rom("A", "crc=\"352441c2\"")),
            Path::new("fixture"),
            "Game Boy",
            &cancel
        )
        .is_err()
    );
}

#[test]
fn recursive_scan_preserves_sources_excludes_prefixes_and_records_unsupported() {
    let directory = tempfile::tempdir().unwrap();
    std::fs::create_dir(directory.path().join("skip")).unwrap();
    std::fs::create_dir(directory.path().join("nested")).unwrap();
    for path in ["known.gb", "nested/duplicate.GBC", "skip/excluded.gb"] {
        std::fs::write(directory.path().join(path), b"abc").unwrap();
    }
    std::fs::write(directory.path().join("unknown.gb"), b"xyz").unwrap();
    std::fs::write(directory.path().join("archive.zip"), b"not actually a zip").unwrap();
    let index = CatalogIndex::new(vec![catalog(&dat(&rom(
        "Synthetic",
        "size=\"3\" sha1=\"a9993e364706816aba3e25717850c26c9cd0d89d\"",
    )))]);
    let report = scan_folder(
        directory.path(),
        &index,
        &ScanOptions {
            exclusions: vec!["skip".into()],
        },
        &Cancellation::default(),
        |_| {},
    )
    .unwrap();
    assert!(!report.cancelled);
    assert_eq!(report.records.len(), 4);
    assert_eq!(
        report
            .records
            .iter()
            .filter(|r| r.result.status == Verification::Exact)
            .count(),
        2
    );
    assert_eq!(
        report
            .records
            .iter()
            .filter(|r| r.result.status == Verification::Unknown)
            .count(),
        1
    );
    assert_eq!(
        report
            .records
            .iter()
            .filter(|r| r.result.status == Verification::Unsupported)
            .count(),
        1
    );
    for path in ["known.gb", "nested/duplicate.GBC", "skip/excluded.gb"] {
        assert_eq!(std::fs::read(directory.path().join(path)).unwrap(), b"abc");
    }
    assert_eq!(report.hashed_bytes, 9);
}

#[test]
fn cancelled_scan_keeps_rom_bytes_and_rejects_unsafe_exclusions() {
    let directory = tempfile::tempdir().unwrap();
    let bytes = vec![42; 200_000];
    let path = directory.path().join("synthetic.gb");
    std::fs::write(&path, &bytes).unwrap();
    let cancel = Cancellation::default();
    let index = CatalogIndex::new(vec![]);
    let report = scan_folder(
        directory.path(),
        &index,
        &ScanOptions::default(),
        &cancel,
        |_| cancel.cancel(),
    )
    .unwrap();
    assert!(report.cancelled);
    assert!(report.records.is_empty());
    assert_eq!(std::fs::read(path).unwrap(), bytes);
    assert!(
        scan_folder(
            directory.path(),
            &index,
            &ScanOptions {
                exclusions: vec!["../escape".into()]
            },
            &Cancellation::default(),
            |_| {}
        )
        .is_err()
    );
    assert!(
        scan_folder(
            &directory.path().join("missing"),
            &index,
            &ScanOptions::default(),
            &Cancellation::default(),
            |_| {}
        )
        .is_err()
    );
}

#[cfg(unix)]
#[test]
fn symlinks_are_not_followed_or_hashed() {
    let directory = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    std::fs::write(outside.path().join("a.gb"), b"abc").unwrap();
    std::os::unix::fs::symlink(outside.path(), directory.path().join("linked")).unwrap();
    std::os::unix::fs::symlink(
        outside.path().join("a.gb"),
        directory.path().join("linked.gb"),
    )
    .unwrap();
    let report = scan_folder(
        directory.path(),
        &CatalogIndex::new(vec![]),
        &ScanOptions::default(),
        &Cancellation::default(),
        |_| {},
    )
    .unwrap();
    assert_eq!(report.records.len(), 2);
    assert_eq!(report.hashed_bytes, 0);
    assert!(
        report
            .records
            .iter()
            .all(|r| r.result.status == Verification::Unsupported)
    );
}

#[test]
fn database_roundtrip_retains_immutable_snapshot_and_completed_collection() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("skyline.sqlite3");
    let bytes = dat(&rom("A", "crc=\"352441c2\""));
    let catalog = catalog(&bytes);
    let imported = ImportedCatalog {
        catalog: catalog.clone(),
        snapshot: bytes.clone(),
    };
    let mut store = Store::open(&path).unwrap();
    store.import_catalog(&imported).unwrap();
    store.import_catalog(&imported).unwrap();
    assert_eq!(store.catalogs().unwrap().len(), 1);
    assert_eq!(
        store.snapshot(&catalog.provenance.snapshot_sha256).unwrap(),
        bytes
    );
    let record = FileRecord {
        path: PathBuf::from("synthetic.gb"),
        size: Some(3),
        digests: Some(hashes()),
        result: identify("crc=\"352441c2\""),
    };
    store.save_completed_scan(&[record]).unwrap();
    drop(store);
    let mut store = Store::open(&path).unwrap();
    assert_eq!(store.collection().unwrap().len(), 1);
    let corrupted = ImportedCatalog {
        catalog,
        snapshot: b"different".to_vec(),
    };
    assert!(store.import_catalog(&corrupted).is_err());
    assert_eq!(store.catalogs().unwrap().len(), 1);
    assert_eq!(
        store.collection().unwrap()[0].result.status,
        Verification::CrcOnly
    );
}

#[test]
fn newer_database_schema_is_rejected_without_downgrade() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("newer.sqlite3");
    let connection = rusqlite::Connection::open(&path).unwrap();
    connection.execute_batch("PRAGMA user_version=999").unwrap();
    drop(connection);
    assert!(Store::open(&path).is_err());
    let connection = rusqlite::Connection::open(path).unwrap();
    assert_eq!(
        connection
            .query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        999
    );
}

#[test]
fn source_change_during_hashing_prevents_saved_evidence() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("changing.gb");
    std::fs::write(&path, b"abc").unwrap();
    let mut changed = false;
    let result = hash_file(&path, &Cancellation::default(), |_| {
        if !changed {
            std::fs::write(&path, b"changed length").unwrap();
            changed = true;
        }
    });
    assert!(result.is_err());
}

#[test]
fn unknown_rom_attributes_do_not_silently_claim_supported_verification() {
    let result = identify("crc=\"352441c2\" crc64=\"unimplemented\"");
    assert_eq!(result.status, Verification::Unsupported);
    assert_eq!(result.candidates[0].entry.unsupported_algorithms, ["crc64"]);
    let result = identify("crc=\"352441c2\" status=\"unrecognized-provider-flag\"");
    assert_eq!(result.candidates[0].entry.quality, DumpQuality::Unspecified);
    assert_eq!(
        result.candidates[0].entry.raw_status,
        "unrecognized-provider-flag"
    );
}

#[test]
fn view_search_filter_and_sort_use_original_and_reference_names() {
    use skyline_core::view::{Sort, query_rows};
    let row = |path: &str, size: u64, result: MatchResult| FileRecord {
        path: path.into(),
        size: Some(size),
        digests: Some(hashes()),
        result,
    };
    let rows = vec![
        row("z.gb", 3, identify("crc=\"352441c2\"")),
        row("a.gb", 5, CatalogIndex::new(vec![]).identify(3, &hashes())),
    ];
    assert_eq!(query_rows(&rows, "", None, Sort::Name, false), [1, 0]);
    assert_eq!(query_rows(&rows, "", None, Sort::Size, true), [1, 0]);
    assert_eq!(query_rows(&rows, "SYNTHETIC", None, Sort::Name, false), [0]);
    assert_eq!(
        query_rows(&rows, "", Some(&Verification::Unknown), Sort::Name, false),
        [1]
    );
    assert!(query_rows(&rows, "missing", None, Sort::Name, false).is_empty());
}

#[test]
fn cli_end_to_end_import_scan_and_version() {
    use std::process::Command;
    let directory = tempfile::tempdir().unwrap();
    let database = directory.path().join("cli.sqlite3");
    let reference = directory.path().join("reference.dat");
    let roms = directory.path().join("roms");
    std::fs::create_dir(&roms).unwrap();
    std::fs::write(
        &reference,
        dat(&rom(
            "Synthetic",
            "size=\"3\" sha1=\"a9993e364706816aba3e25717850c26c9cd0d89d\"",
        )),
    )
    .unwrap();
    let source = roms.join("original.gb");
    std::fs::write(&source, b"abc").unwrap();
    let binary = env!("CARGO_BIN_EXE_skyline-cli");
    let version = Command::new(binary).arg("--version").output().unwrap();
    assert!(version.status.success());
    assert_eq!(
        String::from_utf8(version.stdout).unwrap().trim(),
        format!("Skyline {}", skyline_core::VERSION)
    );
    let imported = Command::new(binary)
        .arg("import")
        .arg(&database)
        .arg("Game Boy")
        .arg(&reference)
        .output()
        .unwrap();
    assert!(
        imported.status.success(),
        "{}",
        String::from_utf8_lossy(&imported.stderr)
    );
    let scanned = Command::new(binary)
        .arg("scan")
        .arg(&database)
        .arg(&roms)
        .output()
        .unwrap();
    assert!(
        scanned.status.success(),
        "{}",
        String::from_utf8_lossy(&scanned.stderr)
    );
    let records: Vec<FileRecord> = serde_json::from_slice(&scanned.stdout).unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].result.status, Verification::Exact);
    assert_eq!(std::fs::read(source).unwrap(), b"abc");
    assert_eq!(
        Store::open(&database).unwrap().collection().unwrap().len(),
        1
    );
}

#[test]
fn descriptive_rom_attributes_do_not_block_verification() {
    // No-Intro/Datoso DATs carry serial/mia metadata on ordinary single-file entries.
    let result = identify(
        "size=\"3\" sha1=\"a9993e364706816aba3e25717850c26c9cd0d89d\" status=\"verified\" serial=\"DMG-XYZ\" mia=\"no\"",
    );
    assert_eq!(result.status, Verification::Exact);
    assert!(result.candidates[0].entry.unsupported_algorithms.is_empty());
    // A copier header changes which bytes the digests describe; stay fail-closed.
    let result = identify("size=\"3\" crc=\"352441c2\" header=\"4E 45 53 1A\"");
    assert_eq!(result.status, Verification::Unsupported);
}

#[test]
fn rejected_size_evidence_reads_as_plain_numbers() {
    let result = identify("size=\"4\" crc=\"352441c2\"");
    assert_eq!(
        result.rejected[0].failures[0],
        "Size: expected 4, calculated 3"
    );
}

#[test]
fn view_sorts_names_case_insensitively_and_status_by_strength() {
    use skyline_core::view::{Sort, query_rows};
    let row = |path: &str, result: MatchResult| FileRecord {
        path: path.into(),
        size: Some(3),
        digests: Some(hashes()),
        result,
    };
    let unknown = || CatalogIndex::new(vec![]).identify(3, &hashes());
    let rows = vec![
        row("b.gb", unknown()),
        row("A.gb", unknown()),
        row("c.gb", identify("crc=\"352441c2\"")),
        row(
            "d.gb",
            identify("size=\"3\" sha1=\"a9993e364706816aba3e25717850c26c9cd0d89d\""),
        ),
    ];
    assert_eq!(query_rows(&rows, "", None, Sort::Name, false), [1, 0, 2, 3]);
    assert_eq!(
        query_rows(&rows, "", None, Sort::Status, false),
        [3, 2, 1, 0]
    );
}

#[test]
fn display_helpers_are_human_readable() {
    use skyline_core::view::{display_path, format_size, format_utc};
    assert_eq!(
        display_path(Path::new(r"\\?\C:\ROMs\a.gb")),
        r"C:\ROMs\a.gb"
    );
    assert_eq!(
        display_path(Path::new(r"\\?\UNC\nas\share\a.gb")),
        r"\\nas\share\a.gb"
    );
    assert_eq!(
        display_path(Path::new("/home/roms/a.gb")),
        "/home/roms/a.gb"
    );
    assert_eq!(format_size(512), "512 B");
    assert_eq!(format_size(65_536), "64.0 KiB");
    assert_eq!(format_size(1_048_576), "1.0 MiB");
    assert_eq!(format_utc(0), "1970-01-01 00:00 UTC");
    assert_eq!(format_utc(951_782_400), "2000-02-29 00:00 UTC");
    assert_eq!(format_utc(1_791_820_799), "2026-10-12 15:59 UTC");
}

#[cfg(windows)]
#[test]
fn windows_junctions_are_skipped_without_reading_targets() {
    let selected = tempfile::tempdir().unwrap();
    let target = tempfile::tempdir().unwrap();
    std::fs::write(target.path().join("synthetic.gb"), b"abc").unwrap();
    let junction = selected.path().join("junction");
    let created = std::process::Command::new("cmd")
        .args(["/C", "mklink", "/J"])
        .arg(&junction)
        .arg(target.path())
        .output()
        .unwrap();
    assert!(
        created.status.success(),
        "{}",
        String::from_utf8_lossy(&created.stderr)
    );
    assert!(skyline_core::hash::is_link(
        &std::fs::symlink_metadata(&junction).unwrap()
    ));
    let index = CatalogIndex::new(vec![]);
    let report = scan_folder(
        selected.path(),
        &index,
        &ScanOptions::default(),
        &Cancellation::default(),
        |_| {},
    )
    .unwrap();
    assert_eq!(report.records.len(), 1);
    assert_eq!(report.hashed_bytes, 0);
    assert_eq!(report.records[0].result.status, Verification::Unsupported);
    assert!(
        scan_folder(
            &junction,
            &index,
            &ScanOptions::default(),
            &Cancellation::default(),
            |_| {}
        )
        .is_err()
    );
    std::fs::remove_dir(&junction).unwrap();
    assert_eq!(
        std::fs::read(target.path().join("synthetic.gb")).unwrap(),
        b"abc"
    );
}
