use skyline_core::{
    catalog::parse_dat,
    hash::{Cancellation, hash_stream},
    matching::CatalogIndex,
    model::{Catalog, FileRecord, Verification},
    scan::{ScanOptions, ScanReport, scan_folder},
};
use std::{io::Write, path::Path};
use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

fn catalog(system: &str, entries: &[(&str, &[u8])]) -> Catalog {
    let roms: String = entries
        .iter()
        .map(|(name, bytes)| {
            let (_, h) = hash_stream(*bytes, &Cancellation::default(), |_| {}).unwrap();
            format!(
                "<game name=\"{name}\"><rom name=\"{name}\" size=\"{}\" crc=\"{}\" sha1=\"{}\"/></game>",
                bytes.len(),
                h.crc32.unwrap(),
                h.sha1.unwrap()
            )
        })
        .collect();
    parse_dat(
        format!("<datafile><header><name>{system}</name></header>{roms}</datafile>").as_bytes(),
        Path::new("zip.dat"),
        "",
        &Cancellation::default(),
    )
    .unwrap()
}

/// Members as (name, bytes, compression); a trailing `/` makes a directory entry.
fn zip_bytes(members: &[(&str, &[u8], CompressionMethod)]) -> Vec<u8> {
    let mut writer = ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for (name, bytes, method) in members {
        let options = SimpleFileOptions::default().compression_method(*method);
        if name.ends_with('/') {
            writer.add_directory(*name, options).unwrap();
        } else {
            writer.start_file(*name, options).unwrap();
            writer.write_all(bytes).unwrap();
        }
    }
    writer.finish().unwrap().into_inner()
}

fn scan(folder: &Path, catalogs: Vec<Catalog>) -> ScanReport {
    scan_folder(
        folder,
        &CatalogIndex::new(catalogs),
        &ScanOptions::default(),
        &Cancellation::default(),
        |_| {},
    )
    .unwrap()
}

fn rom(seed: u8, size: usize) -> Vec<u8> {
    (0..size)
        .map(|i| (i as u8).wrapping_mul(31) ^ seed)
        .collect()
}

fn find(records: &[FileRecord], member: &str) -> FileRecord {
    records
        .iter()
        .find(|r| r.member.as_deref() == Some(member))
        .unwrap_or_else(|| panic!("no record for {member}"))
        .clone()
}

#[test]
fn rom_inside_a_zip_matches_like_a_loose_file_without_changing_the_archive() {
    let temp = tempfile::tempdir().unwrap();
    let game = rom(1, 32_768);
    let archive = zip_bytes(&[("Tetris (World).gb", &game, CompressionMethod::Deflated)]);
    let path = temp.path().join("Tetris (World).zip");
    std::fs::write(&path, &archive).unwrap();
    let report = scan(
        temp.path(),
        vec![catalog(
            "Nintendo - Game Boy",
            &[("Tetris (World).gb", &game)],
        )],
    );
    assert_eq!(report.records.len(), 1);
    let record = &report.records[0];
    assert_eq!(record.result.status, Verification::Exact);
    assert_eq!(record.member.as_deref(), Some("Tetris (World).gb"));
    assert_eq!(record.size, Some(32_768));
    // A one-ROM ZIP named after its ROM shows just the archive name.
    assert_eq!(record.display_name(), "Tetris (World).zip");
    assert_eq!(
        report.hashed_bytes, 32_768,
        "uncompressed bytes are counted"
    );
    assert_eq!(
        std::fs::read(&path).unwrap(),
        archive,
        "archive is unchanged"
    );
}

#[test]
fn members_follow_the_loose_file_rules() {
    let temp = tempfile::tempdir().unwrap();
    let game = rom(2, 65_536);
    let nfo = b"Scene release notes\nEnjoy!\n".repeat(4);
    let inner = zip_bytes(&[("x.gb", b"x", CompressionMethod::Stored)]);
    let archive = zip_bytes(&[
        ("Pack/", b"", CompressionMethod::Stored),
        ("Pack/Game (USA).gba", &game, CompressionMethod::Deflated),
        ("Pack/readme.nfo", &nfo, CompressionMethod::Deflated),
        ("Pack/cover.png", &rom(3, 5_000), CompressionMethod::Stored),
        ("Pack/inner.zip", &inner, CompressionMethod::Stored),
    ]);
    std::fs::write(temp.path().join("Pack.zip"), archive).unwrap();
    let report = scan(
        temp.path(),
        vec![catalog(
            "Nintendo - Game Boy Advance",
            &[("Game (USA).gba", &game)],
        )],
    );
    let records = &report.records;
    assert_eq!(records.len(), 4, "directories are not records");
    let game = find(records, "Pack/Game (USA).gba");
    assert_eq!(game.result.status, Verification::Exact);
    assert_eq!(game.display_name(), "Pack.zip › Pack/Game (USA).gba");
    let readme = find(records, "Pack/readme.nfo");
    assert_eq!(
        (readme.result.status, readme.digests.is_some()),
        (Verification::Other, false)
    );
    assert_eq!(
        find(records, "Pack/cover.png").result.status,
        Verification::Other
    );
    assert_eq!(
        find(records, "Pack/inner.zip").result.status,
        Verification::Unsupported
    );
}

#[test]
fn headered_nes_rom_in_a_zip_matches_its_headerless_content() {
    let temp = tempfile::tempdir().unwrap();
    let content = rom(4, 40_960);
    let headered = [b"NES\x1a\x02\x01\x00\x00".as_slice(), &[0; 8], &content].concat();
    std::fs::write(
        temp.path().join("Game.zip"),
        zip_bytes(&[("Game.nes", &headered, CompressionMethod::Deflated)]),
    )
    .unwrap();
    let report = scan(
        temp.path(),
        vec![catalog(
            "Nintendo - Nintendo Entertainment System (Headerless)",
            &[("Game.unh", &content)],
        )],
    );
    let record = &report.records[0];
    assert_eq!(record.result.status, Verification::Exact);
    assert!(
        record
            .result
            .explanation
            .starts_with("Compared without the iNES")
    );
    assert_eq!(record.header.as_ref().unwrap().header_bytes, 16);
}

/// Patches a little-endian field in the first local header and the central directory.
fn patch(archive: &mut [u8], local_offset: usize, central_offset: usize, value: &[u8]) {
    let local = archive.windows(4).position(|w| w == b"PK\x03\x04").unwrap();
    let central = archive.windows(4).position(|w| w == b"PK\x01\x02").unwrap();
    archive[local + local_offset..][..value.len()].copy_from_slice(value);
    archive[central + central_offset..][..value.len()].copy_from_slice(value);
}

#[test]
fn damaged_and_unusual_members_become_clear_records_and_the_scan_continues() {
    let temp = tempfile::tempdir().unwrap();
    let game = rom(5, 4_096);
    let stored = || zip_bytes(&[("Game.gb", &game, CompressionMethod::Stored)]);
    // Flipped data byte: the CRC check at the end of the member fails.
    let mut damaged = stored();
    let data = damaged.windows(16).position(|w| w == &game[..16]).unwrap();
    damaged[data + 100] ^= 0xff;
    std::fs::write(temp.path().join("damaged.zip"), damaged).unwrap();
    // Declared size smaller than the data: never decompressed past the declaration.
    let mut lying = stored();
    patch(&mut lying, 22, 24, &1024_u32.to_le_bytes());
    std::fs::write(temp.path().join("lying.zip"), lying).unwrap();
    // Compression method 12 (bzip2) is not enabled.
    let mut bzip2 = stored();
    patch(&mut bzip2, 8, 10, &12_u16.to_le_bytes());
    std::fs::write(temp.path().join("bzip2.zip"), bzip2).unwrap();
    // General-purpose flag bit 0: encrypted.
    let mut encrypted = stored();
    patch(&mut encrypted, 6, 8, &1_u16.to_le_bytes());
    std::fs::write(temp.path().join("encrypted.zip"), encrypted).unwrap();
    std::fs::write(temp.path().join("not really.zip"), b"This is a text file").unwrap();
    std::fs::write(temp.path().join("empty.zip"), zip_bytes(&[])).unwrap();
    std::fs::write(temp.path().join("Good.zip"), stored()).unwrap();
    let report = scan(
        temp.path(),
        vec![catalog("Nintendo - Game Boy", &[("Game.gb", &game)])],
    );
    let by_archive = |name: &str| {
        report
            .records
            .iter()
            .find(|r| r.path.file_name().unwrap() == name)
            .unwrap()
            .result
            .clone()
    };
    let damaged = by_archive("damaged.zip");
    assert_eq!(damaged.status, Verification::ReadError);
    assert!(
        damaged.explanation.contains("CRC"),
        "{}",
        damaged.explanation
    );
    let lying = by_archive("lying.zip");
    assert_eq!(lying.status, Verification::ReadError);
    assert!(
        lying.explanation.contains("declares 1024"),
        "{}",
        lying.explanation
    );
    let bzip2 = by_archive("bzip2.zip");
    assert_eq!(bzip2.status, Verification::Unsupported);
    assert!(
        bzip2.explanation.contains("Compression method 12"),
        "{}",
        bzip2.explanation
    );
    let encrypted = by_archive("encrypted.zip");
    assert_eq!(encrypted.status, Verification::Unsupported);
    assert!(
        encrypted.explanation.contains("Encrypted"),
        "{}",
        encrypted.explanation
    );
    let fake = by_archive("not really.zip");
    assert_eq!(fake.status, Verification::ReadError);
    assert!(fake.explanation.starts_with("Not a readable ZIP"));
    assert_eq!(by_archive("empty.zip").status, Verification::Other);
    assert_eq!(
        by_archive("Good.zip").status,
        Verification::Exact,
        "the scan continued"
    );
}

#[test]
fn cancellation_inside_a_zip_stops_the_scan() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::write(
        temp.path().join("Big.zip"),
        zip_bytes(&[("Big.gba", &rom(6, 1 << 20), CompressionMethod::Deflated)]),
    )
    .unwrap();
    let cancel = Cancellation::default();
    let report = scan_folder(
        temp.path(),
        &CatalogIndex::new(vec![]),
        &ScanOptions::default(),
        &cancel,
        |progress| {
            if progress.hashed_bytes > 0 {
                cancel.cancel();
            }
        },
    )
    .unwrap();
    assert!(report.cancelled);
    assert!(report.records.is_empty());
}

#[test]
fn identical_member_bytes_give_equivalent_results_whatever_the_zip() {
    let temp = tempfile::tempdir().unwrap();
    let game = rom(7, 131_072);
    std::fs::write(
        temp.path().join("Stored.zip"),
        zip_bytes(&[("Game.gb", &game, CompressionMethod::Stored)]),
    )
    .unwrap();
    std::fs::write(
        temp.path().join("Deflated.zip"),
        zip_bytes(&[
            (
                "notes.txt",
                b"other member first",
                CompressionMethod::Deflated,
            ),
            ("sub/Game.gb", &game, CompressionMethod::Deflated),
        ]),
    )
    .unwrap();
    let report = scan(
        temp.path(),
        vec![catalog("Nintendo - Game Boy", &[("Game.gb", &game)])],
    );
    let stored = find(&report.records, "Game.gb");
    let deflated = find(&report.records, "sub/Game.gb");
    assert_eq!(stored.result.status, Verification::Exact);
    assert_eq!(stored.result.status, deflated.result.status);
    assert_eq!(stored.digests, deflated.digests);
    assert_eq!(stored.size, deflated.size);
    assert_eq!(
        serde_json::to_value(&stored.result.candidates).unwrap(),
        serde_json::to_value(&deflated.result.candidates).unwrap()
    );
}

#[test]
fn implausibly_large_members_are_refused_before_decompression() {
    let temp = tempfile::tempdir().unwrap();
    let game = rom(8, 4_096);
    // Claims ~4 GB from 4 KiB stored: refused without reading, as a possible bomb.
    let mut bomb = zip_bytes(&[("Bomb.gb", &game, CompressionMethod::Stored)]);
    patch(&mut bomb, 22, 24, &4_000_000_000_u32.to_le_bytes());
    std::fs::write(temp.path().join("bomb.zip"), bomb).unwrap();
    let report = scan(temp.path(), vec![]);
    let record = &report.records[0];
    assert_eq!(record.result.status, Verification::Unsupported);
    assert!(record.result.explanation.contains("decompression bomb"));
    assert_eq!(report.hashed_bytes, 0);
}
