use skyline_core::{
    catalog::{ImportedCatalog, parse_dat},
    hash::{Cancellation, hash_stream},
    matching::CatalogIndex,
    model::{Catalog, Digests, FileRecord, Verification},
    scan::{ScanOptions, scan_folder},
    store::Store,
    view::{Sort, query_rows, query_rows_in_system},
};
use std::{path::Path, process::Command};

fn digests(bytes: &[u8]) -> Digests {
    hash_stream(bytes, &Cancellation::default(), |_| {})
        .unwrap()
        .1
}

fn xml(system: &str, filename: &str, bytes: &[u8]) -> Vec<u8> {
    let h = digests(bytes);
    format!(
        "<datafile><header><name>{system}</name><version>synthetic-1</version></header>\
         <game name=\"Synthetic release\"><rom name=\"{filename}\" size=\"{}\" crc=\"{}\" md5=\"{}\" sha1=\"{}\" sha256=\"{}\"/></game></datafile>",
        bytes.len(), h.crc32.unwrap(), h.md5.unwrap(), h.sha1.unwrap(), h.sha256.unwrap()
    ).into_bytes()
}

fn catalog(system: &str, filename: &str, bytes: &[u8]) -> Catalog {
    parse_dat(
        &xml(system, filename, bytes),
        Path::new("synthetic.dat"),
        "",
        &Cancellation::default(),
    )
    .unwrap()
}

fn scan(root: &Path, catalogs: Vec<Catalog>) -> Vec<FileRecord> {
    scan_folder(
        root,
        &CatalogIndex::new(catalogs),
        &ScanOptions::default(),
        &Cancellation::default(),
        |_| {},
    )
    .unwrap()
    .records
}

#[test]
fn mixed_cartridge_systems_match_all_digests_without_filename_inference_or_writes() {
    let representations = [
        ("NES", "nes"),
        ("SNES", "sfc"),
        ("SNES", "smc"),
        ("SNES", "swc"),
        ("SNES", "fig"),
        ("Nintendo 64", "n64"),
        ("Nintendo 64", "z64"),
        ("Nintendo 64", "v64"),
        ("Game Boy", "gb"),
        ("Game Boy Color", "gbc"),
        ("Game Boy Advance", "gba"),
        ("Nintendo DS", "nds"),
        ("Virtual Boy", "vb"),
        ("Mega Drive", "md"),
        ("Mega Drive", "gen"),
        ("Mega Drive", "smd"),
        ("Master System", "sms"),
        ("Game Gear", "gg"),
        ("SG-1000", "sg"),
        ("PC Engine", "pce"),
        ("SuperGrafx", "sgx"),
        ("Atari 2600", "a26"),
        ("Atari 5200", "a52"),
        ("Atari 7800", "a78"),
        ("Atari Jaguar", "j64"),
        ("Atari Jaguar", "jag"),
        ("Atari Lynx", "lnx"),
        ("Neo Geo Pocket", "ngp"),
        ("Neo Geo Pocket Color", "ngc"),
        ("WonderSwan", "ws"),
        ("WonderSwan Color", "wsc"),
        ("ColecoVision", "col"),
        ("ColecoVision", "cv"),
        ("Vectrex", "vec"),
        ("Supervision", "sv"),
        ("MSX", "rom"),
        ("Other cartridge", "bin"),
    ];
    let directory = tempfile::tempdir().unwrap();
    let mut catalogs = vec![];
    let mut originals = vec![];
    for (id, (system, extension)) in representations.into_iter().enumerate() {
        let bytes = format!("synthetic bytes for representation {id}").into_bytes();
        let path = directory
            .path()
            .join(format!("renamed-{id:02}.{}", extension.to_uppercase()));
        std::fs::write(&path, &bytes).unwrap();
        catalogs.push(catalog(system, &format!("Canonical.{extension}"), &bytes));
        originals.push((path, bytes, system));
    }
    let records = scan(directory.path(), catalogs);
    assert_eq!(records.len(), originals.len());
    for (record, (path, bytes, system)) in records.iter().zip(&originals) {
        assert_eq!(record.path.file_name(), path.file_name());
        assert_eq!(
            record.result.status,
            Verification::Exact,
            "{}",
            path.display()
        );
        assert_eq!(record.result.candidates.len(), 1);
        assert_eq!(record.result.candidates[0].system, *system);
        assert_eq!(record.result.candidates[0].checked_algorithms.len(), 4);
        assert_eq!(std::fs::read(path).unwrap(), *bytes);
    }
}

#[test]
fn dat_declared_custom_and_extensionless_representations_need_no_new_system_code() {
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(directory.path().join("renamed.CARTDATA"), b"custom").unwrap();
    std::fs::write(directory.path().join("renamed"), b"bare").unwrap();
    std::fs::write(directory.path().join("unrelated.xyz"), b"other").unwrap();
    let records = scan(
        directory.path(),
        vec![
            catalog("Custom system", "Canonical.cartdata", b"custom"),
            catalog("Custom system", "Canonical", b"bare"),
        ],
    );
    assert_eq!(
        records
            .iter()
            .filter(|r| r.result.status == Verification::Exact)
            .count(),
        2
    );
    // Neither a ROM type nor a catalog size: an Other file, not read.
    let other: Vec<_> = records
        .iter()
        .filter(|r| r.result.status == Verification::Other)
        .collect();
    assert_eq!(other.len(), 1);
    assert!(other[0].digests.is_none());
}

#[test]
fn supported_extensions_without_catalogs_are_unknown_and_never_infer_a_system() {
    let directory = tempfile::tempdir().unwrap();
    // ROM images are binary; text content would be classified as an Other file.
    std::fs::write(directory.path().join("Example.nes"), b"unknown\0").unwrap();
    let records = scan(directory.path(), vec![]);
    assert_eq!(records[0].result.status, Verification::Unknown);
    assert!(records[0].result.candidates.is_empty());
    assert_eq!(records[0].digests, Some(digests(b"unknown\0")));
}

#[test]
fn archives_discs_and_packages_cannot_be_enabled_by_a_dat_extension() {
    let directory = tempfile::tempdir().unwrap();
    let extensions = [
        "zip", "7z", "rar", "iso", "cue", "chd", "rvz", "cia", "nsp", "xci",
    ];
    let mut catalogs = vec![];
    for extension in extensions {
        let filename = format!("Unsupported.{extension}");
        std::fs::write(directory.path().join(&filename), b"container").unwrap();
        catalogs.push(catalog("Container system", &filename, b"container"));
    }
    let records = scan(directory.path(), catalogs);
    assert_eq!(records.len(), extensions.len());
    for record in records {
        assert_eq!(record.result.status, Verification::Unsupported);
        assert!(record.digests.is_none());
        assert!(record.result.explanation.contains("adapter"));
    }
    let index = CatalogIndex::new(vec![catalog(
        "Container system",
        "Canonical.zip",
        b"container",
    )]);
    // Renaming a container to .rom cannot turn a matching container entry into Exact.
    assert_eq!(
        index.identify(9, &digests(b"container")).status,
        Verification::Unsupported
    );
}

#[test]
fn automatic_system_uses_catalog_metadata_and_explicit_custom_overrides_are_retained() {
    let xml = xml("Nintendo - Game Boy Advance", "Canonical.gba", b"abc");
    let inferred = parse_dat(&xml, Path::new("fixture"), "", &Cancellation::default()).unwrap();
    assert_eq!(inferred.provenance.system, "Nintendo - Game Boy Advance");
    let explicit = parse_dat(
        &xml,
        Path::new("fixture"),
        "  Custom / system  ",
        &Cancellation::default(),
    )
    .unwrap();
    assert_eq!(explicit.provenance.system, "Custom / system");
    assert_eq!(
        explicit.provenance.snapshot_sha256,
        inferred.provenance.snapshot_sha256
    );
}

#[test]
fn header_padding_is_not_part_of_the_automatic_system_label() {
    let inferred = catalog("  Custom system  ", "Canonical.rom", b"abc");
    assert_eq!(inferred.provenance.system, "Custom system");
    assert_eq!(inferred.name, "  Custom system  ");
}

#[test]
fn raw_byte_order_and_header_differences_remain_unknown_without_normalization() {
    let index = CatalogIndex::new(vec![catalog("Nintendo 64", "Canonical.z64", b"ABCD")]);
    assert_eq!(
        index.identify(4, &digests(b"BADC")).status,
        Verification::Unknown
    );
    assert_eq!(
        index.identify(8, &digests(b"HEADABCD")).status,
        Verification::Unknown
    );
}

#[test]
fn system_search_and_filter_preserve_cross_system_ambiguity_and_unknown_rows() {
    let index = CatalogIndex::new(vec![
        catalog("SNES", "Canonical.sfc", b"shared"),
        catalog("Mega Drive", "Canonical.md", b"shared"),
    ]);
    let ambiguous = index.identify(6, &digests(b"shared"));
    assert_eq!(ambiguous.status, Verification::Ambiguous);
    let records = vec![
        FileRecord {
            path: "shared.rom".into(),
            size: Some(6),
            digests: Some(digests(b"shared")),
            header: None,
            format: None,
            header_check: None,
            result: ambiguous,
        },
        FileRecord {
            path: "unknown.rom".into(),
            size: Some(7),
            digests: Some(digests(b"unknown")),
            header: None,
            format: None,
            header_check: None,
            result: index.identify(7, &digests(b"unknown")),
        },
    ];
    assert_eq!(query_rows(&records, "sNeS", None, Sort::Name, false), [0]);
    assert_eq!(
        query_rows_in_system(&records, "", None, Some("Mega Drive"), Sort::Name, false),
        [0]
    );
    assert!(
        query_rows_in_system(
            &records,
            "",
            Some(&Verification::Unknown),
            Some("SNES"),
            Sort::Name,
            false
        )
        .is_empty()
    );
    assert_eq!(
        query_rows_in_system(&records, "", None, None, Sort::Name, false),
        [0, 1]
    );
    assert_eq!(records[0].result.status, Verification::Ambiguous);
    assert_eq!(records[0].result.candidates.len(), 2);
}

#[test]
fn cli_auto_import_and_mixed_scan_roundtrip_system_evidence_in_sqlite() {
    let temporary = tempfile::tempdir().unwrap();
    let database = temporary.path().join("collection.sqlite3");
    let roms = temporary.path().join("roms");
    std::fs::create_dir(&roms).unwrap();
    for (system, filename, bytes) in [
        ("NES", "Canonical.nes", &b"nes"[..]),
        ("Mega Drive", "Canonical.md", &b"mega"[..]),
    ] {
        let dat_path = temporary.path().join(format!("{system}.dat"));
        std::fs::write(&dat_path, xml(system, filename, bytes)).unwrap();
        let output = Command::new(env!("CARGO_BIN_EXE_skyline-cli"))
            .arg("import")
            .arg(&database)
            .arg("auto")
            .arg(&dat_path)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        std::fs::write(roms.join(filename), bytes).unwrap();
    }
    let output = Command::new(env!("CARGO_BIN_EXE_skyline-cli"))
        .arg("scan")
        .arg(&database)
        .arg(&roms)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let records: Vec<FileRecord> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(records.len(), 2);
    assert!(
        records
            .iter()
            .all(|r| r.result.status == Verification::Exact)
    );
    let store = Store::open(&database).unwrap();
    assert_eq!(store.catalogs().unwrap().len(), 2);
    assert_eq!(store.collection().unwrap().len(), 2);
    assert!(
        store
            .collection()
            .unwrap()
            .iter()
            .any(|r| r.result.candidates[0].system == "Mega Drive")
    );
}

#[test]
fn automatic_system_catalog_snapshot_keeps_provenance_on_reopen() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("provenance.sqlite3");
    let snapshot = xml("WonderSwan", "Canonical.ws", b"abc");
    let imported = ImportedCatalog {
        catalog: catalog("WonderSwan", "Canonical.ws", b"abc"),
        snapshot,
    };
    let mut store = Store::open(&path).unwrap();
    store.import_catalog(&imported).unwrap();
    drop(store);
    let store = Store::open(&path).unwrap();
    let restored = &store.catalogs().unwrap()[0];
    assert_eq!(restored.provenance.system, "WonderSwan");
    assert_eq!(
        restored.provenance.snapshot_sha256,
        imported.catalog.provenance.snapshot_sha256
    );
    assert_eq!(
        store
            .snapshot(&restored.provenance.snapshot_sha256)
            .unwrap(),
        imported.snapshot
    );
}

fn headered_scan(file: &str, header: &[u8], content: &[u8], entry: &str) -> FileRecord {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join(file);
    let bytes = [header, content].concat();
    std::fs::write(&path, &bytes).unwrap();
    let mut records = scan(
        directory.path(),
        vec![catalog("Headerless", entry, content)],
    );
    assert_eq!(
        std::fs::read(&path).unwrap(),
        bytes,
        "source must be unchanged"
    );
    records.remove(0)
}

#[test]
fn recognized_headers_are_excluded_for_comparison_only() {
    let content: Vec<u8> = (0..40_960_u32).map(|i| (i * 7) as u8).collect();
    let ines = [b"NES\x1a\x02\x01".as_slice(), &[0; 10]].concat();
    let record = headered_scan("Game.nes", &ines, &content, "Game.nes");
    assert_eq!(record.result.status, Verification::Exact);
    assert!(record.result.explanation.contains("iNES"));
    let header = record.header.unwrap();
    assert_eq!((header.header_bytes, header.size), (16, 40_960));
    assert_eq!(record.size, Some(40_976), "whole-file evidence is kept");

    let mut a78 = vec![0_u8; 128];
    a78[1..10].copy_from_slice(b"ATARI7800");
    assert_eq!(
        headered_scan("Game.a78", &a78, &content, "Game.a78")
            .result
            .status,
        Verification::Exact
    );
    let mut lynx = vec![0_u8; 64];
    lynx[..5].copy_from_slice(b"LYNX\0");
    assert_eq!(
        headered_scan("Game.lnx", &lynx, &content, "Game.lnx")
            .result
            .status,
        Verification::Exact
    );
    // SNES copier headers have no signature: only size (1 KiB multiple + 512) marks them.
    let snes: Vec<u8> = content[..32_768].to_vec();
    for extension in ["smc", "sfc", "swc", "fig"] {
        assert_eq!(
            headered_scan(&format!("Game.{extension}"), &[0; 512], &snes, "Game.sfc")
                .result
                .status,
            Verification::Exact,
            "headerless DAT must accept the supported SNES alias {extension}"
        );
    }
}

#[test]
fn header_comparison_preserves_all_digests_across_short_reads_and_buffer_boundaries() {
    use skyline_core::{formats::detect_header, hash::hash_stream_with_header};
    use std::io::{self, Read};

    struct ShortReads<'a>(&'a [u8]);
    impl Read for ShortReads<'_> {
        fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
            let count = self.0.len().min(buffer.len()).min(7);
            buffer[..count].copy_from_slice(&self.0[..count]);
            self.0 = &self.0[count..];
            Ok(count)
        }
    }

    let header = [b"NES\x1a".as_slice(), &[0; 12]].concat();
    let content: Vec<u8> = (0..150_000_u32).map(|i| (i * 13) as u8).collect();
    let bytes = [header, content.clone()].concat();
    // Large reads cross the 64 KiB hashing boundary; short reads split the signature
    // and header. Both must yield identical whole-file and content-only evidence.
    for reader in [
        Box::new(bytes.as_slice()) as Box<dyn Read + '_>,
        Box::new(ShortReads(&bytes)),
    ] {
        let mut progress = vec![];
        let hashed = hash_stream_with_header(
            reader,
            &Cancellation::default(),
            |size| progress.push(size),
            |prefix| detect_header(prefix, "nes", bytes.len() as u64),
        )
        .unwrap();
        assert_eq!(hashed.size, bytes.len() as u64);
        assert_eq!(hashed.digests, digests(&bytes));
        let evidence = hashed.header.unwrap();
        assert_eq!(evidence.header_bytes, 16);
        assert_eq!(evidence.size, content.len() as u64);
        assert_eq!(evidence.digests, digests(&content));
        assert_eq!(progress.last(), Some(&hashed.size));
        assert!(progress.windows(2).all(|pair| pair[0] < pair[1]));
    }
}

#[test]
fn whole_file_match_wins_and_unmatched_headers_explain_both_comparisons() {
    let content = vec![9_u8; 8192];
    let ines = [b"NES\x1a".as_slice(), &[0; 12]].concat();
    let whole = [ines.as_slice(), &content].concat();
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(directory.path().join("Game.nes"), &whole).unwrap();
    // A catalog that lists the headered file still matches the whole file.
    let records = scan(
        directory.path(),
        vec![catalog("Headered", "Game.nes", &whole)],
    );
    assert_eq!(records[0].result.status, Verification::Exact);
    assert!(!records[0].result.explanation.contains("without the"));
    // No match either way: Unknown, and the explanation says both were tried.
    let records = scan(directory.path(), vec![catalog("Other", "Other.nes", b"x")]);
    assert_eq!(records[0].result.status, Verification::Unknown);
    assert!(
        records[0]
            .result
            .explanation
            .contains("Also compared without the iNES")
    );
    // A file that is only a header has no content to compare.
    std::fs::write(directory.path().join("Game.nes"), &ines).unwrap();
    assert!(
        scan(directory.path(), vec![])
            .pop()
            .unwrap()
            .header
            .is_none()
    );
}

#[test]
fn extensionless_files_are_read_only_when_their_size_matches_a_catalog_entry() {
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(directory.path().join("Game"), b"bare").unwrap();
    std::fs::write(directory.path().join("LICENSE"), b"MIT License text").unwrap();
    std::fs::create_dir(directory.path().join(".git")).unwrap();
    std::fs::write(directory.path().join(".git/HEAD"), b"ref: main").unwrap();
    let records = scan(
        directory.path(),
        vec![catalog("Custom system", "Canonical", b"bare")],
    );
    let read: Vec<_> = records
        .iter()
        .filter(|r| r.digests.is_some())
        .map(|r| r.display_name())
        .collect();
    assert_eq!(read, ["Game"]);
    assert_eq!(
        records.len(),
        3,
        "skipped files stay visible as Unsupported"
    );
    // Without any extensionless catalog entry, extensionless files are never read.
    let records = scan(directory.path(), vec![]);
    assert!(records.iter().all(|r| r.digests.is_none()));
}

#[test]
fn readmes_artwork_and_saves_are_other_files_not_unknown_roms() {
    use skyline_core::{
        systems::collection_systems,
        view::{other_files, query_rows},
    };
    let directory = tempfile::tempdir().unwrap();
    let write =
        |name: &str, bytes: &[u8]| std::fs::write(directory.path().join(name), bytes).unwrap();
    write("notes.md", b"# My collection\n\nSorted by region.\n");
    write("readme.txt", b"Thanks for downloading");
    write("boxart.png", b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR\0\0\x01\0");
    write("Game.sav", &[0_u8; 8192]);
    // A text file whose size happens to equal a catalog entry is read, still Other.
    write("manual.txt", b"exactly the size");
    write(
        "Unknown.md",
        &[0x00, 0xff, 0x10, 0x00, 0x53, 0x45, 0x47, 0x41],
    );
    // A DAT that lists .txt/.png/.sav files does not make them ROM-like.
    let pc = catalog(
        "IBM - PC and Compatibles (Digital)",
        "Game/readme.txt",
        b"16 bytes of text",
    );
    let save = catalog("Mobile - Palm OS (Digital)", "Game.sav", b"save");
    let index = CatalogIndex::new(vec![pc, save]);
    let records = scan_folder(
        directory.path(),
        &index,
        &ScanOptions::default(),
        &Cancellation::default(),
        |_| {},
    )
    .unwrap()
    .records;
    let status = |name: &str| {
        let record = records.iter().find(|r| r.display_name() == name).unwrap();
        (record.result.status.clone(), record.digests.is_some())
    };
    assert_eq!(
        status("notes.md"),
        (Verification::Other, true),
        "text, despite .md"
    );
    assert_eq!(
        status("readme.txt"),
        (Verification::Other, false),
        "not read"
    );
    assert_eq!(status("boxart.png"), (Verification::Other, false));
    assert_eq!(status("Game.sav"), (Verification::Other, false));
    assert_eq!(
        status("manual.txt"),
        (Verification::Other, true),
        "size matched"
    );
    assert_eq!(
        status("Unknown.md"),
        (Verification::Unknown, true),
        "binary .md"
    );
    // Only the binary .md is suggested as a Mega Drive file.
    let systems = collection_systems(&records, &index.summary());
    assert_eq!(systems.len(), 1);
    assert_eq!(
        (systems[0].name.as_str(), systems[0].suggested_files),
        ("Mega Drive", 1)
    );
    // Other files are hidden unless asked for.
    assert_eq!(other_files(&records), 5);
    assert_eq!(query_rows(&records, "", None, Sort::Name, false).len(), 1);
    assert_eq!(
        query_rows(&records, "", Some(&Verification::Other), Sort::Name, false).len(),
        5
    );
}

#[test]
fn cli_scan_can_use_one_dat_folder_without_legacy_manual_imports() {
    let temp = tempfile::tempdir().unwrap();
    let database = temp.path().join("cli.sqlite3");
    let dats = temp.path().join("dats");
    let roms = temp.path().join("roms");
    std::fs::create_dir_all(&dats).unwrap();
    std::fs::create_dir_all(&roms).unwrap();
    std::fs::write(roms.join("Game.gb"), b"\0game").unwrap();
    // The same bytes under a legacy manual label and in the folder's DAT.
    let legacy = temp.path().join("legacy.dat");
    std::fs::write(&legacy, xml("Nintendo - Game Boy", "Game.gb", b"\0game")).unwrap();
    std::fs::write(
        dats.join("gb.dat"),
        xml("Nintendo - Game Boy (Updated)", "Game.gb", b"\0game"),
    )
    .unwrap();
    let cli = env!("CARGO_BIN_EXE_skyline-cli");
    let run = |args: &[&std::ffi::OsStr]| {
        let output = Command::new(cli).args(args).output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        output.stdout
    };
    run(&[
        "import".as_ref(),
        database.as_os_str(),
        "Game Boy".as_ref(),
        legacy.as_os_str(),
    ]);
    run(&[
        "import-folder".as_ref(),
        database.as_os_str(),
        dats.as_os_str(),
    ]);
    let status = |stdout: Vec<u8>| {
        let records: Vec<FileRecord> = serde_json::from_slice(&stdout).unwrap();
        records[0].result.status.clone()
    };
    // All catalogs: two identities (legacy label and folder) disagree.
    let all = run(&["scan".as_ref(), database.as_os_str(), roms.as_os_str()]);
    assert_eq!(status(all), Verification::Ambiguous);
    let folder = run(&[
        "scan".as_ref(),
        database.as_os_str(),
        roms.as_os_str(),
        "--dats".as_ref(),
        dats.as_os_str(),
    ]);
    assert_eq!(status(folder), Verification::Exact);
}
