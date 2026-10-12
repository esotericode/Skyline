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

/// A No-Intro-style pair: the Headered set hashes the whole file and records the
/// canonical header bytes; the Headerless set hashes only the content (`.unh`).
fn nes_pair(content: &[u8], canonical: &[u8]) -> Vec<skyline_core::model::Catalog> {
    let catalog = |name: &str, rom: String| {
        parse_dat(
            format!("<datafile><header><name>{name}</name></header><game name=\"Synthetic (World)\">{rom}</game></datafile>").as_bytes(),
            Path::new("nes.dat"),
            "",
            &Cancellation::default(),
        )
        .unwrap()
    };
    let headered = [canonical, content].concat();
    let (_, whole) = hash_stream(&headered[..], &Cancellation::default(), |_| {}).unwrap();
    let (_, bare) = hash_stream(content, &Cancellation::default(), |_| {}).unwrap();
    let hex = canonical
        .iter()
        .map(|byte| format!("{byte:02X}"))
        .collect::<Vec<_>>()
        .join(" ");
    vec![
        catalog(
            "Nintendo - Nintendo Entertainment System (Headered)",
            format!(
                "<rom name=\"Synthetic (World).nes\" size=\"{}\" sha1=\"{}\" header=\"{hex}\"/>",
                headered.len(),
                whole.sha1.unwrap()
            ),
        ),
        catalog(
            "Nintendo - Nintendo Entertainment System (Headerless)",
            format!(
                "<rom name=\"Synthetic (World).unh\" size=\"{}\" sha1=\"{}\"/>",
                content.len(),
                bare.sha1.unwrap()
            ),
        ),
    ]
}

fn scan_with(catalogs: Vec<skyline_core::model::Catalog>, bytes: &[u8]) -> FileRecord {
    let temp = tempfile::tempdir().unwrap();
    std::fs::write(temp.path().join("Game.nes"), bytes).unwrap();
    scan_folder(
        temp.path(),
        &CatalogIndex::new(catalogs),
        &ScanOptions::default(),
        &Cancellation::default(),
        |_| {},
    )
    .unwrap()
    .records
    .remove(0)
}

#[test]
fn headers_are_compared_with_the_canonical_header_of_the_same_game() {
    let content: Vec<u8> = (0..24_576_u32).map(|i| (i * 31) as u8).collect();
    let canonical = [
        b"NES\x1a".as_slice(),
        &[1, 1, 0x10, 0x08, 0, 0, 0, 0, 0, 0, 0, 1],
    ]
    .concat();

    // Canonical header: whole-file match, header confirmed.
    let record = scan_with(
        nes_pair(&content, &canonical),
        &[canonical.clone(), content.clone()].concat(),
    );
    assert_eq!(record.result.status, Verification::Exact);
    let check = record.header_check.unwrap();
    assert!(check.differences.is_empty());
    assert_eq!(check.actual.as_deref(), Some(check.expected.as_str()));

    // Dirty header (old dumps often differ in flag bytes): content matches the
    // Headerless set, and the comparison names the differing bytes.
    let mut dirty = canonical.clone();
    dirty[7] = 0x00;
    dirty[15] = 0x00;
    let record = scan_with(
        nes_pair(&content, &canonical),
        &[dirty, content.clone()].concat(),
    );
    assert_eq!(record.result.status, Verification::Exact);
    assert!(
        record
            .result
            .explanation
            .starts_with("Compared without the iNES")
    );
    let check = record.header_check.unwrap();
    assert_eq!(check.differences, [7, 15]);
    assert!(check.catalog.contains("(Headered)"));

    // No header at all: the content matches, and the canonical header is offered.
    let record = scan_with(nes_pair(&content, &canonical), &content);
    assert_eq!(record.result.status, Verification::Exact);
    let check = record.header_check.unwrap();
    assert_eq!(check.actual, None);
    assert!(check.expected.starts_with("4E 45 53 1A"));

    // Free-text header metadata (serials, "!none") is never treated as bytes.
    let text = scan(
        "Game.md",
        &content,
        "Sega - Mega Drive - Genesis",
        &content,
        "T-50086-00",
    );
    assert_eq!(text.result.status, Verification::Exact);
    assert!(text.header_check.is_none());
}
