use skyline_core::{
    catalog::parse_dat,
    formats::header_notes,
    hash::{Cancellation, hash_stream},
    matching::CatalogIndex,
    model::{FileRecord, Verification},
    scan::{ScanOptions, scan_folder},
};
use std::path::Path;

fn scan(
    filename: &str,
    bytes: &[u8],
    catalog_name: &str,
    expected: &[u8],
    reference_header: &str,
) -> FileRecord {
    let temp = tempfile::tempdir().unwrap();
    std::fs::write(temp.path().join(filename), bytes).unwrap();
    let (_, hashes) = hash_stream(expected, &Cancellation::default(), |_| {}).unwrap();
    let xml = format!(
        "<datafile><header><name>{catalog_name}</name></header><game name=\"Synthetic\"><rom name=\"{filename}\" size=\"{}\" sha256=\"{}\" header=\"{reference_header}\"/></game></datafile>",
        expected.len(),
        hashes.sha256.unwrap()
    );
    let catalog = parse_dat(
        xml.as_bytes(),
        Path::new("header.dat"),
        "",
        &Cancellation::default(),
    )
    .unwrap();
    let report = scan_folder(
        temp.path(),
        &CatalogIndex::new(vec![catalog]),
        &ScanOptions::default(),
        &Cancellation::default(),
        |_| {},
    )
    .unwrap();
    assert_eq!(std::fs::read(temp.path().join(filename)).unwrap(), bytes);
    report.records.into_iter().next().unwrap()
}

#[test]
fn headered_nes_metadata_verifies_whole_bytes_and_headerless_content_keeps_emulator_note() {
    let header = [b"NES\x1a".as_slice(), &[0; 12]].concat();
    let content = vec![42; 1024];
    let full = [header, content.clone()].concat();
    let record = scan(
        "Game.nes",
        &full,
        "Nintendo - Nintendo Entertainment System (Headered)",
        &full,
        "4E 45 53 1A",
    );
    assert_eq!(record.result.status, Verification::Exact);
    assert!(!record.result.explanation.starts_with("Compared without"));
    assert!(
        header_notes(&record)
            .iter()
            .any(|note| note.contains("expects headered"))
    );
    assert_eq!(
        record.result.candidates[0]
            .entry
            .reference_header
            .as_deref(),
        Some("4E 45 53 1A")
    );
    let record = scan(
        "Game.nes",
        &full,
        "Nintendo - Nintendo Entertainment System (Headerless)",
        &content,
        "!none",
    );
    assert_eq!(record.result.status, Verification::Exact);
    assert!(
        header_notes(&record)
            .iter()
            .any(|note| note.contains("Only the header-free content matched"))
    );
    assert!(
        header_notes(&record)
            .iter()
            .any(|note| note.contains("not a recommendation to remove"))
    );
    let bare = scan(
        "Game.nes",
        &content,
        "Nintendo - Nintendo Entertainment System (Headerless)",
        &content,
        "!none",
    );
    assert!(bare.header.is_none());
    assert!(
        header_notes(&bare)
            .iter()
            .any(|note| note.contains("most NES emulators need"))
    );
}

#[test]
fn snes_copier_assessment_keeps_internal_metadata_and_fds_header_is_comparison_only() {
    let content = vec![23; 1024];
    let full = [vec![0; 512], content.clone()].concat();
    let record = scan(
        "Game.smc",
        &full,
        "Nintendo - Super Nintendo Entertainment System",
        &content,
        "!none",
    );
    assert_eq!(record.result.status, Verification::Exact);
    assert!(
        header_notes(&record)
            .iter()
            .any(|note| note.contains("heuristic"))
    );
    assert!(
        header_notes(&record)
            .iter()
            .any(|note| note.contains("internal"))
    );
    let fds = [b"FDS\x1a".as_slice(), &[0; 12], &content].concat();
    let record = scan(
        "Game.fds",
        &fds,
        "Nintendo - Family Computer Disk System (FDS)",
        &content,
        "!none",
    );
    assert_eq!(record.result.status, Verification::Exact);
    assert_eq!(record.header.unwrap().header_bytes, 16);
}

#[test]
fn nes_trainer_is_not_removed_and_n64_signature_describes_byte_layout() {
    let mut header = [0; 16];
    header[..4].copy_from_slice(b"NES\x1a");
    header[6] = 4;
    let content = [vec![7; 512], vec![9; 1024]].concat();
    let full = [&header[..], &content].concat();
    let record = scan("Game.nes", &full, "NES (Headerless)", &content, "!none");
    assert_eq!(record.result.status, Verification::Exact);
    assert_eq!(record.header.as_ref().unwrap().header_bytes, 16);
    assert!(
        header_notes(&record)
            .iter()
            .any(|note| note.contains("Trainer bytes are retained"))
    );
    for (prefix, label) in [
        ([0x80, 0x37, 0x12, 0x40], "big-endian"),
        ([0x37, 0x80, 0x40, 0x12], "byte-swapped"),
        ([0x40, 0x12, 0x37, 0x80], "little-endian"),
    ] {
        let full = [&prefix[..], &[5; 1024]].concat();
        let record = scan("Game.n64", &full, "Nintendo 64", &full, "");
        assert_eq!(record.result.status, Verification::Exact);
        assert!(record.header.is_none());
        assert!(record.format.unwrap().name.contains(label));
    }
}
